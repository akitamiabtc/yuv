use bitcoin::{Transaction, Txid};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use yuv_pixels::Chroma;
use yuv_types::announcements::ChromaInfo;
use yuv_types::{YuvTransaction, YuvTxType};

/// Response for [`getrawyuvtransaction`](YuvTransactionsRpcServer::get_raw_yuv_transaction) RPC
/// method.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "data")]
pub enum GetRawYuvTransactionResponse {
    /// Transaction is not found.
    ///
    /// Provided proof was rejected, or no proofs were provided yet.
    None,

    /// Transaction is found and it's raw data is provided, but it's in the queue to be checked.
    Pending,

    /// Transaction is found, it's raw data is provided, and it's checked, but node has
    /// no parent transactions to attach it.
    Checked,

    /// Transaction is found, it's raw data is provided, it's checked, and the node has
    /// all parent transactions to attach it.
    Attached(YuvTransaction),
}

/// Response for [`emulateyuvtransaction`](YuvTransactionsRpcServer::emulate_yuv_transaction) RPC
/// method that is defined for returning reason of transaction rejection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "data")]
pub enum EmulateYuvTransactionResponse {
    /// Transaction will be rejected by node for given reason.
    Invalid { reason: String },

    /// Transaction could be accepted by node.
    Valid,
}

impl EmulateYuvTransactionResponse {
    pub fn invalid(reason: String) -> Self {
        Self::Invalid { reason }
    }
}

/// A wrapper around [`bitcoin::blockdata::transaction`] that contains `Txid`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TransactionResponse {
    pub txid: Txid,

    #[serde(flatten)]
    pub bitcoin_tx: Transaction,
}

impl From<Transaction> for TransactionResponse {
    fn from(bitcoin_tx: Transaction) -> Self {
        Self {
            txid: bitcoin_tx.txid(),
            bitcoin_tx,
        }
    }
}

impl From<TransactionResponse> for Transaction {
    fn from(tx: TransactionResponse) -> Self {
        tx.bitcoin_tx
    }
}

impl From<YuvTransactionResponse> for YuvTransaction {
    fn from(response: YuvTransactionResponse) -> Self {
        Self::new(response.bitcoin_tx.into(), response.tx_type)
    }
}

impl From<YuvTransaction> for YuvTransactionResponse {
    fn from(tx: YuvTransaction) -> Self {
        Self {
            bitcoin_tx: tx.bitcoin_tx.into(),
            tx_type: tx.tx_type,
        }
    }
}

/// Response for [`listyuvtransactions`] RPC method that is defined for returning the list of
/// attached YUV transactions.
///
/// [`listyuvtransactions`]: YuvTransactionsRpcServer::list_yuv_transactions
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct YuvTransactionResponse {
    pub bitcoin_tx: TransactionResponse,
    pub tx_type: YuvTxType,
}

/// RPC methods for transactions.
#[cfg_attr(feature = "client", rpc(server, client))]
#[cfg_attr(not(feature = "client"), rpc(server))]
#[async_trait::async_trait]
pub trait YuvTransactionsRpc {
    /// Provide proofs to YUV transaction by id.
    #[method(name = "provideyuvproof")]
    async fn provide_yuv_proof(&self, yuv_tx: YuvTransaction) -> RpcResult<bool>;

    /// Provide YUV transactions to YUV node without submitting them on-chain.
    #[method(name = "providelistyuvproofs")]
    async fn provide_list_yuv_proofs(&self, yuv_txs: Vec<YuvTransaction>) -> RpcResult<bool>;

    /// Get YUV transaction by id and return its proofs.
    #[method(name = "getrawyuvtransaction")]
    async fn get_raw_yuv_transaction(&self, txid: Txid) -> RpcResult<GetRawYuvTransactionResponse>;

    /// Get list of YUV transactions by id and return its proofs. If requested transactions aren't
    /// exist the response array will be empty.
    #[method(name = "getlistrawyuvtransactions")]
    async fn get_list_raw_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<YuvTransaction>>;

    /// Get transaction list by page number.
    #[method(name = "listyuvtransactions")]
    async fn list_yuv_transactions(&self, page: u64) -> RpcResult<Vec<YuvTransactionResponse>>;

    /// Send YUV transaction to Bitcoin network.
    #[method(name = "sendrawyuvtransaction")]
    async fn send_raw_yuv_tx(
        &self,
        yuv_tx: YuvTransaction,
        max_burn_amount: Option<u64>,
    ) -> RpcResult<bool>;

    /// Check if YUV transaction is frozen or not.
    #[method(name = "isyuvtxoutfrozen")]
    async fn is_yuv_txout_frozen(&self, txid: Txid, vout: u32) -> RpcResult<bool>;

    /// Emulate transaction check and attach without actuall broadcasting or
    /// mining it to the network.
    ///
    /// This method is useful for checking if node can immidiatelly check and
    /// attach transaction to internal storage.
    #[method(name = "emulateyuvtransaction")]
    async fn emulate_yuv_transaction(
        &self,
        yuv_tx: YuvTransaction,
    ) -> RpcResult<EmulateYuvTransactionResponse>;

    /// Get the [ChromaInfo] that contains the information about the token.
    #[method(name = "getchromainfo")]
    async fn get_chroma_info(&self, chroma: Chroma) -> RpcResult<Option<ChromaInfo>>;
}
