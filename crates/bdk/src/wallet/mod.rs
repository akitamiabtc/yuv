// Bitcoin Dev Kit
// Written in 2020 by Alekos Filini <alekos.filini@gmail.com>
//
// Copyright (c) 2020-2021 Bitcoin Dev Kit Developers
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! Wallet
//!
//! This module defines the [`Wallet`] structure.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use bitcoin::secp256k1::Secp256k1;

use bitcoin::consensus::encode::serialize;
use bitcoin::psbt;
use bitcoin::sighash::{EcdsaSighashType, TapSighashType};
use bitcoin::{
    absolute, Address, Network, OutPoint, Script, ScriptBuf, Sequence, Transaction, TxOut, Txid,
    Weight, Witness,
};

use miniscript::psbt::{PsbtExt, PsbtInputExt, PsbtInputSatisfier};

#[allow(unused_imports)]
use log::{debug, error, info, trace};

#[allow(missing_docs)]
pub mod coin_selection;
pub mod export;
pub mod signer;
pub mod time;
#[allow(missing_docs)]
pub mod tx_builder;
pub(crate) mod utils;
#[cfg(feature = "verify")]
#[cfg_attr(docsrs, doc(cfg(feature = "verify")))]
pub mod verify;

pub use utils::IsDust;

use coin_selection::DefaultCoinSelectionAlgorithm;
use signer::{SignOptions, SignerOrdering, SignersContainer, TransactionSigner};
use tx_builder::{BumpFee, CreateTx, FeePolicy, TxBuilder, TxParams};
use utils::{check_nsequence_rbf, After, Older, SecpCtx};

use crate::blockchain::{GetHeight, NoopProgress, Progress, WalletSync};
use crate::database::{BatchDatabase, BatchOperations, DatabaseUtils, SyncTime};
use crate::descriptor::checksum::calc_checksum_bytes_internal;
use crate::descriptor::policy::BuildSatisfaction;
use crate::descriptor::{
    calc_checksum, into_wallet_descriptor_checked, DerivedDescriptor, DescriptorMeta,
    ExtendedDescriptor, ExtractPolicy, IntoWalletDescriptor, Policy, XKeyUtils,
};
use crate::error::{Error, MiniscriptPsbtError};
use crate::psbt::PsbtUtils;
use crate::signer::SignerError;
use crate::types::*;
use crate::wallet::coin_selection::Excess::{Change, NoChange};

const CACHE_ADDR_BATCH_SIZE: u32 = 100;
const COINBASE_MATURITY: u32 = 100;

/// A Bitcoin wallet
///
/// The `Wallet` struct acts as a way of coherently interfacing with output descriptors and related transactions.
/// Its main components are:
///
/// 1. output *descriptors* from which it can derive addresses.
/// 2. A [`Database`] where it tracks transactions and utxos related to the descriptors.
/// 3. [`signer`]s that can contribute signatures to addresses instantiated from the descriptors.
///
/// [`Database`]: crate::database::Database
/// [`signer`]: crate::signer
#[derive(Debug)]
pub struct Wallet<D> {
    descriptor: ExtendedDescriptor,
    change_descriptor: Option<ExtendedDescriptor>,

    signers: Arc<SignersContainer>,
    change_signers: Arc<SignersContainer>,

    network: Network,

    database: RefCell<D>,

    secp: SecpCtx,
}

/// The address index selection strategy to use to derived an address from the wallet's external
/// descriptor. See [`Wallet::get_address`]. If you're unsure which one to use use `WalletIndex::New`.
#[derive(Debug)]
pub enum AddressIndex {
    /// Return a new address after incrementing the current descriptor index.
    New,
    /// Return the address for the current descriptor index if it has not been used in a received
    /// transaction. Otherwise return a new address as with [`AddressIndex::New`].
    ///
    /// Use with caution, if the wallet has not yet detected an address has been used it could
    /// return an already used address. This function is primarily meant for situations where the
    /// caller is untrusted; for example when deriving donation addresses on-demand for a public
    /// web page.
    LastUnused,
    /// Return the address for a specific descriptor index. Does not change the current descriptor
    /// index used by `AddressIndex::New` and `AddressIndex::LastUsed`. The index must be non-hardened,
    /// i.e., < 2**31.
    ///
    /// Use with caution, if an index is given that is less than the current descriptor index
    /// then the returned address may have already been used.
    Peek(u32),
    /// Return the address for a specific descriptor index and reset the current descriptor index
    /// used by `AddressIndex::New` and `AddressIndex::LastUsed` to this value. The index must be
    /// non-hardened, i.e. < 2**31
    ///
    /// Use with caution, if an index is given that is less than the current descriptor index
    /// then the returned address and subsequent addresses returned by calls to `AddressIndex::New`
    /// and `AddressIndex::LastUsed` may have already been used. Also if the index is reset to a
    /// value earlier than the [`crate::blockchain::Blockchain`] stop_gap (default is 20) then a
    /// larger stop_gap should be used to monitor for all possibly used addresses.
    Reset(u32),
}

/// A derived address and the index it was found at.
/// For convenience this automatically derefs to `Address`
#[derive(Debug, PartialEq, Eq)]
pub struct AddressInfo {
    /// Child index of this address
    pub index: u32,
    /// Address
    pub address: Address,
    /// Type of keychain
    pub keychain: KeychainKind,
}

impl Deref for AddressInfo {
    type Target = Address;

    fn deref(&self) -> &Self::Target {
        &self.address
    }
}

impl fmt::Display for AddressInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[derive(Debug, Default)]
/// Options to a [`sync`].
///
/// [`sync`]: Wallet::sync
pub struct SyncOptions {
    /// The progress tracker which may be informed when progress is made.
    pub progress: Option<Box<dyn Progress>>,
}

impl<D> Wallet<D>
where
    D: BatchDatabase,
{
    #[deprecated = "Just use Wallet::new -- all wallets are offline now!"]
    /// Create a new "offline" wallet
    pub fn new_offline<E: IntoWalletDescriptor>(
        descriptor: E,
        change_descriptor: Option<E>,
        network: Network,
        database: D,
    ) -> Result<Self, Error> {
        Self::new(descriptor, change_descriptor, network, database)
    }

    /// Create a wallet.
    ///
    /// The only way this can fail is if the descriptors passed in do not match the checksums in `database`.
    pub fn new<E: IntoWalletDescriptor>(
        descriptor: E,
        change_descriptor: Option<E>,
        network: Network,
        mut database: D,
    ) -> Result<Self, Error> {
        let secp = Secp256k1::new();

        let (descriptor, keymap) = into_wallet_descriptor_checked(descriptor, &secp, network)?;
        Self::db_checksum(
            &mut database,
            &descriptor.to_string(),
            KeychainKind::External,
        )?;
        let signers = Arc::new(SignersContainer::build(keymap, &descriptor, &secp));
        let (change_descriptor, change_signers) = match change_descriptor {
            Some(desc) => {
                let (change_descriptor, change_keymap) =
                    into_wallet_descriptor_checked(desc, &secp, network)?;
                Self::db_checksum(
                    &mut database,
                    &change_descriptor.to_string(),
                    KeychainKind::Internal,
                )?;

                let change_signers = Arc::new(SignersContainer::build(
                    change_keymap,
                    &change_descriptor,
                    &secp,
                ));

                (Some(change_descriptor), change_signers)
            }
            None => (None, Arc::new(SignersContainer::new())),
        };

        Ok(Wallet {
            descriptor,
            change_descriptor,
            signers,
            change_signers,
            network,
            database: RefCell::new(database),
            secp,
        })
    }

    /// This checks the checksum within [`BatchDatabase`] twice (if needed). The first time with the
    /// actual checksum, and the second time with the checksum of `descriptor+checksum`. The second
    /// check is necessary for backwards compatibility of a checksum-inception bug.
    fn db_checksum(db: &mut D, desc: &str, kind: KeychainKind) -> Result<(), Error> {
        let checksum = calc_checksum_bytes_internal(desc, true)?;
        if db.check_descriptor_checksum(kind, checksum).is_ok() {
            return Ok(());
        }

        let checksum_inception = calc_checksum_bytes_internal(desc, false)?;
        db.check_descriptor_checksum(kind, checksum_inception)
    }

    /// Get the Bitcoin network the wallet is using.
    pub fn network(&self) -> Network {
        self.network
    }

    // Return a newly derived address for the specified `keychain`.
    fn get_new_address(&self, keychain: KeychainKind) -> Result<AddressInfo, Error> {
        let incremented_index = self.fetch_and_increment_index(keychain)?;

        let address_result = self
            .get_descriptor_for_keychain(keychain)
            .at_derivation_index(incremented_index)
            .expect("can't be hardened")
            .address(self.network);

        address_result
            .map(|address| AddressInfo {
                address,
                index: incremented_index,
                keychain,
            })
            .map_err(|_| Error::ScriptDoesntHaveAddressForm)
    }

    // Return the the last previously derived address for `keychain` if it has not been used in a
    // received transaction. Otherwise return a new address using [`Wallet::get_new_address`].
    fn get_unused_address(&self, keychain: KeychainKind) -> Result<AddressInfo, Error> {
        let current_index = self.fetch_index(keychain)?;

        let derived_key = self
            .get_descriptor_for_keychain(keychain)
            .at_derivation_index(current_index)
            .expect("can't be hardened");

        let script_pubkey = derived_key.script_pubkey();

        let found_used = self
            .list_transactions(true)?
            .iter()
            .flat_map(|tx_details| tx_details.transaction.as_ref())
            .flat_map(|tx| tx.output.iter())
            .any(|o| o.script_pubkey == script_pubkey);

        if found_used {
            self.get_new_address(keychain)
        } else {
            derived_key
                .address(self.network)
                .map(|address| AddressInfo {
                    address,
                    index: current_index,
                    keychain,
                })
                .map_err(|_| Error::ScriptDoesntHaveAddressForm)
        }
    }

    // Return derived address for the descriptor of given [`KeychainKind`] at a specific index
    fn peek_address(&self, index: u32, keychain: KeychainKind) -> Result<AddressInfo, Error> {
        self.get_descriptor_for_keychain(keychain)
            .at_derivation_index(index)
            .map_err(|_| Error::HardenedIndex)?
            .address(self.network)
            .map(|address| AddressInfo {
                index,
                address,
                keychain,
            })
            .map_err(|_| Error::ScriptDoesntHaveAddressForm)
    }

    // Return derived address for `keychain` at a specific index and reset current
    // address index
    fn reset_address(&self, index: u32, keychain: KeychainKind) -> Result<AddressInfo, Error> {
        self.set_index(keychain, index)?;

        self.get_descriptor_for_keychain(keychain)
            .at_derivation_index(index)
            .map_err(|_| Error::HardenedIndex)?
            .address(self.network)
            .map(|address| AddressInfo {
                index,
                address,
                keychain,
            })
            .map_err(|_| Error::ScriptDoesntHaveAddressForm)
    }

    /// Return a derived address using the external descriptor, see [`AddressIndex`] for
    /// available address index selection strategies. If none of the keys in the descriptor are derivable
    /// (i.e. does not end with /*) then the same address will always be returned for any [`AddressIndex`].
    pub fn get_address(&self, address_index: AddressIndex) -> Result<AddressInfo, Error> {
        self._get_address(address_index, KeychainKind::External)
    }

    /// Return a derived address using the internal (change) descriptor.
    ///
    /// If the wallet doesn't have an internal descriptor it will use the external descriptor.
    ///
    /// see [`AddressIndex`] for available address index selection strategies. If none of the keys
    /// in the descriptor are derivable (i.e. does not end with /*) then the same address will always
    /// be returned for any [`AddressIndex`].
    pub fn get_internal_address(&self, address_index: AddressIndex) -> Result<AddressInfo, Error> {
        self._get_address(address_index, KeychainKind::Internal)
    }

    fn _get_address(
        &self,
        address_index: AddressIndex,
        keychain: KeychainKind,
    ) -> Result<AddressInfo, Error> {
        match address_index {
            AddressIndex::New => self.get_new_address(keychain),
            AddressIndex::LastUnused => self.get_unused_address(keychain),
            AddressIndex::Peek(index) => self.peek_address(index, keychain),
            AddressIndex::Reset(index) => self.reset_address(index, keychain),
        }
    }

    /// Ensures that there are at least `max_addresses` addresses cached in the database if the
    /// descriptor is derivable, or 1 address if it is not.
    /// Will return `Ok(true)` if there are new addresses generated (either external or internal),
    /// and `Ok(false)` if all the required addresses are already cached. This function is useful to
    /// explicitly cache addresses in a wallet to do things like check [`Wallet::is_mine`] on
    /// transaction output scripts.
    pub fn ensure_addresses_cached(&self, max_addresses: u32) -> Result<bool, Error> {
        let mut new_addresses_cached = false;
        let max_address = match self.descriptor.has_wildcard() {
            false => 0,
            true => max_addresses,
        };
        debug!("max_address {}", max_address);
        if self
            .database
            .borrow()
            .get_script_pubkey_from_path(KeychainKind::External, max_address.saturating_sub(1))?
            .is_none()
        {
            debug!("caching external addresses");
            new_addresses_cached = true;
            self.cache_addresses(KeychainKind::External, 0, max_address)?;
        }

        if let Some(change_descriptor) = &self.change_descriptor {
            let max_address = match change_descriptor.has_wildcard() {
                false => 0,
                true => max_addresses,
            };

            if self
                .database
                .borrow()
                .get_script_pubkey_from_path(KeychainKind::Internal, max_address.saturating_sub(1))?
                .is_none()
            {
                debug!("caching internal addresses");
                new_addresses_cached = true;
                self.cache_addresses(KeychainKind::Internal, 0, max_address)?;
            }
        }
        Ok(new_addresses_cached)
    }

    /// Return whether or not a `script` is part of this wallet (either internal or external)
    pub fn is_mine(&self, script: &Script) -> Result<bool, Error> {
        self.database.borrow().is_mine(script)
    }

    /// Return the list of unspent outputs of this wallet
    ///
    /// Note that this method only operates on the internal database, which first needs to be
    /// [`Wallet::sync`] manually.
    pub fn list_unspent(&self) -> Result<Vec<LocalUtxo>, Error> {
        Ok(self
            .database
            .borrow()
            .iter_utxos()?
            .into_iter()
            .filter(|l| !l.is_spent)
            .collect())
    }

    /// Returns the `UTXO` owned by this wallet corresponding to `outpoint` if it exists in the
    /// wallet's database.
    pub fn get_utxo(&self, outpoint: OutPoint) -> Result<Option<LocalUtxo>, Error> {
        self.database.borrow().get_utxo(&outpoint)
    }

    /// Return a single transactions made and received by the wallet
    ///
    /// Optionally fill the [`TransactionDetails::transaction`] field with the raw transaction if
    /// `include_raw` is `true`.
    ///
    /// Note that this method only operates on the internal database, which first needs to be
    /// [`Wallet::sync`] manually.
    pub fn get_tx(
        &self,
        txid: &Txid,
        include_raw: bool,
    ) -> Result<Option<TransactionDetails>, Error> {
        self.database.borrow().get_tx(txid, include_raw)
    }

    /// Return an unsorted list of transactions made and received by the wallet
    ///
    /// Optionally fill the [`TransactionDetails::transaction`] field with the raw transaction if
    /// `include_raw` is `true`.
    ///
    /// To sort transactions, the following code can be used:
    /// ```no_run
    /// # let mut tx_list: Vec<bdk::TransactionDetails> = vec![];
    /// tx_list.sort_by(|a, b| {
    ///     b.confirmation_time
    ///         .as_ref()
    ///         .map(|t| t.height)
    ///         .cmp(&a.confirmation_time.as_ref().map(|t| t.height))
    /// });
    /// ```
    ///
    /// Note that this method only operates on the internal database, which first needs to be
    /// [`Wallet::sync`] manually.
    pub fn list_transactions(&self, include_raw: bool) -> Result<Vec<TransactionDetails>, Error> {
        self.database.borrow().iter_txs(include_raw)
    }

    /// Return the balance, separated into available, trusted-pending, untrusted-pending and immature
    /// values.
    ///
    /// Note that this method only operates on the internal database, which first needs to be
    /// [`Wallet::sync`] manually.
    pub fn get_balance(&self) -> Result<Balance, Error> {
        let mut immature = 0;
        let mut trusted_pending = 0;
        let mut untrusted_pending = 0;
        let mut confirmed = 0;
        let utxos = self.list_unspent()?;
        let database = self.database.borrow();
        let last_sync_height = match database
            .get_sync_time()?
            .map(|sync_time| sync_time.block_time.height)
        {
            Some(height) => height,
            // None means database was never synced
            None => return Ok(Balance::default()),
        };
        for u in utxos {
            // Unwrap used since utxo set is created from database
            let tx = database
                .get_tx(&u.outpoint.txid, true)?
                .expect("Transaction not found in database");
            if let Some(tx_conf_time) = &tx.confirmation_time {
                if tx.transaction.expect("No transaction").is_coin_base()
                    && (last_sync_height - tx_conf_time.height) < COINBASE_MATURITY
                {
                    immature += u.txout.value;
                } else {
                    confirmed += u.txout.value;
                }
            } else if u.keychain == KeychainKind::Internal {
                trusted_pending += u.txout.value;
            } else {
                untrusted_pending += u.txout.value;
            }
        }

        Ok(Balance {
            immature,
            trusted_pending,
            untrusted_pending,
            confirmed,
        })
    }

    /// Add an external signer
    ///
    /// See [the `signer` module](signer) for an example.
    pub fn add_signer(
        &mut self,
        keychain: KeychainKind,
        ordering: SignerOrdering,
        signer: Arc<dyn TransactionSigner>,
    ) {
        let signers = match keychain {
            KeychainKind::External => Arc::make_mut(&mut self.signers),
            KeychainKind::Internal => Arc::make_mut(&mut self.change_signers),
        };

        signers.add_external(signer.id(&self.secp), ordering, signer);
    }

    /// Get the signers
    ///
    /// ## Example
    ///
    /// ```
    /// # use bdk::{Wallet, KeychainKind};
    /// # use bdk::bitcoin::Network;
    /// # use bdk::database::MemoryDatabase;
    /// let wallet = Wallet::new("wpkh(tprv8ZgxMBicQKsPe73PBRSmNbTfbcsZnwWhz5eVmhHpi31HW29Z7mc9B4cWGRQzopNUzZUT391DeDJxL2PefNunWyLgqCKRMDkU1s2s8bAfoSk/84'/0'/0'/0/*)", None, Network::Testnet, MemoryDatabase::new())?;
    /// for secret_key in wallet.get_signers(KeychainKind::External).signers().iter().filter_map(|s| s.descriptor_secret_key()) {
    ///     // secret_key: tprv8ZgxMBicQKsPe73PBRSmNbTfbcsZnwWhz5eVmhHpi31HW29Z7mc9B4cWGRQzopNUzZUT391DeDJxL2PefNunWyLgqCKRMDkU1s2s8bAfoSk/84'/0'/0'/0/*
    ///     println!("secret_key: {}", secret_key);
    /// }
    ///
    /// Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_signers(&self, keychain: KeychainKind) -> Arc<SignersContainer> {
        match keychain {
            KeychainKind::External => Arc::clone(&self.signers),
            KeychainKind::Internal => Arc::clone(&self.change_signers),
        }
    }

    #[allow(missing_docs)]
    pub fn build_tx(&self) -> TxBuilder<'_, D, DefaultCoinSelectionAlgorithm, CreateTx> {
        TxBuilder {
            wallet: self,
            params: TxParams::default(),
            coin_selection: DefaultCoinSelectionAlgorithm::default(),
            phantom: core::marker::PhantomData,
        }
    }

    pub(crate) fn create_tx<Cs: coin_selection::CoinSelectionAlgorithm<D>>(
        &self,
        coin_selection: Cs,
        params: TxParams,
    ) -> Result<(psbt::PartiallySignedTransaction, TransactionDetails), Error> {
        let external_policy = self
            .descriptor
            .extract_policy(&self.signers, BuildSatisfaction::None, &self.secp)?
            .unwrap();
        let internal_policy = self
            .change_descriptor
            .as_ref()
            .map(|desc| {
                Ok::<_, Error>(
                    desc.extract_policy(&self.change_signers, BuildSatisfaction::None, &self.secp)?
                        .unwrap(),
                )
            })
            .transpose()?;

        // The policy allows spending external outputs, but it requires a policy path that hasn't been
        // provided
        if params.change_policy != tx_builder::ChangeSpendPolicy::OnlyChange
            && external_policy.requires_path()
            && params.external_policy_path.is_none()
        {
            return Err(Error::SpendingPolicyRequired(KeychainKind::External));
        };
        // Same for the internal_policy path, if present
        if let Some(internal_policy) = &internal_policy {
            if params.change_policy != tx_builder::ChangeSpendPolicy::ChangeForbidden
                && internal_policy.requires_path()
                && params.internal_policy_path.is_none()
            {
                return Err(Error::SpendingPolicyRequired(KeychainKind::Internal));
            };
        }

        let external_requirements = external_policy.get_condition(
            params
                .external_policy_path
                .as_ref()
                .unwrap_or(&BTreeMap::new()),
        )?;
        let internal_requirements = internal_policy
            .map(|policy| {
                Ok::<_, Error>(
                    policy.get_condition(
                        params
                            .internal_policy_path
                            .as_ref()
                            .unwrap_or(&BTreeMap::new()),
                    )?,
                )
            })
            .transpose()?;

        let requirements =
            external_requirements.merge(&internal_requirements.unwrap_or_default())?;
        debug!("Policy requirements: {:?}", requirements);

        let version = match params.version {
            Some(tx_builder::Version(0)) => {
                return Err(Error::Generic("Invalid version `0`".into()))
            }
            Some(tx_builder::Version(1)) if requirements.csv.is_some() => {
                return Err(Error::Generic(
                    "TxBuilder requested version `1`, but at least `2` is needed to use OP_CSV"
                        .into(),
                ))
            }
            Some(tx_builder::Version(x)) => x,
            None if requirements.csv.is_some() => 2,
            _ => 1,
        };

        // We use a match here instead of a map_or_else as it's way more readable :)
        let current_height = match params.current_height {
            // If they didn't tell us the current height, we assume it's the latest sync height.
            None => self.database().get_sync_time()?.map(|sync_time| {
                absolute::LockTime::from_height(sync_time.block_time.height)
                    .expect("Invalid height")
            }),
            h => h,
        };

        let lock_time = match params.locktime {
            // When no nLockTime is specified, we try to prevent fee sniping, if possible
            None => {
                // Fee sniping can be partially prevented by setting the timelock
                // to current_height. If we don't know the current_height,
                // we default to 0.
                let fee_sniping_height = current_height.unwrap_or(absolute::LockTime::ZERO);

                // We choose the biggest between the required nlocktime and the fee sniping
                // height
                match requirements.timelock {
                    // No requirement, just use the fee_sniping_height
                    None => fee_sniping_height,
                    // There's a block-based requirement, but the value is lower than the fee_sniping_height
                    Some(value @ absolute::LockTime::Blocks(_)) if value < fee_sniping_height => fee_sniping_height,
                    // There's a time-based requirement or a block-based requirement greater
                    // than the fee_sniping_height use that value
                    Some(value) => value,
                }
            }
            // Specific nLockTime required and we have no constraints, so just set to that value
            Some(x) if requirements.timelock.is_none() => x,
            // Specific nLockTime required and it's compatible with the constraints
            Some(x) if requirements.timelock.unwrap().is_same_unit(x) && x >= requirements.timelock.unwrap() => x,
            // Invalid nLockTime required
            Some(x) => return Err(Error::Generic(format!("TxBuilder requested timelock of `{:?}`, but at least `{:?}` is required to spend from this script", x, requirements.timelock.unwrap())))
        };

        let n_sequence = match (params.rbf, requirements.csv) {
            // No RBF or CSV but there's an nLockTime, so the nSequence cannot be final
            (None, None) if lock_time != absolute::LockTime::ZERO => {
                Sequence::ENABLE_LOCKTIME_NO_RBF
            }
            // No RBF, CSV or nLockTime, make the transaction final
            (None, None) => Sequence::MAX,

            // No RBF requested, use the value from CSV. Note that this value is by definition
            // non-final, so even if a timelock is enabled this nSequence is fine, hence why we
            // don't bother checking for it here. The same is true for all the other branches below
            (None, Some(csv)) => csv,

            // RBF with a specific value but that value is too high
            (Some(tx_builder::RbfValue::Value(rbf)), _) if !rbf.is_rbf() => {
                return Err(Error::Generic(
                    "Cannot enable RBF with a nSequence >= 0xFFFFFFFE".into(),
                ))
            }
            // RBF with a specific value requested, but the value is incompatible with CSV
            (Some(tx_builder::RbfValue::Value(rbf)), Some(csv))
                if !check_nsequence_rbf(rbf, csv) =>
            {
                return Err(Error::Generic(format!(
                    "Cannot enable RBF with nSequence `{:?}` given a required OP_CSV of `{:?}`",
                    rbf, csv
                )))
            }

            // RBF enabled with the default value with CSV also enabled. CSV takes precedence
            (Some(tx_builder::RbfValue::Default), Some(csv)) => csv,
            // Valid RBF, either default or with a specific value. We ignore the `CSV` value
            // because we've already checked it before
            (Some(rbf), _) => rbf.get_value(),
        };

        let (fee_rate, mut fee_amount) = match params
            .fee_policy
            .as_ref()
            .unwrap_or(&FeePolicy::FeeRate(FeeRate::default()))
        {
            //FIXME: see https://github.com/bitcoindevkit/bdk/issues/256
            FeePolicy::FeeAmount(fee) => {
                if let Some(previous_fee) = params.bumping_fee {
                    if *fee < previous_fee.absolute {
                        return Err(Error::FeeTooLow {
                            required: previous_fee.absolute,
                        });
                    }
                }
                (FeeRate::from_sat_per_vb(0.0), *fee)
            }
            FeePolicy::FeeRate(rate) => {
                if let Some(previous_fee) = params.bumping_fee {
                    let required_feerate = FeeRate::from_sat_per_vb(previous_fee.rate + 1.0);
                    if *rate < required_feerate {
                        return Err(Error::FeeRateTooLow {
                            required: required_feerate,
                        });
                    }
                }
                (*rate, 0)
            }
        };

        let mut tx = Transaction {
            version,
            lock_time,
            input: vec![],
            output: vec![],
        };

        if params.manually_selected_only && params.utxos.is_empty() {
            return Err(Error::NoUtxosSelected);
        }

        // we keep it as a float while we accumulate it, and only round it at the end
        let mut outgoing: u64 = 0;
        let mut received: u64 = 0;

        let recipients = params.recipients.iter().map(|(r, v)| (r, *v));

        for (index, (script_pubkey, value)) in recipients.enumerate() {
            if !params.allow_dust
                && value.is_dust(script_pubkey)
                && !script_pubkey.is_provably_unspendable()
            {
                return Err(Error::OutputBelowDustLimit(index));
            }

            if self.is_mine(script_pubkey)? {
                received += value;
            }

            let new_out = TxOut {
                script_pubkey: script_pubkey.clone(),
                value,
            };

            tx.output.push(new_out);

            outgoing += value;
        }

        fee_amount += fee_rate.fee_wu(tx.weight());

        // Segwit transactions' header is 2WU larger than legacy txs' header,
        // as they contain a witness marker (1WU) and a witness flag (1WU) (see BIP144).
        // At this point we really don't know if the resulting transaction will be segwit
        // or legacy, so we just add this 2WU to the fee_amount - overshooting the fee amount
        // is better than undershooting it.
        // If we pass a fee_amount that is slightly higher than the final fee_amount, we
        // end up with a transaction with a slightly higher fee rate than the requested one.
        // If, instead, we undershoot, we may end up with a feerate lower than the requested one
        // - we might come up with non broadcastable txs!
        fee_amount += fee_rate.fee_wu(Weight::from_wu(2));

        if params.change_policy != tx_builder::ChangeSpendPolicy::ChangeAllowed
            && self.change_descriptor.is_none()
        {
            return Err(Error::Generic(
                "The `change_policy` can be set only if the wallet has a change_descriptor".into(),
            ));
        }

        let (required_utxos, optional_utxos) = self.preselect_utxos(
            params.change_policy,
            &params.unspendable,
            params.utxos.clone(),
            params.drain_wallet,
            params.manually_selected_only,
            params.bumping_fee.is_some(), // we mandate confirmed transactions if we're bumping the fee
            current_height.map(absolute::LockTime::to_consensus_u32),
        )?;

        // get drain script
        let drain_script = match params.drain_to {
            Some(ref drain_recipient) => drain_recipient.clone(),
            None => self
                .get_internal_address(AddressIndex::New)?
                .address
                .script_pubkey(),
        };

        let coin_selection = coin_selection.coin_select(
            self.database.borrow().deref(),
            required_utxos,
            optional_utxos,
            fee_rate,
            outgoing + fee_amount,
            &drain_script,
        )?;
        fee_amount += coin_selection.fee_amount;
        let excess = &coin_selection.excess;

        tx.input = coin_selection
            .selected
            .iter()
            .map(|u| bitcoin::TxIn {
                previous_output: u.outpoint(),
                script_sig: ScriptBuf::default(),
                sequence: n_sequence,
                witness: Witness::new(),
            })
            .collect();

        if tx.output.is_empty() {
            // Uh oh, our transaction has no outputs.
            // We allow this when:
            // - We have a drain_to address and the utxos we must spend (this happens,
            // for example, when we RBF)
            // - We have a drain_to address and drain_wallet set
            // Otherwise, we don't know who we should send the funds to, and how much
            // we should send!
            if params.drain_to.is_some() && (params.drain_wallet || !params.utxos.is_empty()) {
                if let NoChange {
                    dust_threshold,
                    remaining_amount,
                    change_fee,
                } = excess
                {
                    return Err(Error::InsufficientFunds {
                        needed: *dust_threshold,
                        available: remaining_amount.saturating_sub(*change_fee),
                    });
                }
            } else {
                return Err(Error::NoRecipients);
            }
        }

        match excess {
            NoChange {
                remaining_amount, ..
            } => fee_amount += remaining_amount,
            Change { amount, fee } => {
                if self.is_mine(&drain_script)? {
                    received += amount;
                }
                fee_amount += fee;

                // create drain output
                let drain_output = TxOut {
                    value: *amount,
                    script_pubkey: drain_script,
                };

                // TODO: We should pay attention when adding a new output: this might increase
                // the lenght of the "number of vouts" parameter by 2 bytes, potentially making
                // our feerate too low
                tx.output.push(drain_output);
            }
        };

        // sort input/outputs according to the chosen algorithm
        params.ordering.sort_tx(&mut tx);

        let txid = tx.txid();
        let sent = coin_selection.local_selected_amount();
        let psbt = self.complete_transaction(tx, coin_selection.selected, params)?;

        let transaction_details = TransactionDetails {
            transaction: None,
            txid,
            confirmation_time: None,
            received,
            sent,
            fee: Some(fee_amount),
        };

        Ok((psbt, transaction_details))
    }

    #[allow(missing_docs)]
    pub fn build_fee_bump(
        &self,
        txid: Txid,
    ) -> Result<TxBuilder<'_, D, DefaultCoinSelectionAlgorithm, BumpFee>, Error> {
        let mut details = match self.database.borrow().get_tx(&txid, true)? {
            None => return Err(Error::TransactionNotFound),
            Some(tx) if tx.transaction.is_none() => return Err(Error::TransactionNotFound),
            Some(tx) if tx.confirmation_time.is_some() => return Err(Error::TransactionConfirmed),
            Some(tx) => tx,
        };
        let mut tx = details.transaction.take().unwrap();
        if !tx
            .input
            .iter()
            .any(|txin| txin.sequence.to_consensus_u32() <= 0xFFFFFFFD)
        {
            return Err(Error::IrreplaceableTransaction);
        }

        let feerate = FeeRate::from_wu(details.fee.ok_or(Error::FeeRateUnavailable)?, tx.weight());

        // remove the inputs from the tx and process them
        let original_txin = tx.input.drain(..).collect::<Vec<_>>();
        let original_utxos = original_txin
            .iter()
            .map(|txin| -> Result<_, Error> {
                let txout = self
                    .database
                    .borrow()
                    .get_previous_output(&txin.previous_output)?
                    .ok_or(Error::UnknownUtxo)?;

                let (weight, keychain) = match self
                    .database
                    .borrow()
                    .get_path_from_script_pubkey(&txout.script_pubkey)?
                {
                    #[allow(deprecated)]
                    Some((keychain, _)) => (
                        self._get_descriptor_for_keychain(keychain)
                            .0
                            .max_weight_to_satisfy()
                            .unwrap(),
                        keychain,
                    ),
                    None => {
                        // estimate the weight based on the scriptsig/witness size present in the
                        // original transaction
                        let weight =
                            serialize(&txin.script_sig).len() * 4 + serialize(&txin.witness).len();
                        (weight, KeychainKind::External)
                    }
                };

                let utxo = LocalUtxo {
                    outpoint: txin.previous_output,
                    txout,
                    keychain,
                    is_spent: true,
                };

                Ok(WeightedUtxo {
                    satisfaction_weight: weight,
                    utxo: Utxo::Local(utxo),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if tx.output.len() > 1 {
            let mut change_index = None;
            for (index, txout) in tx.output.iter().enumerate() {
                let (_, change_type) = self._get_descriptor_for_keychain(KeychainKind::Internal);
                match self
                    .database
                    .borrow()
                    .get_path_from_script_pubkey(&txout.script_pubkey)?
                {
                    Some((keychain, _)) if keychain == change_type => change_index = Some(index),
                    _ => {}
                }
            }

            if let Some(change_index) = change_index {
                tx.output.remove(change_index);
            }
        }

        let params = TxParams {
            // TODO: figure out what rbf option should be?
            version: Some(tx_builder::Version(tx.version)),
            recipients: tx
                .output
                .into_iter()
                .map(|txout| (txout.script_pubkey, txout.value))
                .collect(),
            utxos: original_utxos,
            bumping_fee: Some(tx_builder::PreviousFee {
                absolute: details.fee.ok_or(Error::FeeRateUnavailable)?,
                rate: feerate.as_sat_per_vb(),
            }),
            ..Default::default()
        };

        Ok(TxBuilder {
            wallet: self,
            params,
            coin_selection: DefaultCoinSelectionAlgorithm::default(),
            phantom: core::marker::PhantomData,
        })
    }

    #[allow(missing_docs)]
    pub fn sign(
        &self,
        psbt: &mut psbt::PartiallySignedTransaction,
        sign_options: SignOptions,
    ) -> Result<bool, Error> {
        // This adds all the PSBT metadata for the inputs, which will help us later figure out how
        // to derive our keys
        self.update_psbt_with_descriptor(psbt)?;

        // If we aren't allowed to use `witness_utxo`, ensure that every input (except p2tr and finalized ones)
        // has the `non_witness_utxo`
        if !sign_options.trust_witness_utxo
            && psbt
                .inputs
                .iter()
                .filter(|i| i.final_script_witness.is_none() && i.final_script_sig.is_none())
                .filter(|i| i.tap_internal_key.is_none() && i.tap_merkle_root.is_none())
                .any(|i| i.non_witness_utxo.is_none())
        {
            return Err(Error::Signer(signer::SignerError::MissingNonWitnessUtxo));
        }

        // If the user hasn't explicitly opted-in, refuse to sign the transaction unless every input
        // is using `SIGHASH_ALL` or `SIGHASH_DEFAULT` for taproot
        if !sign_options.allow_all_sighashes
            && !psbt.inputs.iter().all(|i| {
                i.sighash_type.is_none()
                    || i.sighash_type == Some(EcdsaSighashType::All.into())
                    || i.sighash_type == Some(TapSighashType::All.into())
                    || i.sighash_type == Some(TapSighashType::Default.into())
            })
        {
            return Err(Error::Signer(signer::SignerError::NonStandardSighash));
        }

        for signer in self
            .signers
            .signers()
            .iter()
            .chain(self.change_signers.signers().iter())
        {
            signer.sign_transaction(psbt, &sign_options, &self.secp)?;
        }

        // attempt to finalize
        if sign_options.try_finalize {
            self.finalize_psbt(psbt, sign_options)
        } else {
            Ok(false)
        }
    }

    /// Return the spending policies for the wallet's descriptor
    pub fn policies(&self, keychain: KeychainKind) -> Result<Option<Policy>, Error> {
        match (keychain, self.change_descriptor.as_ref()) {
            (KeychainKind::External, _) => Ok(self.descriptor.extract_policy(
                &self.signers,
                BuildSatisfaction::None,
                &self.secp,
            )?),
            (KeychainKind::Internal, None) => Ok(None),
            (KeychainKind::Internal, Some(desc)) => Ok(desc.extract_policy(
                &self.change_signers,
                BuildSatisfaction::None,
                &self.secp,
            )?),
        }
    }

    /// Return the "public" version of the wallet's descriptor, meaning a new descriptor that has
    /// the same structure but with every secret key removed
    ///
    /// This can be used to build a watch-only version of a wallet
    pub fn public_descriptor(
        &self,
        keychain: KeychainKind,
    ) -> Result<Option<ExtendedDescriptor>, Error> {
        match (keychain, self.change_descriptor.as_ref()) {
            (KeychainKind::External, _) => Ok(Some(self.descriptor.clone())),
            (KeychainKind::Internal, None) => Ok(None),
            (KeychainKind::Internal, Some(desc)) => Ok(Some(desc.clone())),
        }
    }

    /// Finalize a PSBT, i.e., for each input determine if sufficient data is available to pass
    /// validation and construct the respective `scriptSig` or `scriptWitness`. Please refer to
    /// [BIP174](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#Input_Finalizer)
    /// for further information.
    ///
    /// Returns `true` if the PSBT could be finalized, and `false` otherwise.
    ///
    /// The [`SignOptions`] can be used to tweak the behavior of the finalizer.
    pub fn finalize_psbt(
        &self,
        psbt: &mut psbt::PartiallySignedTransaction,
        sign_options: SignOptions,
    ) -> Result<bool, Error> {
        let tx = &psbt.unsigned_tx;
        let mut finished = true;

        for (n, input) in tx.input.iter().enumerate() {
            let psbt_input = &psbt
                .inputs
                .get(n)
                .ok_or(Error::Signer(SignerError::InputIndexOutOfRange))?;
            if psbt_input.final_script_sig.is_some() || psbt_input.final_script_witness.is_some() {
                continue;
            }
            // if the height is None in the database it means it's still unconfirmed, so consider
            // that as a very high value
            let create_height = self
                .database
                .borrow()
                .get_tx(&input.previous_output.txid, false)?
                .map(|tx| tx.confirmation_time.map(|c| c.height).unwrap_or(u32::MAX));
            let last_sync_height = self
                .database()
                .get_sync_time()?
                .map(|sync_time| sync_time.block_time.height);
            let current_height = sign_options.assume_height.or(last_sync_height);

            debug!(
                "Input #{} - {}, using `create_height` = {:?}, `current_height` = {:?}",
                n, input.previous_output, create_height, current_height
            );

            // - Try to derive the descriptor by looking at the txout. If it's in our database, we
            //   know exactly which `keychain` to use, and which derivation index it is
            // - If that fails, try to derive it by looking at the psbt input: the complete logic
            //   is in `src/descriptor/mod.rs`, but it will basically look at `bip32_derivation`,
            //   `redeem_script` and `witness_script` to determine the right derivation
            // - If that also fails, it will try it on the internal descriptor, if present
            let desc = psbt
                .get_utxo_for(n)
                .map(|txout| self.get_descriptor_for_txout(&txout))
                .transpose()?
                .flatten()
                .or_else(|| {
                    self.descriptor.derive_from_psbt_input(
                        psbt_input,
                        psbt.get_utxo_for(n),
                        &self.secp,
                    )
                })
                .or_else(|| {
                    self.change_descriptor.as_ref().and_then(|desc| {
                        desc.derive_from_psbt_input(psbt_input, psbt.get_utxo_for(n), &self.secp)
                    })
                });

            match desc {
                Some(desc) => {
                    let mut tmp_input = bitcoin::TxIn::default();
                    match desc.satisfy(
                        &mut tmp_input,
                        (
                            PsbtInputSatisfier::new(psbt, n),
                            After::new(current_height, false),
                            Older::new(current_height, create_height, false),
                        ),
                    ) {
                        Ok(_) => {
                            let psbt_input = &mut psbt.inputs[n];
                            psbt_input.final_script_sig = Some(tmp_input.script_sig);
                            psbt_input.final_script_witness = Some(tmp_input.witness);
                            if sign_options.remove_partial_sigs {
                                psbt_input.partial_sigs.clear();
                            }
                        }
                        Err(e) => {
                            debug!("satisfy error {:?} for input {}", e, n);
                            finished = false
                        }
                    }
                }
                None => finished = false,
            }
        }

        Ok(finished)
    }

    /// Return the secp256k1 context used for all signing operations
    pub fn secp_ctx(&self) -> &SecpCtx {
        &self.secp
    }

    /// Returns the descriptor used to create addresses for a particular `keychain`.
    pub fn get_descriptor_for_keychain(&self, keychain: KeychainKind) -> &ExtendedDescriptor {
        let (descriptor, _) = self._get_descriptor_for_keychain(keychain);
        descriptor
    }

    // Internals

    fn _get_descriptor_for_keychain(
        &self,
        keychain: KeychainKind,
    ) -> (&ExtendedDescriptor, KeychainKind) {
        match keychain {
            KeychainKind::Internal if self.change_descriptor.is_some() => (
                self.change_descriptor.as_ref().unwrap(),
                KeychainKind::Internal,
            ),
            _ => (&self.descriptor, KeychainKind::External),
        }
    }

    fn get_descriptor_for_txout(&self, txout: &TxOut) -> Result<Option<DerivedDescriptor>, Error> {
        Ok(self
            .database
            .borrow()
            .get_path_from_script_pubkey(&txout.script_pubkey)?
            .map(|(keychain, child)| (self.get_descriptor_for_keychain(keychain), child))
            .map(|(desc, child)| {
                desc.at_derivation_index(child)
                    .expect("child is not hardened")
            }))
    }

    fn fetch_and_increment_index(&self, keychain: KeychainKind) -> Result<u32, Error> {
        let (descriptor, keychain) = self._get_descriptor_for_keychain(keychain);
        let index = match descriptor.has_wildcard() {
            false => 0,
            true => self.database.borrow_mut().increment_last_index(keychain)?,
        };

        if self
            .database
            .borrow()
            .get_script_pubkey_from_path(keychain, index)?
            .is_none()
        {
            self.cache_addresses(keychain, index, CACHE_ADDR_BATCH_SIZE)?;
        }

        Ok(index)
    }

    fn fetch_index(&self, keychain: KeychainKind) -> Result<u32, Error> {
        let (descriptor, keychain) = self._get_descriptor_for_keychain(keychain);
        let index = match descriptor.has_wildcard() {
            false => Some(0),
            true => self.database.borrow_mut().get_last_index(keychain)?,
        };

        if let Some(i) = index {
            Ok(i)
        } else {
            self.fetch_and_increment_index(keychain)
        }
    }

    fn set_index(&self, keychain: KeychainKind, index: u32) -> Result<(), Error> {
        self.database.borrow_mut().set_last_index(keychain, index)?;
        Ok(())
    }

    fn cache_addresses(
        &self,
        keychain: KeychainKind,
        from: u32,
        mut count: u32,
    ) -> Result<(), Error> {
        let (descriptor, keychain) = self._get_descriptor_for_keychain(keychain);
        if !descriptor.has_wildcard() {
            if from > 0 {
                return Ok(());
            }

            count = 1;
        }

        let mut address_batch = self.database.borrow().begin_batch();

        let start_time = time::Instant::new();
        for i in from..(from + count) {
            address_batch.set_script_pubkey(
                &descriptor
                    .at_derivation_index(i)
                    .expect("i is not hardened")
                    .script_pubkey(),
                keychain,
                i,
            )?;
        }

        info!(
            "Derivation of {} addresses from {} took {} ms",
            count,
            from,
            start_time.elapsed().as_millis()
        );

        self.database.borrow_mut().commit_batch(address_batch)?;

        Ok(())
    }

    fn get_available_utxos(&self) -> Result<Vec<(LocalUtxo, usize)>, Error> {
        Ok(self
            .list_unspent()?
            .into_iter()
            .map(|utxo| {
                let keychain = utxo.keychain;
                (
                    utxo,
                    #[allow(deprecated)]
                    self.get_descriptor_for_keychain(keychain)
                        .max_weight_to_satisfy()
                        .unwrap(),
                )
            })
            .collect())
    }

    /// Given the options returns the list of utxos that must be used to form the
    /// transaction and any further that may be used if needed.
    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    fn preselect_utxos(
        &self,
        change_policy: tx_builder::ChangeSpendPolicy,
        unspendable: &HashSet<OutPoint>,
        manually_selected: Vec<WeightedUtxo>,
        must_use_all_available: bool,
        manual_only: bool,
        must_only_use_confirmed_tx: bool,
        current_height: Option<u32>,
    ) -> Result<(Vec<WeightedUtxo>, Vec<WeightedUtxo>), Error> {
        //    must_spend <- manually selected utxos
        //    may_spend  <- all other available utxos
        let mut may_spend = self.get_available_utxos()?;

        may_spend.retain(|may_spend| {
            !manually_selected
                .iter()
                .any(|manually_selected| manually_selected.utxo.outpoint() == may_spend.0.outpoint)
        });
        let mut must_spend = manually_selected;

        // NOTE: we are intentionally ignoring `unspendable` here. i.e manual
        // selection overrides unspendable.
        if manual_only {
            return Ok((must_spend, vec![]));
        }

        let database = self.database.borrow();
        let satisfies_confirmed = may_spend
            .iter()
            .map(|u| {
                database
                    .get_tx(&u.0.outpoint.txid, true)
                    .map(|tx| match tx {
                        // We don't have the tx in the db for some reason,
                        // so we can't know for sure if it's mature or not.
                        // We prefer not to spend it.
                        None => false,
                        Some(tx) => {
                            // Whether the UTXO is mature and, if needed, confirmed
                            let mut spendable = true;
                            if must_only_use_confirmed_tx && tx.confirmation_time.is_none() {
                                return false;
                            }
                            if tx
                                .transaction
                                .expect("We specifically ask for the transaction above")
                                .is_coin_base()
                            {
                                if let Some(current_height) = current_height {
                                    match &tx.confirmation_time {
                                        Some(t) => {
                                            // https://github.com/bitcoin/bitcoin/blob/c5e67be03bb06a5d7885c55db1f016fbf2333fe3/src/validation.cpp#L373-L375
                                            spendable &= (current_height.saturating_sub(t.height))
                                                >= COINBASE_MATURITY;
                                        }
                                        None => spendable = false,
                                    }
                                }
                            }
                            spendable
                        }
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut i = 0;
        may_spend.retain(|u| {
            let retain = change_policy.is_satisfied_by(&u.0)
                && !unspendable.contains(&u.0.outpoint)
                && satisfies_confirmed[i];
            i += 1;
            retain
        });

        let mut may_spend = may_spend
            .into_iter()
            .map(|(local_utxo, satisfaction_weight)| WeightedUtxo {
                satisfaction_weight,
                utxo: Utxo::Local(local_utxo),
            })
            .collect();

        if must_use_all_available {
            must_spend.append(&mut may_spend);
        }

        Ok((must_spend, may_spend))
    }

    fn complete_transaction(
        &self,
        tx: Transaction,
        selected: Vec<Utxo>,
        params: TxParams,
    ) -> Result<psbt::PartiallySignedTransaction, Error> {
        let mut psbt = psbt::PartiallySignedTransaction::from_unsigned_tx(tx)?;

        if params.add_global_xpubs {
            let mut all_xpubs = self.descriptor.get_extended_keys()?;
            if let Some(change_descriptor) = &self.change_descriptor {
                all_xpubs.extend(change_descriptor.get_extended_keys()?);
            }

            for xpub in all_xpubs {
                let origin = match xpub.origin {
                    Some(origin) => origin,
                    None if xpub.xkey.depth == 0 => {
                        (xpub.root_fingerprint(&self.secp), vec![].into())
                    }
                    _ => return Err(Error::MissingKeyOrigin(xpub.xkey.to_string())),
                };

                psbt.xpub.insert(xpub.xkey, origin);
            }
        }

        let mut lookup_output = selected
            .into_iter()
            .map(|utxo| (utxo.outpoint(), utxo))
            .collect::<HashMap<_, _>>();

        // add metadata for the inputs
        for (psbt_input, input) in psbt.inputs.iter_mut().zip(psbt.unsigned_tx.input.iter()) {
            let utxo = match lookup_output.remove(&input.previous_output) {
                Some(utxo) => utxo,
                None => continue,
            };

            match utxo {
                Utxo::Local(utxo) => {
                    *psbt_input =
                        match self.get_psbt_input(utxo, params.sighash, params.only_witness_utxo) {
                            Ok(psbt_input) => psbt_input,
                            Err(e) => match e {
                                Error::UnknownUtxo => psbt::Input {
                                    sighash_type: params.sighash,
                                    ..psbt::Input::default()
                                },
                                _ => return Err(e),
                            },
                        }
                }
                Utxo::Foreign {
                    psbt_input: foreign_psbt_input,
                    outpoint,
                } => {
                    let is_taproot = foreign_psbt_input
                        .witness_utxo
                        .as_ref()
                        .map(|txout| txout.script_pubkey.is_v1_p2tr())
                        .unwrap_or(false);
                    if !is_taproot
                        && !params.only_witness_utxo
                        && foreign_psbt_input.non_witness_utxo.is_none()
                    {
                        return Err(Error::Generic(format!(
                            "Missing non_witness_utxo on foreign utxo {}",
                            outpoint
                        )));
                    }
                    *psbt_input = *foreign_psbt_input;
                }
            }
        }

        self.update_psbt_with_descriptor(&mut psbt)?;

        Ok(psbt)
    }

    /// get the corresponding PSBT Input for a LocalUtxo
    pub fn get_psbt_input(
        &self,
        utxo: LocalUtxo,
        sighash_type: Option<psbt::PsbtSighashType>,
        only_witness_utxo: bool,
    ) -> Result<psbt::Input, Error> {
        // Try to find the prev_script in our db to figure out if this is internal or external,
        // and the derivation index
        let (keychain, child) = self
            .database
            .borrow()
            .get_path_from_script_pubkey(&utxo.txout.script_pubkey)?
            .ok_or(Error::UnknownUtxo)?;

        let mut psbt_input = psbt::Input {
            sighash_type,
            ..psbt::Input::default()
        };

        let desc = self.get_descriptor_for_keychain(keychain);
        let derived_descriptor = desc
            .at_derivation_index(child)
            .expect("child can't be hardened");

        psbt_input
            .update_with_descriptor_unchecked(&derived_descriptor)
            .map_err(MiniscriptPsbtError::Conversion)?;

        let prev_output = utxo.outpoint;
        if let Some(prev_tx) = self.database.borrow().get_raw_tx(&prev_output.txid)? {
            if desc.is_witness() || desc.is_taproot() {
                psbt_input.witness_utxo = Some(prev_tx.output[prev_output.vout as usize].clone());
            }
            if !desc.is_taproot() && (!desc.is_witness() || !only_witness_utxo) {
                psbt_input.non_witness_utxo = Some(prev_tx);
            }
        }
        Ok(psbt_input)
    }

    fn update_psbt_with_descriptor(
        &self,
        psbt: &mut psbt::PartiallySignedTransaction,
    ) -> Result<(), Error> {
        // We need to borrow `psbt` mutably within the loops, so we have to allocate a vec for all
        // the input utxos and outputs
        //
        // Clippy complains that the collect is not required, but that's wrong
        #[allow(clippy::needless_collect)]
        let utxos = (0..psbt.inputs.len())
            .filter_map(|i| psbt.get_utxo_for(i).map(|utxo| (true, i, utxo)))
            .chain(
                psbt.unsigned_tx
                    .output
                    .iter()
                    .enumerate()
                    .map(|(i, out)| (false, i, out.clone())),
            )
            .collect::<Vec<_>>();

        // Try to figure out the keychain and derivation for every input and output
        for (is_input, index, out) in utxos.into_iter() {
            if let Some((keychain, child)) = self
                .database
                .borrow()
                .get_path_from_script_pubkey(&out.script_pubkey)?
            {
                debug!(
                    "Found descriptor for input #{} {:?}/{}",
                    index, keychain, child
                );

                let desc = self.get_descriptor_for_keychain(keychain);
                let desc = desc
                    .at_derivation_index(child)
                    .expect("child can't be hardened");

                if is_input {
                    psbt.update_input_with_descriptor(index, &desc)
                        .map_err(MiniscriptPsbtError::UtxoUpdate)?;
                } else {
                    psbt.update_output_with_descriptor(index, &desc)
                        .map_err(MiniscriptPsbtError::OutputUpdate)?;
                }
            }
        }

        Ok(())
    }

    /// Return an immutable reference to the internal database
    pub fn database(&self) -> impl std::ops::Deref<Target = D> + '_ {
        self.database.borrow()
    }

    /// Sync the internal database with the blockchain
    #[maybe_async]
    pub fn sync<B: WalletSync + GetHeight>(
        &self,
        blockchain: &B,
        sync_opts: SyncOptions,
    ) -> Result<(), Error> {
        debug!("Begin sync...");

        // TODO: for the next runs, we cannot reuse the `sync_opts.progress` object due to trait
        // restrictions
        let mut progress_iter = sync_opts.progress.into_iter();
        let mut new_progress = || {
            progress_iter
                .next()
                .unwrap_or_else(|| Box::new(NoopProgress))
        };

        let run_setup = self.ensure_addresses_cached(CACHE_ADDR_BATCH_SIZE)?;
        debug!("run_setup: {}", run_setup);

        // TODO: what if i generate an address first and cache some addresses?
        // TODO: we should sync if generating an address triggers a new batch to be stored

        // We need to ensure descriptor is derivable to fullfil "missing cache", otherwise we will
        // end up with an infinite loop
        let has_wildcard = self.descriptor.has_wildcard()
            && (self.change_descriptor.is_none()
                || self.change_descriptor.as_ref().unwrap().has_wildcard());

        // Restrict max rounds in case of faulty "missing cache" implementation by blockchain
        let max_rounds = if has_wildcard { 100 } else { 1 };

        for _ in 0..max_rounds {
            let sync_res = if run_setup {
                maybe_await!(blockchain.wallet_setup(&self.database, new_progress()))
            } else {
                maybe_await!(blockchain.wallet_sync(&self.database, new_progress()))
            };

            // If the error is the special `MissingCachedScripts` error, we return the number of
            // scripts we should ensure cached.
            // On any other error, we should return the error.
            // On no error, we say `ensure_cache` is 0.
            let ensure_cache = sync_res.map_or_else(
                |e| match e {
                    Error::MissingCachedScripts(inner) => {
                        // each call to `WalletSync` is expensive, maximize on scripts to search for
                        let extra =
                            std::cmp::max(inner.missing_count as u32, CACHE_ADDR_BATCH_SIZE);
                        let last = inner.last_count as u32;
                        Ok(extra + last)
                    }
                    _ => Err(e),
                },
                |_| Ok(0_u32),
            )?;

            // cache and try again, break when there is nothing to cache
            if !self.ensure_addresses_cached(ensure_cache)? {
                break;
            }
        }

        let sync_time = SyncTime {
            block_time: BlockTime {
                height: maybe_await!(blockchain.get_height())?,
                timestamp: time::get_timestamp(),
            },
        };
        debug!("Saving `sync_time` = {:?}", sync_time);
        self.database.borrow_mut().set_sync_time(sync_time)?;

        Ok(())
    }

    /// Return the checksum of the public descriptor associated to `keychain`
    ///
    /// Internally calls [`Self::get_descriptor_for_keychain`] to fetch the right descriptor
    pub fn descriptor_checksum(&self, keychain: KeychainKind) -> String {
        self.get_descriptor_for_keychain(keychain)
            .to_string()
            .split_once('#')
            .unwrap()
            .1
            .to_string()
    }
}

/// Deterministically generate a unique name given the descriptors defining the wallet
///
/// Compatible with [`wallet_name_from_descriptor`]
pub fn wallet_name_from_descriptor<T>(
    descriptor: T,
    change_descriptor: Option<T>,
    network: Network,
    secp: &SecpCtx,
) -> Result<String, Error>
where
    T: IntoWalletDescriptor,
{
    //TODO check descriptors contains only public keys
    let descriptor = descriptor
        .into_wallet_descriptor(secp, network)?
        .0
        .to_string();
    let mut wallet_name = calc_checksum(&descriptor[..descriptor.find('#').unwrap()])?;
    if let Some(change_descriptor) = change_descriptor {
        let change_descriptor = change_descriptor
            .into_wallet_descriptor(secp, network)?
            .0
            .to_string();
        wallet_name.push_str(
            calc_checksum(&change_descriptor[..change_descriptor.find('#').unwrap()])?.as_str(),
        );
    }

    Ok(wallet_name)
}
