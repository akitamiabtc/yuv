use async_trait::async_trait;
use bitcoin::{Amount, BlockHash, OutPoint, Txid};
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
    EmulateYuvTransactionResponse, GetRawYuvTransactionResponseHex,
    GetRawYuvTransactionResponseJson, ProvideYuvProofRequest, YuvTransactionResponse,
    YuvTransactionStatus, YuvTransactionsRpcServer,
};
use yuv_storage::{
    ChromaInfoStorage, FrozenTxsStorage, KeyValueError, MempoolEntryStorage, PagesStorage,
    TransactionsStorage,
};
use yuv_tx_check::{check_transaction, CheckError};
use yuv_types::{
    announcements::ChromaInfo, ControllerMessage, ProofMap, YuvTransaction, YuvTxType,
};

// TODO: Rename to "RpcController"
/// Controller for transactions from RPC.
pub struct TransactionsController<TransactionsStorage, StateStorage, BitcoinClient> {
    /// Max items per request
    max_items_per_request: usize,
    /// Internal storage of transactions.
    txs_storage: TransactionsStorage,
    /// Internal state storage.
    state_storage: StateStorage,
    /// Event bus for simplifying communication with services.
    event_bus: EventBus,
    /// Bitcoin RPC Client.
    bitcoin_client: Arc<BitcoinClient>,
}

impl<TS, SS, BC> TransactionsController<TS, SS, BC>
where
    TS: TransactionsStorage + PagesStorage + Send + Sync + 'static,
    SS: FrozenTxsStorage + ChromaInfoStorage + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn new(
        storage: TS,
        full_event_bus: EventBus,
        state_storage: SS,
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
            state_storage,
            bitcoin_client,
        }
    }
}

impl<TS, SS, BC> TransactionsController<TS, SS, BC>
where
    TS: TransactionsStorage + PagesStorage + Send + Sync + 'static,
    SS: FrozenTxsStorage + ChromaInfoStorage + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    async fn send_txs_to_confirm(&self, yuv_txs: Vec<YuvTransaction>) -> RpcResult<()> {
        // Send message to message handler about new tx with proof.
        self.event_bus
            .try_send(ControllerMessage::InitializeTxs(yuv_txs))
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
impl<TS, SS, BC> YuvTransactionsRpcServer for TransactionsController<TS, SS, BC>
where
    TS: TransactionsStorage + PagesStorage + Clone + Send + Sync + 'static,
    SS: FrozenTxsStorage + ChromaInfoStorage + MempoolEntryStorage + Clone + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    /// Handle new YUV transaction with proof to check.
    async fn provide_yuv_proof(&self, yuv_tx: YuvTransaction) -> RpcResult<bool> {
        // Send message to message handler to wait its confirmation.
        self.send_txs_to_confirm(vec![yuv_tx]).await?;

        Ok(true)
    }

    /// Handle new YUV transaction with proof to check.
    async fn provide_yuv_proof_short(
        &self,
        txid: Txid,
        tx_type: String,
        blockhash: Option<BlockHash>,
    ) -> RpcResult<bool> {
        let tx_type = YuvTxType::from_hex(tx_type).map_err(|err| {
            tracing::error!("Failed to parse tx type hex: {err}");
            ErrorObjectOwned::owned(
                INVALID_REQUEST_CODE,
                "Hex parse error",
                Option::<Vec<u8>>::None,
            )
        })?;

        self.provide_list_yuv_proofs(vec![ProvideYuvProofRequest::new(txid, tx_type, blockhash)])
            .await
    }

    async fn provide_list_yuv_proofs(
        &self,
        proofs: Vec<ProvideYuvProofRequest>,
    ) -> RpcResult<bool> {
        if proofs.len() > self.max_items_per_request {
            return Err(ErrorObject::owned(
                INVALID_REQUEST_CODE,
                format!(
                    "Too many yuv_txs, max amount is {}",
                    self.max_items_per_request
                ),
                Option::<Vec<u8>>::None,
            ));
        }

        let mut yuv_txs = Vec::with_capacity(proofs.len());
        for proof in proofs {
            let bitcoin_tx = self
                .bitcoin_client
                .get_raw_transaction(&proof.txid, proof.blockhash)
                .await
                .map_err(|err| {
                    tracing::error!("Failed to get raw Bitcoin transaction by txid: {err}");
                    ErrorObjectOwned::owned(
                        INTERNAL_ERROR_CODE,
                        "Service is dead",
                        Option::<Vec<u8>>::None,
                    )
                })?;

            let yuv_tx = YuvTransaction::new(bitcoin_tx, proof.tx_type);
            yuv_txs.push(yuv_tx);
        }

        // Send message to message handler to wait its confirmation.
        self.send_txs_to_confirm(yuv_txs).await?;

        Ok(true)
    }

    async fn get_raw_yuv_transaction(
        &self,
        txid: Txid,
    ) -> RpcResult<GetRawYuvTransactionResponseJson> {
        let mempool_entry = self
            .state_storage
            .get_mempool_entry(&txid)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get mempool entry: {e}");
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                )
            })?;

        if let Some(entry) = mempool_entry {
            return Ok(GetRawYuvTransactionResponseJson::new(
                YuvTransactionStatus::Pending,
                Some(entry.yuv_tx.into()),
            ));
        }

        let tx = self.txs_storage.get_yuv_tx(&txid).await.map_err(|e| {
            ErrorObject::owned(INTERNAL_ERROR_CODE, e.to_string(), Option::<Vec<u8>>::None)
        })?;

        match tx {
            Some(tx) => Ok(GetRawYuvTransactionResponseJson::new(
                YuvTransactionStatus::Attached,
                Some(tx.into()),
            )),
            None => Ok(GetRawYuvTransactionResponseJson::new(
                YuvTransactionStatus::None,
                None,
            )),
        }
    }

    async fn get_yuv_transaction(&self, txid: Txid) -> RpcResult<GetRawYuvTransactionResponseHex> {
        let mempool_entry = self
            .state_storage
            .get_mempool_entry(&txid)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get mempool entry: {e}");
                ErrorObject::owned(
                    INTERNAL_ERROR_CODE,
                    "Storage is not available",
                    Option::<Vec<u8>>::None,
                )
            })?;

        if let Some(entry) = mempool_entry {
            return Ok(GetRawYuvTransactionResponseHex::new(
                entry.status.into(),
                Some(entry.yuv_tx.into()),
            ));
        }

        let tx = self.txs_storage.get_yuv_tx(&txid).await.map_err(|e| {
            ErrorObject::owned(INTERNAL_ERROR_CODE, e.to_string(), Option::<Vec<u8>>::None)
        })?;

        match tx {
            Some(tx) => Ok(GetRawYuvTransactionResponseHex::new(
                YuvTransactionStatus::Attached,
                Some(tx.into()),
            )),
            None => Ok(GetRawYuvTransactionResponseHex::new(
                YuvTransactionStatus::None,
                None,
            )),
        }
    }

    async fn get_list_raw_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<YuvTransactionResponse>> {
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
                result.push(tx.into())
            };
        }

        Ok(result)
    }

    async fn get_list_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<GetRawYuvTransactionResponseHex>> {
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

        let mut result: Vec<GetRawYuvTransactionResponseHex> = Vec::new();

        for txid in txids {
            result.push(self.get_yuv_transaction(txid).await?)
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

    /// Send signed YUV transaction to Bitcoin network and validate it after it's confirmed.
    async fn send_yuv_tx(&self, yuv_tx: String, max_burn_amount: Option<u64>) -> RpcResult<bool> {
        let max_burn_amount_btc: Option<f64> = max_burn_amount
            .map(|max_burn_amount_sat| Amount::from_sat(max_burn_amount_sat).to_btc());

        let yuv_tx = YuvTransaction::from_hex(yuv_tx).map_err(|err| {
            tracing::error!("Failed to parse YUV tx hex: {err}");
            ErrorObjectOwned::owned(
                INVALID_REQUEST_CODE,
                "Hex parse error",
                Option::<Vec<u8>>::None,
            )
        })?;

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

    /// Send signed raw YUV transaction to Bitcoin network and validate it after it's confirmed.
    ///
    /// NOTE: this method will soon accept only hex encoded YUV txs.
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
        let freeze_entry = self
            .state_storage
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

        Ok(freeze_entry.is_some())
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
            TransactionEmulator::new(self.txs_storage.clone(), self.state_storage.clone());

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
        self.state_storage
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
/// 2. Transaction is not violating any conservation rules;
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
        let freeze_entry = self.frozen_txs_storage.get_frozen_tx(&parent).await?;

        if freeze_entry.is_some() {
            Err(EmulateYuvTransactionError::ParentTransactionFrozen {
                txid: parent.txid,
                vout: parent.vout,
            })
        } else {
            Ok(())
        }
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
