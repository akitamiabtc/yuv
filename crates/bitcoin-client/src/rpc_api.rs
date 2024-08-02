use async_trait::async_trait;
use bitcoin::address::NetworkUnchecked;
use bitcoin::hashes::hex::FromHex;
use bitcoin::secp256k1::ecdsa::Signature;
use bitcoin::{
    block::Header, Address, Amount, Block, OutPoint, PrivateKey, PublicKey, ScriptBuf, Transaction,
};
use bitcoin_internals::hex::display::DisplayHex;
use serde::*;
use std::collections::HashMap;
use std::iter::FromIterator;

use crate::{
    constants::{BITCOIN_CORE_RPC_V24, BITCOIN_CORE_RPC_V25},
    json, queryable, Error, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonOutPoint {
    pub txid: bitcoin::Txid,
    pub vout: u32,
}

impl From<OutPoint> for JsonOutPoint {
    fn from(o: OutPoint) -> JsonOutPoint {
        JsonOutPoint {
            txid: o.txid,
            vout: o.vout,
        }
    }
}

impl From<JsonOutPoint> for OutPoint {
    fn from(value: JsonOutPoint) -> Self {
        OutPoint {
            txid: value.txid,
            vout: value.vout,
        }
    }
}

/// Shorthand for converting a variable into a serde_json::Value.
pub fn into_json<T>(val: T) -> Result<serde_json::Value>
where
    T: ser::Serialize,
{
    Ok(serde_json::to_value(val)?)
}

/// Shorthand for converting an Option into an Option<serde_json::Value>.
pub fn opt_into_json<T>(opt: Option<T>) -> Result<serde_json::Value>
where
    T: ser::Serialize,
{
    match opt {
        Some(val) => Ok(into_json(val)?),
        None => Ok(serde_json::Value::Null),
    }
}

/// Shorthand for `serde_json::Value::Null`.
pub fn null() -> serde_json::Value {
    serde_json::Value::Null
}

/// Shorthand for an empty serde_json::Value array.
fn empty_arr() -> serde_json::Value {
    serde_json::Value::Array(vec![])
}

/// Shorthand for an empty serde_json object.
fn empty_obj() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// Handle default values in the argument list
///
/// Substitute `Value::Null`s with corresponding values from `defaults` table, except when they are
/// trailing, in which case just skip them altogether in returned list.
///
/// Note, that `defaults` corresponds to the last elements of `args`.
///
/// ```norust
/// arg1 arg2 arg3 arg4
///           def1 def2
/// ```
///
/// Elements of `args` without corresponding `defaults` value, won't be substituted, because they
/// are required.
pub fn handle_defaults<'a>(
    args: &'a mut [serde_json::Value],
    defaults: &[serde_json::Value],
) -> &'a [serde_json::Value] {
    assert!(args.len() >= defaults.len());

    // Pass over the optional arguments in backwards order, filling in defaults after the first
    // non-null optional argument has been observed.
    let mut first_non_null_optional_idx = None;
    for i in 0..defaults.len() {
        let args_i = args.len() - 1 - i;
        let defaults_i = defaults.len() - 1 - i;
        if args[args_i] == serde_json::Value::Null {
            if first_non_null_optional_idx.is_some() {
                if defaults[defaults_i] == serde_json::Value::Null {
                    panic!("Missing `default` for argument idx {}", args_i);
                }
                args[args_i] = defaults[defaults_i].clone();
            }
        } else if first_non_null_optional_idx.is_none() {
            first_non_null_optional_idx = Some(args_i);
        }
    }

    let required_num = args.len() - defaults.len();

    if let Some(i) = first_non_null_optional_idx {
        &args[..i + 1]
    } else {
        &args[..required_num]
    }
}

/// Convert a possible-null result into an Option.
fn opt_result<T: for<'a> de::Deserialize<'a>>(result: serde_json::Value) -> Result<Option<T>> {
    if result == serde_json::Value::Null {
        Ok(None)
    } else {
        Ok(serde_json::from_value(result)?)
    }
}

/// Used to pass raw txs into the API.
pub trait RawTx: Sized + Clone {
    fn raw_hex(self) -> String;
}

impl<'a> RawTx for &'a Transaction {
    fn raw_hex(self) -> String {
        bitcoin::consensus::encode::serialize(self).to_lower_hex_string()
    }
}

impl<'a> RawTx for &'a [u8] {
    fn raw_hex(self) -> String {
        self.to_lower_hex_string()
    }
}

impl<'a> RawTx for &'a Vec<u8> {
    fn raw_hex(self) -> String {
        self.to_lower_hex_string()
    }
}

impl<'a> RawTx for &'a str {
    fn raw_hex(self) -> String {
        self.to_owned()
    }
}

impl RawTx for String {
    fn raw_hex(self) -> String {
        self
    }
}

#[async_trait]
pub trait RpcApi: Sized {
    /// Call a `cmd` rpc with given `args` list
    #[cfg(not(any(test, feature = "mocks")))]
    async fn call<T: for<'a> de::Deserialize<'a>>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> Result<T>;

    /// This is required to be kept, as `mockall` crate can't crate mock for generic paramenters
    /// that is not `'static`. So for tests we are leaving at as this
    #[cfg(any(test, feature = "mocks"))]
    async fn call<T: for<'a> de::Deserialize<'a> + 'static>(
        &self,
        cmd: &str,
        args: &[serde_json::Value],
    ) -> Result<T>;

    /// Query an object implementing `Querable` type
    async fn get_by_id<T: queryable::Queryable<Self>>(
        &self,
        id: &<T as queryable::Queryable<Self>>::Id,
    ) -> Result<T>
    where
        T: Sync + Send,
        <T as queryable::Queryable<Self>>::Id: Sync + Send,
    {
        T::query(self, id).await
    }

    /// Returns an object containing various state info regarding P2P networking. For more
    /// information see: <https://developer.bitcoin.org/reference/rpc/getnetworkinfo.html>
    async fn get_network_info(&self) -> Result<json::GetNetworkInfoResult> {
        self.call("getnetworkinfo", &[]).await
    }

    /// Stops current wallet rescan triggered by an RPC call, e.g. by an importprivkey call.
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/abortrescan.html>
    async fn abort_rescan(&self) -> Result<bool> {
        self.call::<bool>("abortrescan", &[]).await
    }

    /// Invokes ```fn get_network_info(&self)``` and takes from there version field
    async fn version(&self) -> Result<usize> {
        #[derive(Deserialize)]
        struct Response {
            pub version: usize,
        }
        let res: Response = self.call("getnetworkinfo", &[]).await?;
        Ok(res.version)
    }

    /// Add an nrequired-to-sign multisignature address to the wallet. Requires a new wallet backup.
    ///
    /// # Parameters
    /// * nrequired - The number of required signatures out of the n keys or addresses.
    /// * keys - The bitcoin addresses or hex-encoded public keys
    /// * label - A label to assign the addresses to.
    /// * address_type - The address type to use. Options are “legacy”, “p2sh-segwit”, and “bech32”.
    ///
    /// For more info see <https://developer.bitcoin.org/reference/rpc/addmultisigaddress.html>
    async fn add_multisig_address(
        &self,
        nrequired: usize,
        keys: &[json::PubKeyOrAddress<'_>],
        label: Option<&str>,
        address_type: Option<json::AddressType>,
    ) -> Result<json::AddMultiSigAddressResult> {
        let mut args = [
            into_json(nrequired)?,
            into_json(keys)?,
            opt_into_json(label)?,
            opt_into_json(address_type)?,
        ];
        self.call(
            "addmultisigaddress",
            handle_defaults(&mut args, &[into_json("")?, null()]),
        )
        .await
    }

    /// Loads a wallet from a wallet file or directory.
    ///
    /// # Parameters
    /// * wallet - The wallet directory or .dat file.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/loadwallet.html>
    async fn load_wallet(&self, wallet: &str) -> Result<json::LoadWalletResult> {
        self.call("loadwallet", &[wallet.into()]).await
    }

    /// Unloads the wallet referenced by the request endpoint otherwise unloads the wallet specified
    /// in the argument.
    ///
    /// # Parameters
    /// * wallet - The name of the wallet to unload. Must be provided in the RPC endpoint or this
    ///   parameter (but not both).
    ///
    ///  For more information see: <https://developer.bitcoin.org/reference/rpc/unloadwallet.html>
    async fn unload_wallet(&self, wallet: Option<&str>) -> Result<()> {
        let mut args = [opt_into_json(wallet)?];
        self.call("unloadwallet", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Creates and loads a new wallet.
    ///
    /// # Parameters
    /// * wallet - The name for the new wallet. If this is a path, the wallet will be created at the
    ///   path location.
    /// * disable_private_keys - Disable the possibility of private keys (only watchonlys are
    ///   possible in this mode).
    /// * blank - Create a blank wallet. A blank wallet has no keys or HD seed. One can be set using
    ///   sethdseed.
    /// * passphrase - Encrypt the wallet with this passphrase.
    /// * avoid_reuse - Keep track of coin reuse, and treat dirty and clean coins differently with
    ///   privacy considerations in mind.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/createwallet.html>
    async fn create_wallet(
        &self,
        wallet: &str,
        disable_private_keys: Option<bool>,
        blank: Option<bool>,
        passphrase: Option<&str>,
        avoid_reuse: Option<bool>,
    ) -> Result<json::LoadWalletResult> {
        let mut args = [
            wallet.into(),
            opt_into_json(disable_private_keys)?,
            opt_into_json(blank)?,
            opt_into_json(passphrase)?,
            opt_into_json(avoid_reuse)?,
        ];
        self.call(
            "createwallet",
            handle_defaults(
                &mut args,
                &[false.into(), false.into(), into_json("")?, false.into()],
            ),
        )
        .await
    }

    /// Returns a list of currently loaded wallets. For full information on the wallet, use
    /// “getwalletinfo”
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/listwallets.html>
    async fn list_wallets(&self) -> Result<Vec<String>> {
        self.call("listwallets", &[]).await
    }

    /// Returns an object containing various wallet state info.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getwalletinfo.html>
    async fn get_wallet_info(&self) -> Result<json::GetWalletInfoResult> {
        self.call("getwalletinfo", &[]).await
    }

    /// Safely copies current wallet file to destination, which can be a directory or a path with
    /// filename.
    ///
    /// # Parameters
    /// * destination - The destination directory or file
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/backupwallet.html>
    async fn backup_wallet(&self, destination: Option<&str>) -> Result<()> {
        let mut args = [opt_into_json(destination)?];
        self.call("backupwallet", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Reveals the private key corresponding to address. Then the importprivkey can be used with
    /// this output
    ///
    /// # Parameters
    /// * address - The bitcoin address for the private key
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/dumpprivkey.html>
    async fn dump_private_key(&self, address: &Address) -> Result<PrivateKey> {
        self.call("dumpprivkey", &[address.to_string().into()])
            .await
    }

    /// Encrypts the wallet with passphrase. This is for first time encryption. After this, any
    /// calls that interact with private keys such as sending or signing will require the passphrase
    /// to be set prior the making these calls. Use the walletpassphrase call for this, and then
    /// walletlock call. If the wallet is already encrypted, use the walletpassphrasechange call.
    ///
    /// # Parameters
    /// * passphrase - The pass phrase to encrypt the wallet with. It must be at least 1 character,
    ///   but should be long.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/encryptwallet.html>
    async fn encrypt_wallet(&self, passphrase: &str) -> Result<()> {
        self.call("encryptwallet", &[into_json(passphrase)?]).await
    }

    /// Returns the proof-of-work difficulty as a multiple of the minimum difficulty.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getdifficulty.html>
    async fn get_difficulty(&self) -> Result<f64> {
        self.call("getdifficulty", &[]).await
    }

    /// Returns the number of connections to other nodes.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getconnectioncount.html>
    async fn get_connection_count(&self) -> Result<usize> {
        self.call("getconnectioncount", &[]).await
    }

    /// Returns the block with an apropriated hash
    ///
    /// # Parameters
    /// * hash - The block hash
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getblock.html>
    async fn get_block(&self, hash: &bitcoin::BlockHash) -> Result<Block> {
        let hex: String = self.call("getblock", &[into_json(hash)?, 0.into()]).await?;
        let bytes: Vec<u8> = FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }

    /// Returns the block hex
    async fn get_block_hex(&self, hash: &bitcoin::BlockHash) -> Result<String> {
        self.call("getblock", &[into_json(hash)?, 0.into()]).await
    }

    /// Returns the block info
    async fn get_block_info(&self, hash: &bitcoin::BlockHash) -> Result<json::GetBlockResult> {
        self.call("getblock", &[into_json(hash)?, 1.into()]).await
    }

    /// Returns the block with transactions
    async fn get_block_txs(&self, hash: &bitcoin::BlockHash) -> Result<json::GetBlockTxResult> {
        self.call("getblock", &[into_json(hash)?, 2.into()]).await
    }

    /// Returns block header
    ///
    /// # Parameters
    /// * hash - The block hash
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/getblockheader.html>
    async fn get_block_header(&self, hash: &bitcoin::BlockHash) -> Result<Header> {
        let hex: String = self
            .call("getblockheader", &[into_json(hash)?, false.into()])
            .await?;
        let bytes: Vec<u8> = FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }

    /// Returns block header info
    ///
    /// # Parameters
    /// * hash - The block hash
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/getblockheader.html>
    async fn get_block_header_info(
        &self,
        hash: &bitcoin::BlockHash,
    ) -> Result<json::GetBlockHeaderResult> {
        self.call("getblockheader", &[into_json(hash)?, true.into()])
            .await
    }

    /// Returns a json object containing mining-related information.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getmininginfo.html>
    async fn get_mining_info(&self) -> Result<json::GetMiningInfoResult> {
        self.call("getmininginfo", &[]).await
    }

    /// Returns a data structure containing various state info regarding blockchain processing.
    async fn get_blockchain_info(&self) -> Result<json::GetBlockchainInfoResult> {
        let mut raw: serde_json::Value = self.call("getblockchaininfo", &[]).await?;
        // The softfork fields are not backwards compatible:
        // - 0.18.x returns a "softforks" array and a "bip9_softforks" map.
        // - 0.19.x returns a "softforks" map.
        Ok(if self.version().await? < 190000 {
            use Error::UnexpectedStructure as err;

            // First, remove both incompatible softfork fields. We need to scope the mutable ref
            // here for v1.29 borrowck.
            let (bip9_softforks, old_softforks) = {
                let map = raw.as_object_mut().ok_or(err)?;
                let bip9_softforks = map.remove("bip9_softforks").ok_or(err)?;
                let old_softforks = map.remove("softforks").ok_or(err)?;
                // Put back an empty "softforks" field.
                map.insert("softforks".into(), serde_json::Map::new().into());
                (bip9_softforks, old_softforks)
            };
            let mut ret: json::GetBlockchainInfoResult = serde_json::from_value(raw)?;

            // Then convert both softfork types and add them.
            for sf in old_softforks.as_array().ok_or(err)?.iter() {
                let json = sf.as_object().ok_or(err)?;
                let id = json.get("id").ok_or(err)?.as_str().ok_or(err)?;
                let reject = json.get("reject").ok_or(err)?.as_object().ok_or(err)?;
                let active = reject.get("status").ok_or(err)?.as_bool().ok_or(err)?;
                ret.softforks.insert(
                    id.into(),
                    json::Softfork {
                        type_: json::SoftforkType::Buried,
                        bip9: None,
                        height: None,
                        active,
                    },
                );
            }
            for (id, sf) in bip9_softforks.as_object().ok_or(err)?.iter() {
                #[derive(Deserialize)]
                struct OldBip9SoftFork {
                    pub status: json::Bip9SoftforkStatus,
                    pub bit: Option<u8>,
                    #[serde(rename = "startTime")]
                    pub start_time: i64,
                    pub timeout: u64,
                    pub since: u32,
                    pub statistics: Option<json::Bip9SoftforkStatistics>,
                }
                let sf: OldBip9SoftFork = serde_json::from_value(sf.clone())?;
                ret.softforks.insert(
                    id.clone(),
                    json::Softfork {
                        type_: json::SoftforkType::Bip9,
                        bip9: Some(json::Bip9SoftforkInfo {
                            status: sf.status,
                            bit: sf.bit,
                            start_time: sf.start_time,
                            timeout: sf.timeout,
                            since: sf.since,
                            statistics: sf.statistics,
                        }),
                        height: None,
                        active: sf.status == json::Bip9SoftforkStatus::Active,
                    },
                );
            }
            ret
        } else {
            serde_json::from_value(raw)?
        })
    }

    /// Returns the numbers of block in the longest chain.
    async fn get_block_count(&self) -> Result<u64> {
        self.call("getblockcount", &[]).await
    }

    /// Returns the hash of the best (tip) block in the longest blockchain.
    async fn get_best_block_hash(&self) -> Result<bitcoin::BlockHash> {
        self.call("getbestblockhash", &[]).await
    }

    /// Get block hash at a given height
    async fn get_block_hash(&self, height: u64) -> Result<bitcoin::BlockHash> {
        self.call("getblockhash", &[height.into()]).await
    }

    /// Return the raw transaction data.
    ///
    /// # Parameters
    /// * txid - The transaction id
    /// * block_hash - The block in which to look for the transaction
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getrawtransaction.html>
    async fn get_raw_transaction(
        &self,
        txid: &bitcoin::Txid,
        block_hash: Option<bitcoin::BlockHash>,
    ) -> Result<Transaction> {
        let mut args = [
            into_json(txid)?,
            into_json(false)?,
            opt_into_json(block_hash)?,
        ];
        let hex: String = self
            .call("getrawtransaction", handle_defaults(&mut args, &[null()]))
            .await?;
        let bytes: Vec<u8> = FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }

    /// Return the hex of raw transaction.
    ///
    /// # Parameters
    /// * txid - The transaction id
    /// * block_hash - The block in which to look for the transaction
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getrawtransaction.html>
    async fn get_raw_transaction_hex(
        &self,
        txid: &bitcoin::Txid,
        block_hash: Option<&bitcoin::BlockHash>,
    ) -> Result<String> {
        let mut args = [
            into_json(txid)?,
            into_json(false)?,
            opt_into_json(block_hash)?,
        ];
        self.call("getrawtransaction", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Return the raw transaction info.
    ///
    /// # Parameters
    /// * txid - The transaction id
    /// * block_hash - The block in which to look for the transaction
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getrawtransaction.html>
    async fn get_raw_transaction_info(
        &self,
        txid: &bitcoin::Txid,
        block_hash: Option<&bitcoin::BlockHash>,
    ) -> Result<json::GetRawTransactionResult> {
        let mut args = [
            into_json(txid)?,
            into_json(true)?,
            opt_into_json(block_hash)?,
        ];
        self.call("getrawtransaction", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Retrieve a BIP 157 content filter for a particular block.
    ///
    /// # Parameters
    /// * block_hash - The hash of the block
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getblockfilter.html>
    async fn get_block_filter(
        &self,
        block_hash: &bitcoin::BlockHash,
    ) -> Result<json::GetBlockFilterResult> {
        self.call("getblockfilter", &[into_json(block_hash)?]).await
    }

    /// Returns the total available balance.
    ///
    /// # Parameters
    /// * minconf - Only include transactions confirmed at least this many times.
    /// * include_watchonly - Also include balance in watch-only addresses (see ‘importaddress’)
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getbalance.html>
    async fn get_balance(
        &self,
        minconf: Option<usize>,
        include_watchonly: Option<bool>,
    ) -> Result<Amount> {
        let mut args = [
            "*".into(),
            opt_into_json(minconf)?,
            opt_into_json(include_watchonly)?,
        ];
        Ok(Amount::from_btc(
            self.call(
                "getbalance",
                handle_defaults(&mut args, &[0.into(), null()]),
            )
            .await?,
        )?)
    }

    /// Returns an object with all balances in BTC.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getbalances.html>
    async fn get_balances(&self) -> Result<json::GetBalancesResult> {
        Ok(self.call("getbalances", &[]).await?)
    }

    /// Returns the total amount received by the given address in transactions with at least minconf
    /// confirmations.
    ///
    /// # Parameters
    /// * address - The bitcoin address for transactions.
    /// * minconf - Only include transactions confirmed at least this many times.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getreceivedbyaddress.html>
    async fn get_received_by_address(
        &self,
        address: &Address,
        minconf: Option<u32>,
    ) -> Result<Amount> {
        let mut args = [address.to_string().into(), opt_into_json(minconf)?];
        Ok(Amount::from_btc(
            self.call(
                "getreceivedbyaddress",
                handle_defaults(&mut args, &[null()]),
            )
            .await?,
        )?)
    }

    /// Get detailed information about in-wallet transaction
    ///
    /// # Parameters
    /// * txid - The transaction id
    /// * include_watchonly - Whether to include watch-only addresses in balance calculation and
    ///   details[]
    async fn get_transaction(
        &self,
        txid: &bitcoin::Txid,
        include_watchonly: Option<bool>,
    ) -> Result<json::GetTransactionResult> {
        let mut args = [into_json(txid)?, opt_into_json(include_watchonly)?];
        self.call("gettransaction", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// If a label name is provided, this will return only incoming transactions paying to addresses
    /// with the specified label.
    ///
    /// # Parameters
    /// * label - If set, should be a valid label name to return only incoming transactions with the
    /// specified label, or “*” to disable filtering and return all transactions.
    /// * count - The number of transactions to return
    /// * skip - The number of transactions to skip
    /// * include_watchonly - Include transactions to watch-only addresses (see importaddress)
    async fn list_transactions(
        &self,
        label: Option<&str>,
        count: Option<usize>,
        skip: Option<usize>,
        include_watchonly: Option<bool>,
    ) -> Result<Vec<json::ListTransactionResult>> {
        let mut args = [
            label.unwrap_or("*").into(),
            opt_into_json(count)?,
            opt_into_json(skip)?,
            opt_into_json(include_watchonly)?,
        ];
        self.call(
            "listtransactions",
            handle_defaults(&mut args, &[10.into(), 0.into(), null()]),
        )
        .await
    }

    /// Get all transactions in blocks since block blockhash, or all transactions if omitted.
    ///
    /// # Parameters
    /// * blockhash - If set, the block hash to list transactions since, otherwise list all
    ///   transactions.
    /// * target_confirmations - Return the nth block hash from the main chain. e.g. 1 would mean
    ///   the best block hash. Note: this is not used as a filter, but only affects lastblock in the
    ///   return value
    /// * include_watchonly - Include transactions to watch-only addresses (see importaddress)
    /// * include_removed - Show transactions that were removed due to a reorg in the “removed”
    ///   array
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/listsinceblock.html>
    async fn list_since_block(
        &self,
        blockhash: Option<&bitcoin::BlockHash>,
        target_confirmations: Option<usize>,
        include_watchonly: Option<bool>,
        include_removed: Option<bool>,
    ) -> Result<json::ListSinceBlockResult> {
        let mut args = [
            opt_into_json(blockhash)?,
            opt_into_json(target_confirmations)?,
            opt_into_json(include_watchonly)?,
            opt_into_json(include_removed)?,
        ];
        self.call("listsinceblock", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Returns details about an unspent transaction output.
    ///    
    /// # Parameters
    /// * txid - The transaction id
    /// * vout - vout number
    /// * include_mempool -  Whether to include the mempool. Note that an unspent output that is
    ///   spent in the mempool won’t appear.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/gettxout.html>
    async fn get_tx_out(
        &self,
        txid: &bitcoin::Txid,
        vout: u32,
        include_mempool: Option<bool>,
    ) -> Result<Option<json::GetTxOutResult>> {
        let mut args = [
            into_json(txid)?,
            into_json(vout)?,
            opt_into_json(include_mempool)?,
        ];
        opt_result(
            self.call("gettxout", handle_defaults(&mut args, &[null()]))
                .await?,
        )
    }

    /// Returns a hex-encoded proof that “txid” was included in a block.
    ///
    /// # Parameters
    /// * txids - The txids to filter
    /// * blockhash - If specified, looks for txid in the block with this hash
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/gettxoutproof.html>
    async fn get_tx_out_proof(
        &self,
        txids: &[bitcoin::Txid],
        block_hash: Option<&bitcoin::BlockHash>,
    ) -> Result<Vec<u8>> {
        let mut args = [into_json(txids)?, opt_into_json(block_hash)?];
        let hex: String = self
            .call("gettxoutproof", handle_defaults(&mut args, &[null()]))
            .await?;
        Ok(FromHex::from_hex(&hex)?)
    }

    /// Adds a public key (in hex) that can be watched as if it were in your wallet but cannot be
    /// used to spend. Requires a new wallet backup.
    ///
    /// # Parameters
    /// * pubkey - The hex-encoded public key
    /// * label - An optional label
    /// * rescan - Rescan the wallet for transactions
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/importpubkey.html>
    async fn import_public_key(
        &self,
        pubkey: &PublicKey,
        label: Option<&str>,
        rescan: Option<bool>,
    ) -> Result<()> {
        let mut args = [
            pubkey.to_string().into(),
            opt_into_json(label)?,
            opt_into_json(rescan)?,
        ];
        self.call(
            "importpubkey",
            handle_defaults(&mut args, &[into_json("")?, null()]),
        )
        .await
    }

    /// Adds a private key (as returned by dumpprivkey) to your wallet. Requires a new wallet
    /// backup.
    ///
    /// # Parameters
    /// * privkey - The private key (see dumpprivkey)
    /// * label - An optional label
    /// * rescan - Rescan the wallet for transactions
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/importprivkey.html>
    async fn import_private_key(
        &self,
        privkey: &PrivateKey,
        label: Option<&str>,
        rescan: Option<bool>,
    ) -> Result<()> {
        let mut args = [
            privkey.to_string().into(),
            opt_into_json(label)?,
            opt_into_json(rescan)?,
        ];
        self.call(
            "importprivkey",
            handle_defaults(&mut args, &[into_json("")?, null()]),
        )
        .await
    }

    /// Adds an address or script (in hex) that can be watched as if it were in your wallet but
    /// cannot be used to spend. Requires a new wallet backup.
    ///
    /// # Parameters
    /// * address - The Bitcoin address (or hex-encoded script)
    /// * label - An optional label
    /// * rescan - Rescan the wallet for transactions
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/importaddress.html>
    async fn import_address(
        &self,
        address: &Address,
        label: Option<&str>,
        rescan: Option<bool>,
    ) -> Result<()> {
        let mut args = [
            address.to_string().into(),
            opt_into_json(label)?,
            opt_into_json(rescan)?,
        ];
        self.call(
            "importaddress",
            handle_defaults(&mut args, &[into_json("")?, null()]),
        )
        .await
    }

    /// Adds an address or script (in hex) that can be watched as if it were in your wallet but
    /// cannot be used to spend. Requires a new wallet backup.
    ///
    /// # Parameters
    /// * ScriptPubkeyType - The Bitcoin script
    /// * label - An optional label
    /// * rescan - Rescan the wallet for transactions
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/importaddress.html>
    async fn import_address_script(
        &self,
        script: &ScriptBuf,
        label: Option<&str>,
        rescan: Option<bool>,
        p2sh: Option<bool>,
    ) -> Result<()> {
        let mut args = [
            script.as_script().to_string().into(),
            opt_into_json(label)?,
            opt_into_json(rescan)?,
            opt_into_json(p2sh)?,
        ];
        self.call(
            "importaddress",
            handle_defaults(&mut args, &[into_json("")?, true.into(), null()]),
        )
        .await
    }

    /// Import addresses/scripts (with private or public keys, redeem script (P2SH)), optionally
    /// rescanning the blockchain from the earliest creation time of the imported scripts . Requires
    /// a new wallet backup.
    ///
    /// # Parameters
    /// * requests - Data to be imported
    /// * options - json object, optional
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/importmulti.html>
    async fn import_multi(
        &self,
        requests: &[json::ImportMultiRequest<'_>],
        options: Option<&json::ImportMultiOptions>,
    ) -> Result<Vec<json::ImportMultiResult>> {
        let mut json_requests = Vec::with_capacity(requests.len());
        for req in requests {
            json_requests.push(serde_json::to_value(req)?);
        }
        let mut args = [json_requests.into(), opt_into_json(options)?];
        self.call("importmulti", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Sets the label associated with the given address.
    ///
    /// # Parameters
    /// * address - The bitcoin address to be associated with a label.
    /// * label - The label to assign to the address.
    ///
    /// For more informatuin see: <https://developer.bitcoin.org/reference/rpc/setlabel.html>
    async fn set_label(&self, address: &Address, label: &str) -> Result<()> {
        self.call("setlabel", &[address.to_string().into(), label.into()])
            .await
    }

    /// Fills the keypool. Requires wallet passphrase to be set with walletpassphrase call if wallet
    /// is encrypted.
    ///  
    /// # Parameters
    /// * new_size - The new keypool size   
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/keypoolrefill.html>
    async fn key_pool_refill(&self, new_size: Option<usize>) -> Result<()> {
        let mut args = [opt_into_json(new_size)?];
        self.call("keypoolrefill", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Returns array of unspent transaction outputs with between minconf and maxconf (inclusive)
    /// confirmations.
    ///
    /// # Parameters
    /// * minconf - The minimum confirmations to filter
    /// * maxconf - The maximum confirmations to filter
    /// * addresses - The bitcoin addresses to filter
    /// * include_unsafe - Include outputs that are not safe to spend
    /// * query_options - JSON with query options
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/listunspent.html>
    async fn list_unspent(
        &self,
        minconf: Option<usize>,
        maxconf: Option<usize>,
        addresses: Option<&[&Address]>,
        include_unsafe: Option<bool>,
        query_options: Option<json::ListUnspentQueryOptions>,
    ) -> Result<Vec<json::ListUnspentResultEntry>> {
        let mut args = [
            opt_into_json(minconf)?,
            opt_into_json(maxconf)?,
            opt_into_json(addresses)?,
            opt_into_json(include_unsafe)?,
            opt_into_json(query_options)?,
        ];
        let defaults = [
            into_json(0)?,
            into_json(9999999)?,
            empty_arr(),
            into_json(true)?,
            null(),
        ];
        self.call("listunspent", handle_defaults(&mut args, &defaults))
            .await
    }

    /// Updates list of temporarily unspendable outputs. Temporarily lock (unlock=false) or unlock
    /// (unlock=true) specified transaction outputs. If no transaction outputs are specified when
    /// unlocking then all current locked transaction outputs are unlocked. A locked transaction
    /// output will not be chosen by automatic coin selection, when spending bitcoins. Manually
    /// selected coins are automatically unlocked. Locks are stored in memory only. Nodes start with
    /// zero locked outputs, and the locked output list is always cleared (by virtue of process
    /// exit) when a node stops or fails.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/lockunspent.html>
    async fn lock_unspent(&self, outputs: &[OutPoint]) -> Result<bool> {
        let outputs: Vec<_> = outputs
            .iter()
            .map(|o| serde_json::to_value(JsonOutPoint::from(*o)).unwrap())
            .collect();
        self.call("lockunspent", &[false.into(), outputs.into()])
            .await
    }

    /// Updates list of temporarily unspendable outputs. Temporarily lock (unlock=false) or unlock
    /// (unlock=true) specified transaction outputs. If no transaction outputs are specified when
    /// unlocking then all current locked transaction outputs are unlocked. A locked transaction
    /// output will not be chosen by automatic coin selection, when spending bitcoins. Manually
    /// selected coins are automatically unlocked. Locks are stored in memory only. Nodes start with
    /// zero locked outputs, and the locked output list is always cleared (by virtue of process
    /// exit) when a node stops or fails.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/lockunspent.html>
    async fn unlock_unspent(&self, outputs: &[OutPoint]) -> Result<bool> {
        let outputs: Vec<_> = outputs
            .iter()
            .map(|o| serde_json::to_value(JsonOutPoint::from(*o)).unwrap())
            .collect();
        self.call("lockunspent", &[true.into(), outputs.into()])
            .await
    }

    /// List balances by receiving address.
    ///
    /// # Parameters
    /// * minconf - The minimum number of confirmations before payments are included.
    /// * include_empty - Whether to include addresses that haven’t received any payments.
    /// * include_watchonly - Whether to include watch-only addresses
    /// * address_filter - If present, only return information on this address.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/listreceivedbyaddress.html>
    async fn list_received_by_address(
        &self,
        address_filter: Option<&Address>,
        minconf: Option<u32>,
        include_empty: Option<bool>,
        include_watchonly: Option<bool>,
    ) -> Result<Vec<json::ListReceivedByAddressResult>> {
        let mut args = [
            opt_into_json(minconf)?,
            opt_into_json(include_empty)?,
            opt_into_json(include_watchonly)?,
            opt_into_json(address_filter)?,
        ];
        let defaults = [1.into(), false.into(), false.into(), null()];
        self.call(
            "listreceivedbyaddress",
            handle_defaults(&mut args, &defaults),
        )
        .await
    }

    /// Create a transaction spending the given inputs and creating new outputs. Outputs can be
    /// addresses or data. Returns hex-encoded raw transaction. Note that the transaction's inputs
    /// are not signed, and it is not stored in the wallet or transmitted to the network.
    ///
    /// # Parameters
    /// * utxos - The inputs
    /// * outs - The outputs (key-value pairs), where none of the keys are duplicated.
    /// * locktime - Raw locktime. Non-0 value also locktime-activates inputs
    /// * replaceable - Marks this transaction as BIP125-replaceable. Allows this transaction to be
    ///   replaced by a transaction with higher fees. If provided, it is an error if explicit
    ///   sequence numbers are incompatible.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/createrawtransaction.html>
    async fn create_raw_transaction_hex(
        &self,
        utxos: &[json::CreateRawTransactionInput],
        outs: &HashMap<String, Amount>,
        locktime: Option<i64>,
        replaceable: Option<bool>,
    ) -> Result<String> {
        let outs_converted = serde_json::Map::from_iter(
            outs.iter()
                .map(|(key, value)| (key.clone(), serde_json::Value::from(value.to_btc()))),
        );
        let mut args = [
            into_json(utxos)?,
            into_json(outs_converted)?,
            opt_into_json(locktime)?,
            opt_into_json(replaceable)?,
        ];
        let defaults = [into_json(0i64)?, null()];
        self.call(
            "createrawtransaction",
            handle_defaults(&mut args, &defaults),
        )
        .await
    }

    /// Invokes ```fn create_raw_transaction_hex(...)``` and deserialize the response
    async fn create_raw_transaction(
        &self,
        utxos: &[json::CreateRawTransactionInput],
        outs: &HashMap<String, Amount>,
        locktime: Option<i64>,
        replaceable: Option<bool>,
    ) -> Result<Transaction> {
        let hex: String = self
            .create_raw_transaction_hex(utxos, outs, locktime, replaceable)
            .await?;
        let bytes: Vec<u8> = FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }

    /// If the transaction has no inputs, they will be automatically selected to meet its out value.
    /// It will add at most one change output to the outputs. No existing outputs will be modified
    /// unless “subtractFeeFromOutputs” is specified. Note that inputs which were signed may need to
    /// be resigned after completion since in/outputs have been added. The inputs added will not be
    /// signed, use signrawtransactionwithkey or signrawtransactionwithwallet for that. Note that
    /// all existing inputs must have their previous output transaction be in the wallet. Note that
    /// all inputs selected must be of standard form and P2SH scripts must be in the wallet using
    /// importaddress or addmultisigaddress (to calculate fees). You can see whether this is the
    /// case by checking the “solvable” field in the listunspent output. Only pay-to-pubkey,
    /// multisig, and P2SH versions thereof are currently supported for watch-only
    ///
    /// # Parameters
    /// * tx - The hex string of the raw transaction
    /// * options - for backward compatibility: passing in a true instead of an object will result
    ///   in {“includeWatching”:true}
    /// * iswitness - Whether the transaction hex is a serialized witness transaction.
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/fundrawtransaction.html>
    async fn fund_raw_transaction<R: RawTx>(
        &self,
        tx: R,
        options: Option<&json::FundRawTransactionOptions>,
        is_witness: Option<bool>,
    ) -> Result<json::FundRawTransactionResult>
    where
        R: Sync + Send,
    {
        let mut args = [
            tx.raw_hex().into(),
            opt_into_json(options)?,
            opt_into_json(is_witness)?,
        ];
        let defaults = [empty_obj(), null()];
        self.call("fundrawtransaction", handle_defaults(&mut args, &defaults))
            .await
    }

    /// Sign inputs for raw transaction (serialized, hex-encoded). The second optional argument (may
    /// be null) is an array of previous transaction outputs that this transaction depends on but
    /// may not yet be in the block chain. Requires wallet passphrase to be set with
    /// walletpassphrase call if wallet is encrypted.
    ///
    /// # Parameters
    /// * tx - The transaction hex string
    /// * utxos - The previous dependent transaction outputs
    /// * sighash_type - The signature hash type. Must be one of “ALL” “NONE” “SINGLE”
    ///   “ALL|ANYONECANPAY” “NONE|ANYONECANPAY” “SINGLE|ANYONECANPAY”
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/signrawtransactionwithwallet.html>
    async fn sign_raw_transaction_with_wallet<R: RawTx>(
        &self,
        tx: R,
        utxos: Option<&[json::SignRawTransactionInput]>,
        sighash_type: Option<json::EcdsaSighashType>,
    ) -> Result<json::SignRawTransactionResult>
    where
        R: Sync + Send,
    {
        let mut args = [
            tx.raw_hex().into(),
            opt_into_json(utxos)?,
            opt_into_json(sighash_type)?,
        ];
        let defaults = [empty_arr(), null()];
        self.call(
            "signrawtransactionwithwallet",
            handle_defaults(&mut args, &defaults),
        )
        .await
    }

    /// Sign inputs for raw transaction (serialized, hex-encoded). The second argument is an array
    /// of base58-encoded private keys that will be the only keys used to sign the transaction. The
    /// third optional argument (may be null) is an array of previous transaction outputs that this
    /// transaction depends on but may not yet be in the block chain.
    ///
    /// # Parameters
    /// * tx - The transaction hex string
    /// * privkeys - The base58-encoded private keys for signing
    /// * prevtxs - The previous dependent transaction outputs
    /// * sighash_type - The signature hash type. Must be one of: “ALL” “NONE” “SINGLE”
    /// “ALL|ANYONECANPAY” “NONE|ANYONECANPAY” “SINGLE|ANYONECANPAY”
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/signrawtransactionwithkey.html>
    async fn sign_raw_transaction_with_key<R: RawTx>(
        &self,
        tx: R,
        privkeys: &[PrivateKey],
        prevtxs: Option<&[json::SignRawTransactionInput]>,
        sighash_type: Option<json::EcdsaSighashType>,
    ) -> Result<json::SignRawTransactionResult>
    where
        R: Sync + Send,
    {
        let mut args = [
            tx.raw_hex().into(),
            into_json(privkeys)?,
            opt_into_json(prevtxs)?,
            opt_into_json(sighash_type)?,
        ];
        let defaults = [empty_arr(), null()];
        self.call(
            "signrawtransactionwithkey",
            handle_defaults(&mut args, &defaults),
        )
        .await
    }

    /// Returns result of mempool acceptance tests indicating if raw transaction (serialized,
    /// hex-encoded) would be accepted by mempool. This checks if the transaction violates the
    /// consensus or policy rules. See sendrawtransaction call.
    ///
    /// # Parameters
    /// * rawtxs - An array of hex strings of raw transactions.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/testmempoolaccept.html>
    async fn test_mempool_accept<R: RawTx>(
        &self,
        rawtxs: &[R],
    ) -> Result<Vec<json::TestMempoolAcceptResult>>
    where
        R: Sync + Send,
    {
        let hexes: Vec<serde_json::Value> =
            rawtxs.iter().cloned().map(|r| r.raw_hex().into()).collect();
        self.call("testmempoolaccept", &[hexes.into()]).await
    }

    /// Request a graceful shutdown of Bitcoin Core.
    async fn stop(&self) -> Result<String> {
        self.call("stop", &[]).await
    }

    /// Verify a signed message
    ///
    /// # Parameters
    /// * address - The bitcoin address to use for the signature.
    /// * signature - The signature provided by the signer in base 64 encoding (see signmessage).
    /// * message - The message that was signed.
    ///
    /// For more infromation see: <https://developer.bitcoin.org/reference/rpc/verifymessage.html>
    async fn verify_message(
        &self,
        address: &Address,
        signature: &Signature,
        message: &str,
    ) -> Result<bool> {
        let args = [
            address.to_string().into(),
            signature.to_string().into(),
            into_json(message)?,
        ];
        self.call("verifymessage", &args).await
    }

    /// Returns a new Bitcoin address for receiving payments.
    ///
    /// # Parameters
    /// * label - The label name for the address to be linked to. It can also be set to the empty
    ///   string “” to represent the default label. The label does not need to exist, it will be
    ///   created if there is no label by the given name.
    /// * address_type - The address type to use. Options are “legacy”, “p2sh-segwit”, and “bech32”.
    ///
    /// For more infromation see: <https://developer.bitcoin.org/reference/rpc/getnewaddress.html>
    async fn get_new_address(
        &self,
        label: Option<&str>,
        address_type: Option<json::AddressType>,
    ) -> Result<Address<NetworkUnchecked>> {
        self.call(
            "getnewaddress",
            &[opt_into_json(label)?, opt_into_json(address_type)?],
        )
        .await
    }

    /// Return information about the given bitcoin address.
    ///
    /// # Parameters
    /// * address - The bitcoin address for which to get information.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/getaddressinfo.html>     
    async fn get_address_info(&self, address: &Address) -> Result<json::GetAddressInfoResult> {
        self.call("getaddressinfo", &[address.to_string().into()])
            .await
    }

    /// Mine `block_num` blocks and pay coinbase to `address`
    ///
    /// Returns hashes of the generated blocks
    async fn generate_to_address(
        &self,
        block_num: u64,
        address: &Address,
    ) -> Result<Vec<bitcoin::BlockHash>> {
        self.call(
            "generatetoaddress",
            &[block_num.into(), address.to_string().into()],
        )
        .await
    }

    /// Mine up to block_num blocks immediately (before the RPC call returns) to an address in the
    /// wallet.
    async fn generate(
        &self,
        block_num: u64,
        maxtries: Option<u64>,
    ) -> Result<Vec<bitcoin::BlockHash>> {
        self.call("generate", &[block_num.into(), opt_into_json(maxtries)?])
            .await
    }

    /// Mark a block as invalid by `block_hash`
    async fn invalidate_block(&self, block_hash: &bitcoin::BlockHash) -> Result<()> {
        self.call("invalidateblock", &[into_json(block_hash)?])
            .await
    }

    /// Mark a block as valid by `block_hash`
    async fn reconsider_block(&self, block_hash: &bitcoin::BlockHash) -> Result<()> {
        self.call("reconsiderblock", &[into_json(block_hash)?])
            .await
    }

    /// Get txids of all transactions in a memory pool
    async fn get_raw_mempool(&self) -> Result<Vec<bitcoin::Txid>> {
        self.call("getrawmempool", &[]).await
    }

    /// Get mempool data for given transaction
    async fn get_mempool_entry(&self, txid: &bitcoin::Txid) -> Result<json::GetMempoolEntryResult> {
        self.call("getmempoolentry", &[into_json(txid)?]).await
    }

    /// Returns data about each connected network node as an array of [`PeerInfo`][]
    ///
    /// [`PeerInfo`]: net/struct.PeerInfo.html
    async fn get_peer_info(&self) -> Result<Vec<json::GetPeerInfoResult>> {
        self.call("getpeerinfo", &[]).await
    }

    /// Requests that a ping be sent to all other nodes, to measure ping time.
    ///
    /// Results provided in `getpeerinfo`, `pingtime` and `pingwait` fields are decimal seconds.
    ///
    /// Ping command is handled in queue with all other commands, so it measures processing backlog,
    /// not just network ping.
    async fn ping(&self) -> Result<()> {
        self.call("ping", &[]).await
    }

    /// Submit a raw transaction to the Bitcoin network. Use [`Self::send_raw_transaction_opts`] for
    /// additional option params.
    async fn send_raw_transaction<R: RawTx>(&self, tx: R) -> Result<bitcoin::Txid>
    where
        R: Sync + Send,
    {
        self.call("sendrawtransaction", &[tx.raw_hex().into()])
            .await
    }
    /// Submit a raw transaction to the Bitcoin network.
    ///
    /// This method is available since Bitcoin Core 0.25.0,
    /// but this function is backward compatible.
    ///
    /// # Arguments
    ///
    /// 1. `max_fee_rate` -  Reject transactions whose fee rate is higher than the specified value,
    /// expressed in BTC/kvB.
    /// 2. `max_burn_amount` - Reject transactions with provably unspendable outputs (e.g.
    /// 'datacarrier' outputs that use the OP_RETURN opcode) greater than the specified value,
    /// expressed in BTC. If burning funds through unspendable outputs is desired, increase this
    /// value. This check is based on heuristics and does not guarantee spendability of outputs.
    ///
    /// Use None to set default values (0.0)
    ///
    /// For more information see
    /// <https://bitcoincore.org/en/doc/26.0.0/rpc/rawtransactions/sendrawtransaction/>
    async fn send_raw_transaction_opts<R: RawTx>(
        &self,
        tx: R,
        max_fee_rate: Option<f32>,
        max_burn_amount: Option<f64>,
    ) -> Result<bitcoin::Txid>
    where
        R: Sync + Send,
    {
        match self.get_network_info().await?.version {
            ..=BITCOIN_CORE_RPC_V24 => {
                self.call("sendrawtransaction", &[tx.raw_hex().into()])
                    .await
            }
            BITCOIN_CORE_RPC_V25.. => {
                self.call(
                    "sendrawtransaction",
                    &[
                        tx.raw_hex().into(),
                        max_fee_rate.unwrap_or(0.0).into(),
                        max_burn_amount.unwrap_or(0.0).into(),
                    ],
                )
                .await
            }
        }
    }

    /// Estimates the approximate fee per kilobyte needed for a transaction to begin confirmation
    /// within conf_target blocks if possible and return the number of blocks for which the estimate
    /// is valid.
    ///
    /// Uses virtual transaction size as defined in BIP 141 (witness data is discounted).
    ///
    /// # Parameters
    /// * conf_target - Confirmation target in blocks (1 - 1008)
    /// * estimate_mode - The fee estimate mode.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/estimatesmartfee.html>
    async fn estimate_smart_fee(
        &self,
        conf_target: u16,
        estimate_mode: Option<json::EstimateMode>,
    ) -> Result<json::EstimateSmartFeeResult> {
        let mut args = [into_json(conf_target)?, opt_into_json(estimate_mode)?];
        self.call("estimatesmartfee", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Waits for a specific new block and returns useful info about it. Returns the current block
    /// on timeout or exit.
    ///
    /// # Arguments
    ///
    /// 1. `timeout`: Time in milliseconds to wait for a response. 0 indicates no timeout.
    async fn wait_for_new_block(&self, timeout: u64) -> Result<json::BlockRef> {
        self.call("waitfornewblock", &[into_json(timeout)?]).await
    }

    /// Waits for a specific new block and returns useful info about it. Returns the current block
    /// on timeout or exit.
    ///
    /// # Arguments
    ///
    /// 1. `blockhash`: Block hash to wait for.
    /// 2. `timeout`: Time in milliseconds to wait for a response. 0 indicates no timeout.
    async fn wait_for_block(
        &self,
        blockhash: &bitcoin::BlockHash,
        timeout: u64,
    ) -> Result<json::BlockRef> {
        let args = [into_json(blockhash)?, into_json(timeout)?];
        self.call("waitforblock", &args).await
    }

    /// Creates and funds a transaction in the Partially Signed Transaction format.
    ///
    /// # Parameters
    /// * inputs - Leave empty to add inputs automatically. See add_inputs option.
    /// * outputs - The outputs (key-value pairs), where none of the keys are duplicated.
    /// * locktime - Raw locktime. Non-0 value also locktime-activates inputs
    /// * options - “replaceable”: bool, (boolean, optional, default=wallet default) Marks this
    ///   transaction as BIP125 replaceable.
    /// * bip32derivs - Include BIP 32 derivation paths for public keys if we know them
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/walletcreatefundedpsbt.html>
    async fn wallet_create_funded_psbt(
        &self,
        inputs: &[json::CreateRawTransactionInput],
        outputs: &HashMap<String, Amount>,
        locktime: Option<i64>,
        options: Option<json::WalletCreateFundedPsbtOptions>,
        bip32derivs: Option<bool>,
    ) -> Result<json::WalletCreateFundedPsbtResult> {
        let outputs_converted = serde_json::Map::from_iter(
            outputs
                .iter()
                .map(|(key, value)| (key.clone(), serde_json::Value::from(value.to_btc()))),
        );
        let mut args = [
            into_json(inputs)?,
            into_json(outputs_converted)?,
            opt_into_json(locktime)?,
            opt_into_json(options)?,
            opt_into_json(bip32derivs)?,
        ];
        self.call(
            "walletcreatefundedpsbt",
            handle_defaults(
                &mut args,
                &[0.into(), serde_json::Map::new().into(), false.into()],
            ),
        )
        .await
    }

    /// Analyses a descriptor.
    ///
    /// # Parameters
    /// * desc - The descriptor.
    ///
    /// For more information see:
    /// <https://developer.bitcoin.org/reference/rpc/getdescriptorinfo.html>
    async fn get_descriptor_info(&self, desc: &str) -> Result<json::GetDescriptorInfoResult> {
        self.call("getdescriptorinfo", &[desc.to_string().into()])
            .await
    }

    /// Combine multiple partially signed Bitcoin transactions into one transaction.
    ///
    /// # Parameters
    /// * psbts - The base64 strings of partially signed transactions
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/combinepsbt.html>
    async fn combine_psbt(&self, psbts: &[String]) -> Result<String> {
        self.call("combinepsbt", &[into_json(psbts)?]).await
    }

    /// Finalize the inputs of a PSBT. If the transaction is fully signed, it will produce a network
    /// serialized transaction which can be broadcast with sendrawtransaction. Otherwise a PSBT will
    /// be created which has the final_scriptSig and final_scriptWitness fields filled for inputs
    /// that are complete.
    ///
    /// # Parameters
    /// * psbt - A base64 string of a PSBT
    /// * extract - If true and the transaction is complete, extract and return the complete
    /// transaction in normal network serialization instead of the PSBT.
    ///
    /// For more information see: <https://developer.bitcoin.org/reference/rpc/finalizepsbt.html>
    async fn finalize_psbt(
        &self,
        psbt: &str,
        extract: Option<bool>,
    ) -> Result<json::FinalizePsbtResult> {
        let mut args = [into_json(psbt)?, opt_into_json(extract)?];
        self.call("finalizepsbt", handle_defaults(&mut args, &[true.into()]))
            .await
    }

    /// Derives one or more addresses corresponding to an output descriptor.
    ///
    /// # Parameters
    /// * descriptor - The descriptor.
    /// * range - If a ranged descriptor is used, this specifies the end or the range (in
    ///   (begin,end) notation) to derive.
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/deriveaddresses.html>
    async fn derive_addresses(
        &self,
        descriptor: &str,
        range: Option<[u32; 2]>,
    ) -> Result<Vec<Address<NetworkUnchecked>>> {
        let mut args = [into_json(descriptor)?, opt_into_json(range)?];
        self.call("deriveaddresses", handle_defaults(&mut args, &[null()]))
            .await
    }

    /// Rescan the local blockchain for wallet related transactions.
    ///
    /// # Parameters
    /// * start_from - block height where the rescan should start
    /// * stop_height - the last block height that should be scanned. If none is provided it will
    ///   rescan up to the tip at return time of this call.
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/rescanblockchain.html>
    async fn rescan_blockchain(
        &self,
        start_from: Option<usize>,
        stop_height: Option<usize>,
    ) -> Result<(usize, Option<usize>)> {
        let mut args = [opt_into_json(start_from)?, opt_into_json(stop_height)?];

        #[derive(Deserialize)]
        struct Response {
            pub start_height: usize,
            pub stop_height: Option<usize>,
        }
        let res: Response = self
            .call(
                "rescanblockchain",
                handle_defaults(&mut args, &[0.into(), null()]),
            )
            .await?;
        Ok((res.start_height, res.stop_height))
    }

    /// Returns statistics about the unspent transaction output set. This call may take some time.
    async fn get_tx_out_set_info(&self) -> Result<json::GetTxOutSetInfoResult> {
        self.call("gettxoutsetinfo", &[]).await
    }

    /// Returns information about network traffic, including bytes in, bytes out, and current time.
    async fn get_net_totals(&self) -> Result<json::GetNetTotalsResult> {
        self.call("getnettotals", &[]).await
    }

    /// Returns the estimated network hashes per second based on the last n blocks.
    async fn get_network_hash_ps(&self, nblocks: Option<u64>, height: Option<u64>) -> Result<f64> {
        let mut args = [opt_into_json(nblocks)?, opt_into_json(height)?];
        self.call(
            "getnetworkhashps",
            handle_defaults(&mut args, &[null(), null()]),
        )
        .await
    }

    /// Returns the total uptime of the server in seconds
    async fn uptime(&self) -> Result<u64> {
        self.call("uptime", &[]).await
    }

    /// EXPERIMENTAL warning: this call may be removed or changed in future releases.
    ///
    /// # Parameters:
    /// * descriptors - Array of scan objects. Required for “start” action
    ///
    /// For more details see: <https://developer.bitcoin.org/reference/rpc/scantxoutset.html>
    async fn scan_tx_out_set_blocking(
        &self,
        descriptors: &[json::ScanTxOutRequest],
    ) -> Result<json::ScanTxOutResult> {
        self.call("scantxoutset", &["start".into(), into_json(descriptors)?])
            .await
    }
}

#[cfg(any(test, feature = "mocks"))]
mockall::mock! {
    pub RpcApi { }

    #[async_trait]
    impl RpcApi for RpcApi {
        async fn call<T: for<'a> de::Deserialize<'a> + 'static>(&self, method: &str, params: &[serde_json::Value]) -> Result<T>;

        async fn get_raw_transaction(
            &self,
            txid: &bitcoin::Txid,
            block_hash: Option<bitcoin::BlockHash>,
        ) -> Result<Transaction>;

        async fn get_block_hash(&self, height: u64) -> Result<bitcoin::BlockHash>;

        async fn get_block_txs(&self, hash: &bitcoin::BlockHash) -> Result<json::GetBlockTxResult>;

        async fn get_best_block_hash(&self) -> Result<bitcoin::BlockHash>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_mock() {
        let _mock = MockRpcApi::new();
    }
}
