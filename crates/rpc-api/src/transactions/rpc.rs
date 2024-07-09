use jsonrpsee::proc_macros::rpc;

#[cfg(feature = "server")]
use jsonrpsee::core::RpcResult;

use yuv_pixels::Chroma;
use yuv_types::announcements::ChromaInfo;
use yuv_types::YuvTransaction;

use crate::transactions::{
    BlockHash, EmulateYuvTransactionResponse, GetRawYuvTransactionResponseJson,
    ProvideYuvProofRequest, Txid, YuvTransactionResponse,
};

use super::GetRawYuvTransactionResponseHex;

/// RPC methods for transactions.
#[cfg_attr(all(feature = "client", not(feature = "server")), rpc(client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[async_trait::async_trait]
pub trait YuvTransactionsRpc {
    /// Provide YUV proofs to YUV transaction by full YUV transaction.
    #[method(name = "provideyuvproof")]
    async fn provide_yuv_proof(&self, yuv_tx: YuvTransaction) -> RpcResult<bool>;

    /// Provide proofs to YUV transaction by YUV proofs and Txid.
    #[method(name = "provideyuvproofshort")]
    async fn provide_yuv_proof_short(
        &self,
        txid: Txid,
        tx_type: String,
        blockhash: Option<BlockHash>,
    ) -> RpcResult<bool>;

    /// Provide YUV transactions to YUV node without submitting them on-chain.
    #[method(name = "providelistyuvproofs")]
    async fn provide_list_yuv_proofs(&self, proofs: Vec<ProvideYuvProofRequest>)
        -> RpcResult<bool>;

    /// Get YUV transaction by id and return its proofs.
    #[method(name = "getrawyuvtransaction")]
    #[deprecated(since = "0.6.0", note = "use `getyuvtransaction` instead")]
    async fn get_raw_yuv_transaction(
        &self,
        txid: Txid,
    ) -> RpcResult<GetRawYuvTransactionResponseJson>;

    /// Get HEX encoded YUV transaction by id and return its proofs.
    #[method(name = "getyuvtransaction")]
    async fn get_yuv_transaction(&self, txid: Txid) -> RpcResult<GetRawYuvTransactionResponseHex>;

    /// Get list of YUV transactions by id and return its proofs. If requested transactions aren't
    /// exist the response array will be empty.
    #[method(name = "getlistrawyuvtransactions")]
    async fn get_list_raw_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<YuvTransactionResponse>>;

    /// Get list of YUV transactions by id and return its proofs encoded in hex and status.
    /// If requested transactions aren't exist the response array will be empty.
    #[method(name = "getlistyuvtransactions")]
    async fn get_list_yuv_transactions(
        &self,
        txids: Vec<Txid>,
    ) -> RpcResult<Vec<GetRawYuvTransactionResponseHex>>;

    /// Get transaction list by page number.
    #[method(name = "listyuvtransactions")]
    async fn list_yuv_transactions(&self, page: u64) -> RpcResult<Vec<YuvTransactionResponse>>;

    /// Send YUV transaction to Bitcoin network.
    #[method(name = "sendrawyuvtransaction")]
    #[deprecated(since = "0.6.0", note = "use `sendyuvtransaction` instead")]
    async fn send_raw_yuv_tx(
        &self,
        yuv_tx: YuvTransaction,
        max_burn_amount: Option<u64>,
    ) -> RpcResult<bool>;

    /// Send YUV transaction HEX to Bitcoin network.
    #[method(name = "sendyuvtransaction")]
    async fn send_yuv_tx(&self, yuv_tx: String, max_burn_amount: Option<u64>) -> RpcResult<bool>;

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
