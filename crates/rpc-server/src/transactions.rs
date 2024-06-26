use async_trait::async_trait;
use bitcoin::{Amount, OutPoint, Txid};
use bitcoin_client::BitcoinRpcApi;
use event_bus::{typeid, EventBus};
use jsonrpsee::{
    core::RpcResult,
    types::{
        error::{INTERNAL_ERROR_CODE, INVALID_REQUEST_CODE},
        ErrorObject, ErrorObjectOwned,
    },
};
use std::sync::Arc;
use yuv_pixels::Chroma;
use yuv_rpc_api::transactions::{
    EmulateYuvTransactionResponse, GetRawYuvTransactionResponse, YuvTransactionResponse,
    YuvTransactionsRpcServer,
};
use yuv_storage::{
    ChromaInfoStorage, FrozenTxsStorage, KeyValueError, PagesStorage, TransactionsStorage, TxState,
    TxStatesStorage,
};
use yuv_tx_check::{check_transaction, CheckError};
use yuv_types::{
    announcements::ChromaInfo, ControllerMessage, ProofMap, YuvTransaction, YuvTxType,
};

// TODO: Rename to "RpcController"
/// Controller for transactions from RPC.
pub struct TransactionsController<TransactionsStorage, AnnouncementStorage, BitcoinClient> {
    /// Max items per request
    max_items_per_request: usize,
    /// Internal storage of transactions.
    txs_storage: TransactionsStorage,
    /// Internal storage for announcements.
    announcement_storage: AnnouncementStorage,
    /// Event bus for simplifying communication with services.
    event_bus: EventBus,
    /// Internal storage of transactions' states.
    txs_states_storage: TxStatesStorage,
    /// Bitcoin RPC Client.
    bitcoin_client: Arc<BitcoinClient>,
}

impl<TXS, AS, BC> TransactionsController<TXS, AS, BC>
where
    TXS: TransactionsStorage + PagesStorage + Send + Sync + 'static,
    AS: FrozenTxsStorage + ChromaInfoStorage + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn new(
        storage: TXS,
        full_event_bus: EventBus,
        txs_states_storage: TxStatesStorage,
        frozen_txs_storage: AS,
        bitcoin_client: Arc<BC>,
        max_items_per_request: usize,
    ) -> Self {
        let event_bus = full_event_bus
            .extract(&typeid![ControllerMessage], &typeid![])
            .expect("event channels must be presented");

        Self {
            max_items_per_request,
            txs_storage: storage,
            event_bus,
            txs_states_storage,
            announcement_storage: frozen_txs_storage,
            bitcoin_client,
        }
    }
}

impl<TXS, FZS, BC> TransactionsController<TXS, FZS, BC>
where
    TXS: TransactionsStorage + PagesStorage + Send + Sync + 'static,
    FZS: FrozenTxsStorage + ChromaInfoStorage + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    async fn send_txs_to_confirm(&self, yuv_txs: Vec<YuvTransaction>) -> RpcResult<()> {
        // Send message to message handler about new tx with proof.
        self.event_bus
            .try_send(ControllerMessage::ConfirmBatchTx(yuv_txs))
            .await
            // If we failed to send message to message handler, then it's dead.
            .map_err(|_| {
                tracing::error!("failed to send message to message handler");
                ErrorObjectOwned::owned(
                    INTERNAL_ERROR_CODE,
                    "Service is dead",
                    Option::<Vec<u8>>::None,
                )
            })?;

        Ok(())
    }
}

#[async_trait]
impl<TXS, AS, BC> YuvTransactionsRpcServer for TransactionsController<TXS, AS, BC>
where
    TXS: TransactionsStorage + PagesStorage + Clone + Send + Sync + 'static,
    AS: FrozenTxsStorage + ChromaInfoStorage + Clone + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    /// Handle new YUV transaction with proof to check.
    async fn provide_yuv_proof(&self, yuv_tx: YuvTransaction) -> RpcResult<bool> {
        // Send message to message handler to wait its confirmation.
        self.send_txs_to_confirm(vec![yuv_tx]).await?;

        Ok(true)
    }

    async fn provide_list_yuv_proofs(&self, yuv_txs: Vec<YuvTransaction>) -> RpcResult<bool> {
        if yuv_txs.len() > self.max_items_per_request {
            return Err(ErrorObject::owned(
                INVALID_REQUEST_CODE,
                format!(
                    "Too many yuv_txs, max amount is {}",
                    self.max_items_per_request
                ),
                Option::<Vec<u8>>::None,
            ));
        }

        self.send_txs_to_confirm(yuv_txs).await?;

        Ok(true)
    }

    async fn get_raw_yuv_transaction(&self, txid: Txid) -> RpcResult<GetRawYuvTransactionResponse> {
        if let Some(state) = self.txs_states_storage.get(&txid).await {
            return match state {
                TxState::Pending => Ok(GetRawYuvTransactionResponse::Pending),
                TxState::Checked => Ok(GetRawYuvTransactionResponse::Checked),
            };
        }

        let tx = self.txs_storage.get_yuv_tx(&txid).await.map_err(|e| {
            ErrorObject::owned(INTERNAL_ERROR_CODE, e.to_string(), Option::<Vec<u8>>::None)
        })?;

        match tx {
            Some(tx) => Ok(GetRawYuvTransactionResponse::Attached(tx)),
            None => Ok(GetRawYuvTransactionResponse::None),
        }
    }

    async fn get_list_raw_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<YuvTransaction>> {
        if txids.len() > self.max_items_per_request {
            return Err(ErrorObject::owned(
                INVALID_REQUEST_CODE,
                format!(
                    "Too many txids, max amount is {}",
                    self.max_items_per_request
                ),
                Option::<Vec<u8>>::None,
            ));
        }

        let mut result = Vec::new();

        for txid in &txids {
            let tx = self.txs_storage.get_yuv_tx(txid).await.map_err(|e| {
                ErrorObject::owned(INTERNAL_ERROR_CODE, e.to_string(), Option::<Vec<u8>>::None)
            })?;

            if let Some(tx) = tx {
                result.push(tx)
            };
        }

        Ok(result)
    }

    async fn list_yuv_transactions(&self, page: u64) -> RpcResult<Vec<YuvTransactionResponse>> {
        let transactions = match self.txs_storage.get_page_by_num(page).await {
            Ok(Some(page)) => page,

            // If no transactions for this page, return empty list.
            Ok(None) => return Ok(Vec::new()),

            // If we failed to get page, then storage is not available.
            Err(err) => {
                tracing::error!("Failed to get last page: {err}");

                return Err(ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                ));
            }
        };

        let mut res: Vec<YuvTransactionResponse> = Vec::new();

        for txid in transactions {
            match self.txs_storage.get_yuv_tx(&txid).await {
                // if everything is ok, push transaction to result.
                Ok(Some(tx)) => res.push(tx.into()),
                // if transaction not found, then it's not valid.
                //
                // TODO: Maybe we should return error here?
                Ok(None) => {
                    tracing::error!("Transaction with id {txid} not found in page storage");
                    continue;
                }
                // if we failed to get transaction, then storage is not available.
                //
                // TODO: Maybe we should return error here?
                Err(err) => {
                    tracing::error!("Failed to get transaction with id {txid}: {err}");
                    continue;
                }
            }
        }

        Ok(res)
    }

    /// Send provided signed YUV transaction to Bitcoin network and validated it after it confirmed.
    async fn send_raw_yuv_tx(
        &self,
        yuv_tx: YuvTransaction,
        max_burn_amount_sat: Option<u64>,
    ) -> RpcResult<bool> {
        let max_burn_amount_btc: Option<f64> = max_burn_amount_sat
            .map(|max_burn_amount_sat| Amount::from_sat(max_burn_amount_sat).to_btc());

        self.bitcoin_client
            .send_raw_transaction_opts(&yuv_tx.bitcoin_tx, None, max_burn_amount_btc)
            .await
            .map_err(|err| {
                tracing::error!("Failed to send transaction to Bitcoin network: {err}");
                ErrorObjectOwned::owned(
                    INTERNAL_ERROR_CODE,
                    "Service is dead",
                    Option::<Vec<u8>>::None,
                )
            })?;

        // Send message to message handler to wait its confirmation.
        self.send_txs_to_confirm(vec![yuv_tx]).await?;

        Ok(true)
    }

    async fn is_yuv_txout_frozen(&self, txid: Txid, vout: u32) -> RpcResult<bool> {
        let frozen_state = self
            .announcement_storage
            .get_frozen_tx(&OutPoint::new(txid, vout))
            .await
            .map_err(|e| {
                tracing::error!("Failed to get frozen tx: {e}");
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                )
            })?;

        let Some(frozen_entry) = frozen_state else {
            return Ok(false);
        };

        Ok(frozen_entry.is_frozen())
    }

    /// Check that transaction could be accpeted by node.
    ///
    /// For that uses [`TransactionEmulator`] to check that transaction is valid
    /// ([see](TransactionEmulator::emulate_yuv_transaction))) for more info.
    async fn emulate_yuv_transaction(
        &self,
        yuv_tx: YuvTransaction,
    ) -> RpcResult<EmulateYuvTransactionResponse> {
        let emulator =
            TransactionEmulator::new(self.txs_storage.clone(), self.announcement_storage.clone());

        match emulator.emulate_yuv_transaction(&yuv_tx).await {
            // Transaction could be accepted by node.
            Ok(()) => Ok(EmulateYuvTransactionResponse::Valid),
            // Storage is dead:
            Err(EmulateYuvTransactionError::StorageNotAvailable(err)) => {
                tracing::error!("Storage error: {err}");

                Err(ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                ))
            }
            // Error that encountered during emulating:
            Err(err) => Ok(EmulateYuvTransactionResponse::Invalid {
                reason: err.to_string(),
            }),
        }
    }

    async fn get_chroma_info(&self, chroma: Chroma) -> RpcResult<Option<ChromaInfo>> {
        self.announcement_storage
            .get_chroma_info(&chroma)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get chroma info: {e}");
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                )
            })
    }
}

/// Entity that emulates transactions by checking if the one violates any of
/// this checks:
///
/// 1. All proofs are valid for this transaction;
/// 2. Transaction is not violating any consideration rules;
/// 3. None of the inputs are already frozen;
/// 4. That all parents are already attached in internal node storage.
///
/// If any of them encountered, return an error on method [`emulate_yuv_transaction`].
///
/// [`emulate_yuv_transaction`]: TransactionEmulator::emulate_yuv_transaction
// TODO: This could be moved to separate module.
pub struct TransactionEmulator<TransactionStorage, FreezesStorage> {
    /// Internal storage of transactions.
    txs_storage: TransactionStorage,

    /// Internal storage of frozen transactions.
    frozen_txs_storage: FreezesStorage,
}

#[derive(Debug, thiserror::Error)]
pub enum EmulateYuvTransactionError {
    #[error("Transaction check error: {0}")]
    CheckFailed(#[from] CheckError),

    #[error("Parent transaction is not found: {txid}")]
    ParentTransactionNotFound { txid: Txid },

    #[error("Parent UTXO is not found: {txid}:{vout}")]
    ParentUtxoNotFound { txid: Txid, vout: u32 },

    #[error("Parent transaction is frozen: {txid}:{vout}")]
    ParentTransactionFrozen { txid: Txid, vout: u32 },

    #[error("Storage is not available: {0}")]
    StorageNotAvailable(#[from] KeyValueError),
}

impl<TXS, FZS> TransactionEmulator<TXS, FZS>
where
    TXS: TransactionsStorage + Send + Sync + 'static,
    FZS: FrozenTxsStorage + Send + Sync + 'static,
{
    pub fn new(txs_storage: TXS, frozen_txs_storage: FZS) -> Self {
        Self {
            txs_storage,
            frozen_txs_storage,
        }
    }

    /// Emulate transaction check and attach without actuall broadcasting or
    /// mining. See [`TransactionEmulator`] for more info.
    pub async fn emulate_yuv_transaction(
        &self,
        yuv_tx: &YuvTransaction,
    ) -> Result<(), EmulateYuvTransactionError> {
        // Check first two bullets.
        check_transaction(yuv_tx)?;

        let Some(parents) = extract_parents(yuv_tx) else {
            return Ok(());
        };

        self.check_parents(parents).await?;

        Ok(())
    }

    /// Check that all parent transactions are not spent or frozen.
    async fn check_parents(
        &self,
        parents: Vec<OutPoint>,
    ) -> Result<(), EmulateYuvTransactionError> {
        use EmulateYuvTransactionError as Error;

        for parent in parents {
            let tx_entry = self.txs_storage.get_yuv_tx(&parent.txid).await?;

            // Return an error if parent transaction not found.
            let Some(tx) = tx_entry else {
                return Err(Error::ParentTransactionNotFound { txid: parent.txid });
            };

            let Some(output_proofs) = tx.tx_type.output_proofs() else {
                continue;
            };

            // Return an error if parent transaction output not found.
            if output_proofs.get(&parent.vout).is_none() {
                return Err(Error::ParentUtxoNotFound {
                    txid: parent.txid,
                    vout: parent.vout,
                });
            }

            // Return an error if parent transaction output is already frozen.
            self.is_parent_frozen(parent).await?;
        }

        Ok(())
    }

    /// Check if parent UTXO is frozen or not.
    async fn is_parent_frozen(&self, parent: OutPoint) -> Result<(), EmulateYuvTransactionError> {
        let frozen_entry = self
            .frozen_txs_storage
            .get_frozen_tx(&OutPoint::new(parent.txid, parent.vout))
            .await?;

        let Some(frozen_entry) = frozen_entry else {
            return Ok(());
        };

        if frozen_entry.is_frozen() {
            return Err(EmulateYuvTransactionError::ParentTransactionFrozen {
                txid: parent.txid,
                vout: parent.vout,
            });
        }

        Ok(())
    }
}

fn extract_parents(yuv_tx: &YuvTransaction) -> Option<Vec<OutPoint>> {
    match &yuv_tx.tx_type {
        // Issuance check was above, so we skip it.
        YuvTxType::Issue { .. } => None,
        // In case of transfer, parent transaction are one that are used as
        // inputs in input proofs.
        YuvTxType::Transfer {
            ref input_proofs, ..
        } => collect_transfer_parents(yuv_tx, input_proofs).into(),
        // In case of freezes, parent transaction are one that are being frozen.
        YuvTxType::Announcement(_) => {
            tracing::warn!("Announcement emulating is not implemented yet");
            None
        }
    }
}

/// Extract outpoint from inputs that are in the input proofs.
fn collect_transfer_parents(yuv_tx: &YuvTransaction, input_proofs: &ProofMap) -> Vec<OutPoint> {
    yuv_tx
        .bitcoin_tx
        .input
        .iter()
        .enumerate()
        .filter_map(|(vin, input)| {
            input_proofs
                .get(&(vin as u32))
                .map(|_proof| input.previous_output)
        })
        .collect::<Vec<_>>()
}
