use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bitcoin::{OutPoint, TxIn, Txid};
use bitcoin_client::BitcoinRpcApi;
use event_bus::{typeid, EventBus};
use eyre::{eyre, Context, Result};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use yuv_pixels::{Chroma, PixelProof};
use yuv_storage::{ChromaInfoStorage, FrozenTxsStorage, InvalidTxsStorage, TransactionsStorage};
use yuv_types::announcements::{
    ChromaAnnouncement, ChromaInfo, FreezeAnnouncement, IssueAnnouncement,
    TransferOwnershipAnnouncement,
};
use yuv_types::messages::p2p::Inventory;
use yuv_types::{
    Announcement, ControllerMessage, GraphBuilderMessage, ProofMap, TxCheckerMessage,
    YuvTransaction, YuvTxType,
};

use crate::errors::CheckError;
use crate::isolated_checks::{
    check_issue_isolated, check_transfer_isolated, find_owner_in_txinputs,
};

const ATTEMPT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RETRY_ATTEMPTS: usize = 3;

pub struct Config<TxsStorage, StateStorage, BC> {
    pub full_event_bus: EventBus,
    pub txs_storage: TxsStorage,
    pub state_storage: StateStorage,
    pub bitcoin_client: Arc<BC>,
}

/// Async implementation of [`TxChecker`] for node implementation.
///
/// Accepts [`YuvTransaction`]s from channel, check them and sends to graph builder.
///
/// [`TxChecker`]: struct.TxChecker.html
pub struct TxCheckerWorker<TxsStorage, StateStorage, BC>
where
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    /// Index of the worker in the worker pool
    index: usize,

    /// Inner storage of already checked and attached transactions.
    pub(crate) txs_storage: TxsStorage,

    /// Storage for inner states of transactions.
    pub(crate) state_storage: StateStorage,

    /// Event bus for simplifying communication with services
    event_bus: EventBus,

    bitcoin_client: Arc<BC>,
}

impl<TS, SS, BC> TxCheckerWorker<TS, SS, BC>
where
    TS: TransactionsStorage + Clone + Send + Sync + 'static,
    SS: InvalidTxsStorage + FrozenTxsStorage + ChromaInfoStorage + Clone + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn from_config(config: &Config<TS, SS, BC>, index: Option<usize>) -> Self {
        let event_bus = config
            .full_event_bus
            .extract(
                &typeid![GraphBuilderMessage, ControllerMessage],
                &typeid![TxCheckerMessage],
            )
            .expect("event channels must be presented");

        Self {
            index: index.unwrap_or_default(),
            event_bus,
            txs_storage: config.txs_storage.clone(),
            state_storage: config.state_storage.clone(),
            bitcoin_client: Arc::clone(&config.bitcoin_client),
        }
    }

    pub async fn run(mut self, cancellation: CancellationToken) {
        let events = self.event_bus.subscribe::<TxCheckerMessage>();

        loop {
            tokio::select! {
                event_received = events.recv() => {
                    let Ok(event) = event_received else {
                        tracing::trace!(index = self.index, "All incoming events senders are dropped");
                        return;
                    };

                    if let Err(err) = self.handle_event(event).await {
                        tracing::error!(index = self.index, "Failed to handle an event: {}", err);

                        // Error usually occurs when there is no connection established with the
                        // Bitcoin RPC. In this case the node should gracefully shutdown.
                        cancellation.cancel()
                    }
                }
                _ = cancellation.cancelled() => {
                    tracing::trace!(index = self.index, "Cancellation received, stopping TxCheckerWorker");
                    return;
                }
            }
        }
    }

    async fn handle_event(&mut self, event: TxCheckerMessage) -> Result<()> {
        match event {
            TxCheckerMessage::NewTxs { txs, sender } => self
                .check_txs(txs, sender)
                .await
                .wrap_err("failed to check transactions")?,
        }

        Ok(())
    }

    /// Fully check the transaction depends on its type. It inform the controller about the invalid
    /// transactions or request missing parent transactions (in case of [`YuvTxType::Transfer`]).
    /// It also sends valid [`YuvTxType::Issue`] and [`YuvTxType::Transfer`]
    /// transactions to the graph builder.
    pub async fn check_txs(
        &mut self,
        txs: Vec<YuvTransaction>,
        peer_addr: Option<SocketAddr>,
    ) -> Result<()> {
        let mut checked_txs = BTreeMap::new();
        let mut invalid_txs = Vec::new();
        let mut not_found_parents = Vec::new();

        tracing::debug!("Checking txs: {:?}", txs);

        for tx in txs {
            let is_valid = self
                .check_transaction(
                    tx.clone(),
                    &mut invalid_txs,
                    &mut checked_txs,
                    &mut not_found_parents,
                )
                .await?;

            // There is no sense to put it into storage or mark as an invalid tx if it's an
            // announcement.
            if let YuvTxType::Announcement { .. } = &tx.tx_type {
                continue;
            }

            if !is_valid {
                invalid_txs.push(tx.clone());
                continue;
            }

            checked_txs.insert(tx.bitcoin_tx.txid(), tx);
        }

        // Send checked transactions to next worker:
        if !checked_txs.is_empty() {
            self.event_bus
                .send(GraphBuilderMessage::CheckedTxs(
                    checked_txs.values().cloned().collect::<Vec<_>>(),
                ))
                .await;
        }

        // Notify about invalid transactions:
        if !invalid_txs.is_empty() {
            let invalid_txs_ids = invalid_txs.iter().map(|tx| tx.bitcoin_tx.txid()).collect();
            self.event_bus
                .send(ControllerMessage::InvalidTxs {
                    tx_ids: invalid_txs_ids,
                    sender: peer_addr,
                })
                .await;

            self.state_storage.put_invalid_txs(invalid_txs).await?;
        }

        // If there is no info about parent transactions, request them:
        if !not_found_parents.is_empty() {
            let Some(addr) = peer_addr else { return Ok(()) };

            let inventory = not_found_parents
                .iter()
                .map(|txid| Inventory::Ytx(*txid))
                .collect();

            let get_data_msg = ControllerMessage::GetData {
                inv: inventory,
                receiver: addr,
            };

            self.event_bus.send(get_data_msg).await;
        }

        Ok(())
    }

    /// Do the corresponding checks for the transaction based on its type.
    async fn check_transaction(
        &mut self,
        tx: YuvTransaction,
        invalid_txs: &mut Vec<YuvTransaction>,
        checked_txs: &mut BTreeMap<Txid, YuvTransaction>,
        not_found_parents: &mut Vec<Txid>,
    ) -> Result<bool> {
        let is_valid = match &tx.tx_type {
            YuvTxType::Issue {
                announcement,
                output_proofs,
            } => {
                self.check_issuance(&tx, output_proofs, announcement)
                    .await?
            }
            YuvTxType::Announcement(announcement) => {
                self.check_announcements(&tx, announcement, invalid_txs)
                    .await?
            }
            YuvTxType::Transfer {
                ref input_proofs,
                output_proofs,
            } => {
                self.check_transfer(
                    &tx,
                    input_proofs,
                    output_proofs,
                    checked_txs,
                    not_found_parents,
                )
                .await?
            }
        };

        Ok(is_valid)
    }

    async fn check_issuance(
        &self,
        tx: &YuvTransaction,
        output_proofs: &Option<ProofMap>,
        announcement: &IssueAnnouncement,
    ) -> Result<bool> {
        if !self.check_issue_announcement(tx, announcement).await? {
            return Ok(false);
        }

        if check_issue_isolated(&tx.bitcoin_tx, output_proofs, announcement).is_err() {
            return Ok(false);
        }

        self.txs_storage.put_yuv_tx(tx.clone()).await?;

        Ok(true)
    }

    async fn check_transfer(
        &mut self,
        tx: &YuvTransaction,
        input_proofs: &ProofMap,
        output_proofs: &ProofMap,
        checked_txs: &BTreeMap<Txid, YuvTransaction>,
        not_found_parents: &mut Vec<Txid>,
    ) -> Result<bool> {
        if check_transfer_isolated(&tx.bitcoin_tx, input_proofs, output_proofs).is_err() {
            return Ok(false);
        }

        for (parent_id, proof) in input_proofs {
            let Some(txin) = tx.bitcoin_tx.input.get(*parent_id as usize) else {
                return Err(CheckError::InputNotFound.into());
            };

            let parent = txin.previous_output;

            if self.is_output_frozen(&parent, proof).await? {
                tracing::info!(
                    index = self.index,
                    "Transfer tx {} is invalid: output {} is frozen",
                    tx.bitcoin_tx.txid(),
                    parent,
                );

                return Ok(false);
            }

            let is_in_storage = self.txs_storage.get_yuv_tx(&parent.txid).await?.is_some();
            if !is_in_storage && !checked_txs.contains_key(&parent.txid) {
                not_found_parents.push(parent.txid);
            }
        }

        Ok(true)
    }

    /// Check if transaction is frozen.
    async fn is_output_frozen(&self, outpoint: &OutPoint, proof: &PixelProof) -> Result<bool> {
        let chroma = &proof.pixel().chroma;

        if let Some(chroma_info) = self.state_storage.get_chroma_info(chroma).await? {
            if let Some(announcement) = chroma_info.announcement {
                if !announcement.is_freezable {
                    return Ok(false);
                }
            }
        }

        let freeze_entry = self.state_storage.get_frozen_tx(outpoint).await?;

        // Issuer haven't attempted to freeze this output, so it's not frozen:
        let Some(freeze_entry) = freeze_entry else {
            return Ok(false);
        };

        let mut checked_freezes = Vec::new();

        // TODO: optimize this approach.
        for freeze_txid in &freeze_entry.tx_ids {
            let freeze_tx = self
                .txs_storage
                .get_yuv_tx(freeze_txid)
                .await?
                .ok_or_else(|| eyre!("Freeze tx not found, {}", freeze_txid))?;

            let owner_input = self
                .find_owner_in_txinputs(&freeze_tx.bitcoin_tx.input, chroma)
                .await?;
            if owner_input.is_none() {
                tracing::info!(
                    index = self.index,
                    tx = freeze_txid.to_string(),
                    "Freeze tx is invalid: none of the inputs has owner, removing it",
                );

                self.txs_storage.delete_yuv_tx(freeze_txid).await?;

                continue;
            }

            checked_freezes.push(*freeze_txid);
        }

        let is_frozen = checked_freezes.len() % 2 == 1;

        self.state_storage
            .put_frozen_tx(outpoint, checked_freezes)
            .await?;

        Ok(is_frozen)
    }

    /// Check that all the [`Announcement`]s in transcation are valid.
    ///
    /// For more details see checks for specific types of announcement.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - if all the announcements are valid.
    /// - `Ok(false)` - if at least one of the announcements is invalid.
    /// - `Err(err)` - if an error occurred during the check.
    async fn check_announcements(
        &self,
        tx: &YuvTransaction,
        announcement: &Announcement,
        invalid_txs: &mut Vec<YuvTransaction>,
    ) -> Result<bool> {
        let is_checked = match announcement {
            Announcement::Chroma(announcement) => {
                self.check_chroma_announcement(tx, announcement).await?
            }
            Announcement::Freeze(announcement) => {
                self.check_freeze_announcement(tx, announcement).await?
            }
            Announcement::Issue(announcement) => {
                self.check_issue_announcement(tx, announcement).await?
            }
            Announcement::TransferOwnership(announcement) => {
                self.check_transfer_ownership_announcement(tx, announcement)
                    .await?
            }
        };

        self.event_bus
            .send(ControllerMessage::CheckedAnnouncement(tx.bitcoin_tx.txid()))
            .await;

        if !is_checked {
            invalid_txs.push(tx.clone());
            return Ok(false);
        }

        Ok(true)
    }

    /// Check that [ChromaAnnouncement] is valid.
    ///
    /// The chroma announcement is considered valid if:
    /// 1. One of the inputs of the announcement transaction is signed by the issuer of the chroma.
    /// 2. Max supply is bigger than the current total supply.
    async fn check_chroma_announcement(
        &self,
        announcement_tx: &YuvTransaction,
        announcement: &ChromaAnnouncement,
    ) -> Result<bool> {
        let announcement_tx_inputs = &announcement_tx.bitcoin_tx.input;
        let chroma = &announcement.chroma;

        let owner_input = self
            .find_owner_in_txinputs(announcement_tx_inputs, chroma)
            .await?;
        if owner_input.is_none() {
            tracing::debug!(
                index = self.index,
                tx = announcement_tx.bitcoin_tx.txid().to_string(),
                "Chroma announcement tx is invalid: none of the inputs has owner, removing it",
            );

            return Ok(false);
        }

        if let Some(chroma_info) = self
            .state_storage
            .get_chroma_info(&announcement.chroma)
            .await?
        {
            if chroma_info.total_supply > announcement.max_supply {
                tracing::debug!(
                    index = self.index,
                    "Chroma announcement tx {} is invalid: current total supply {} exceeds max supply {}",
                    announcement_tx.bitcoin_tx.txid(),
                    chroma_info.total_supply,
                    announcement.max_supply,
                );

                return Ok(false);
            }
        };

        self.add_chroma_announcements(announcement).await?;

        Ok(true)
    }

    /// Check that [FrezeAnnouncement] is valid.
    ///
    /// The freeze announcement is considered valid if:
    /// 1. The transaction that is being frozen exists in the storage. If the output that is being
    /// frozen doesn't exist in the transaction then it's an invalid freeze announcement. But we
    /// can just skip it because it doesn't break the protocol's rules.
    /// 2. The output that is being frozen is an existing YUV output.
    /// 3. One of the inputs of the announcement freeze transaction is signed by the owner of the
    /// chroma that is being frozen.
    /// 4. The freezes are allowed by the Chroma announcement.
    async fn check_freeze_announcement(
        &self,
        announcement_tx: &YuvTransaction,
        announcement: &FreezeAnnouncement,
    ) -> Result<bool> {
        let freeze_txid = announcement.freeze_txid();
        let freeze_vout = announcement.freeze_vout();

        let Some(freeze_tx) = self.txs_storage.get_yuv_tx(&freeze_txid).await? else {
            // TODO: If there is no transactions, worker should wait its appearance for check.
            return Ok(true);
        };

        let Some(output_proofs) = get_output_proofs(&freeze_tx) else {
            // If the announcement output is being frozen then it's an invalid freeze announcement.
            // But we can just skip it because it doesn't break the protocol's rules.
            tracing::debug!(
                index = self.index,
                "Freeze tx {} tries to freeze an announcement {}. Ignore it.",
                announcement_tx.bitcoin_tx.txid(),
                freeze_txid,
            );

            return Ok(true);
        };

        let Some(output) = output_proofs.get(&freeze_vout) else {
            // If the output that is being frozen doesn't exist then it's an invalid freeze
            // announcement. But we can just skip it because it doesn't break the protocol's rules.
            tracing::debug!(
                index = self.index,
                "Freeze tx {} tries to freeze an unexisting output {}. Ignore it.",
                announcement_tx.bitcoin_tx.txid(),
                announcement.freeze_outpoint(),
            );

            return Ok(true);
        };

        let chroma = &output.pixel().chroma;
        if let Some(chroma_info) = self.state_storage.get_chroma_info(chroma).await? {
            if let Some(chroma_announcement) = chroma_info.announcement {
                if !chroma_announcement.is_freezable {
                    tracing::info!(
                        index = self.index,
                        "Freeze tx {} is invalid: chroma {} doesn't allow freezes, removing it",
                        freeze_txid,
                        chroma,
                    );

                    return Ok(false);
                }
            }
        }

        // Check signer of the freeze tx is issuer of the chroma which frozen tx has.
        let owner_input = self
            .find_owner_in_txinputs(&announcement_tx.bitcoin_tx.input, chroma)
            .await?;
        if owner_input.is_none() {
            tracing::info!(
                index = self.index,
                tx = freeze_txid.to_string(),
                "Freeze tx is invalid: none of the inputs has owner, removing it",
            );

            return Ok(false);
        }

        self.update_freezes(announcement_tx.bitcoin_tx.txid(), announcement)
            .await?;

        Ok(true)
    }

    /// Check that [IssueAnnouncement] is valid.
    ///
    /// The issue announcement is considered valid if:
    /// 1. One of the inputs of the issue announcement transaction is signed by the owner
    /// of the chroma.
    /// 2. Issue amount doesn't exceed the max supply specified in the chroma announcement
    /// (if announced).
    async fn check_issue_announcement(
        &self,
        announcement_yuv_tx: &YuvTransaction,
        announcement: &IssueAnnouncement,
    ) -> Result<bool> {
        let announcement_tx = &announcement_yuv_tx.bitcoin_tx;
        let chroma = &announcement.chroma;
        let issue_amount = announcement.amount;

        let is_tx_already_exists = self
            .txs_storage
            .get_yuv_tx(&announcement_tx.txid())
            .await?
            .is_some();
        if is_tx_already_exists {
            return Ok(true);
        }

        let owner_input = self
            .find_owner_in_txinputs(&announcement_tx.input, chroma)
            .await?;
        if owner_input.is_none() {
            tracing::debug!(
                index = self.index,
                tx = announcement_yuv_tx.bitcoin_tx.txid().to_string(),
                "Issue announcement tx is invalid: none of the inputs has owner, removing it",
            );

            return Ok(false);
        }

        // Bulletproof issuance announcements don't update the total supply so they
        // can be skipped.
        // Non-bulletproof issuance must be checked.
        #[cfg(feature = "bulletproof")]
        if announcement_yuv_tx.is_bulletproof() {
            self.handle_checked_issue_announcement(announcement_yuv_tx, announcement)
                .await?;

            return Ok(true);
        }

        let chroma_info_opt = self.state_storage.get_chroma_info(chroma).await?;
        if let Some(ChromaInfo {
            announcement: Some(ChromaAnnouncement { max_supply, .. }),
            total_supply,
            ..
        }) = chroma_info_opt
        {
            let new_total_supply = total_supply + issue_amount;

            if max_supply != 0 && max_supply < new_total_supply {
                tracing::info!(
                    index = self.index,
                    "Issue announcement tx {} is invalid: current supply {} + announcement amount {} is higher than the max supply {}",
                    announcement_tx.txid(),
                    total_supply,
                    issue_amount,
                    max_supply,
                );

                return Ok(false);
            }
        }

        self.handle_checked_issue_announcement(announcement_yuv_tx, announcement)
            .await?;

        Ok(true)
    }

    async fn handle_checked_issue_announcement(
        &self,
        announcement_yuv_tx: &YuvTransaction,
        announcement: &IssueAnnouncement,
    ) -> Result<()> {
        self.update_supply(announcement).await?;
        self.txs_storage
            .put_yuv_tx(announcement_yuv_tx.clone())
            .await?;

        Ok(())
    }

    /// Check that [TransferOwnershipAnnouncement] is valid.
    ///
    /// The transfer ownership announcement is considered valid if one of the inputs of the
    /// announcement transaction is signed by the current owner of the chroma.
    async fn check_transfer_ownership_announcement(
        &self,
        announcement_yuv_tx: &YuvTransaction,
        announcement: &TransferOwnershipAnnouncement,
    ) -> Result<bool> {
        let announcement_tx = &announcement_yuv_tx.bitcoin_tx;
        let chroma = &announcement.chroma;

        let owner_input = self
            .find_owner_in_txinputs(&announcement_tx.input, chroma)
            .await?;
        if owner_input.is_none() {
            tracing::debug!(
                index = self.index,
                tx = announcement_yuv_tx.bitcoin_tx.txid().to_string(),
                "Transfer ownership announcement tx is invalid: none of the inputs has owner, removing it",
            );

            return Ok(false);
        }

        self.update_owner(announcement).await?;

        tracing::debug!("Changed owner for chroma {}", announcement.chroma);

        Ok(true)
    }

    /// Find owner of the `Chroma` in the inputs.
    async fn find_owner_in_txinputs<'a>(
        &self,
        inputs: &'a [TxIn],
        chroma: &Chroma,
    ) -> eyre::Result<Option<&'a TxIn>> {
        let chroma_info = self.state_storage.get_chroma_info(chroma).await?;

        // Retry finding owner in the inputs if the connection with the Bitcoin RPC is lost.
        // If all the attempts are unsuccessful, gracefully shutdown the node.
        // TODO: this approach should be refactored in the future.
        for i in 0..MAX_RETRY_ATTEMPTS {
            println!("Attempt {}", i);
            let owner_input = find_owner_in_txinputs(
                Arc::clone(&self.bitcoin_client),
                inputs,
                chroma,
                chroma_info.clone(),
            )
            .await;

            match owner_input {
                Ok(owner_input_opt) => return Ok(owner_input_opt),
                Err(err) => {
                    if i == MAX_RETRY_ATTEMPTS - 1 {
                        return Err(err);
                    }
                }
            }

            sleep(ATTEMPT_INTERVAL).await;
        }

        Ok(None)
    }
}

fn get_output_proofs(yuv_tx: &YuvTransaction) -> Option<&ProofMap> {
    match yuv_tx.tx_type {
        YuvTxType::Issue {
            ref output_proofs, ..
        } => output_proofs.as_ref(),
        YuvTxType::Transfer {
            ref output_proofs, ..
        } => Some(output_proofs),
        YuvTxType::Announcement(_) => None,
    }
}
