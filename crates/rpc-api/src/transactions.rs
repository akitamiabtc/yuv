use bitcoin::{BlockHash, Transaction, Txid};
use serde::Deserialize;
use yuv_storage::MempoolStatus;
use yuv_types::{YuvTransaction, YuvTxType};

#[cfg(any(feature = "client", feature = "server"))]
mod rpc;
#[cfg(any(feature = "client", feature = "server"))]
pub use self::rpc::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
/// Describes YUV transaction status.
pub enum YuvTransactionStatus {
    /// Transaction is not found.
    ///
    /// Provided proof was rejected, or no proofs were provided yet.
    None,
    /// Transaction is found, it's raw data is provided, but it's in the queue to be checked.
    Initialized,
    /// Transaction is found, it's raw data is provided, it's partially checked, but hasn't
    /// appeared in the blockchain yet.
    WaitingMined,
    /// Transaction is found, it's raw data is provided, it's partially checked, but is waiting for
    /// enough confirmations.
    Mined,
    /// Transaction is found, it's raw data is provided, it's fully checked, but is waiting to get
    /// attached.
    Attaching,
    /// Transaction is found, it's raw data is provided, it's fully checked, and the node has
    /// all parent transactions to attach it.
    Attached,
    /// TODO: This status is used for `get_raw_yuv_transaction` only and will soon be removed.
    Pending,
}

impl From<MempoolStatus> for YuvTransactionStatus {
    fn from(value: MempoolStatus) -> Self {
        match value {
            MempoolStatus::Initialized => Self::Initialized,
            MempoolStatus::WaitingMined => Self::WaitingMined,
            MempoolStatus::Mined => Self::Mined,
            MempoolStatus::Attaching => Self::Attaching,
        }
    }
}

/// Json encoded response for [`getrawyuvtransaction`](YuvTransactionsRpcServer::get_raw_yuv_transaction) RPC
/// method.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GetRawYuvTransactionResponseJson {
    pub status: YuvTransactionStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<YuvTransactionResponse>,
}

impl GetRawYuvTransactionResponseJson {
    pub fn new(status: YuvTransactionStatus, data: Option<YuvTransactionResponse>) -> Self {
        Self { status, data }
    }
}

/// Hex encoded response for [`getyuvtransaction`](YuvTransactionsRpcServer::get_yuv_transaction) RPC
/// method.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GetRawYuvTransactionResponseHex {
    pub status: YuvTransactionStatus,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[serde(serialize_with = "yuv_tx_to_hex", deserialize_with = "hex_to_yuv_tx")]
    pub data: Option<YuvTransactionResponse>,
}

impl GetRawYuvTransactionResponseHex {
    pub fn new(status: YuvTransactionStatus, data: Option<YuvTransactionResponse>) -> Self {
        Self { status, data }
    }
}

pub fn yuv_tx_to_hex<S>(
    yuv_tx: &Option<YuvTransactionResponse>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match yuv_tx {
        Some(tx) => serializer.serialize_str(&YuvTransaction::from(tx.clone()).hex()),
        None => serializer.serialize_none(),
    }
}

pub fn hex_to_yuv_tx<'de, D>(deserializer: D) -> Result<Option<YuvTransactionResponse>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt_hex = Option::<String>::deserialize(deserializer)?;
    match opt_hex {
        Some(hex) => {
            let tx = YuvTransaction::from_hex(hex).map_err(serde::de::Error::custom)?;
            Ok(Some(YuvTransactionResponse::from(tx)))
        }
        None => Ok(None),
    }
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

/// Request for [`provideyuvproof`] and [`providelistyuvproofs`] RPC methods that are defined for
/// providing YUV proofs without broadcasting the Bitcoin tx.
///
/// [`provideyuvproof`]: YuvTransactionsRpcServer::provide_yuv_proof
/// [`providelistyuvproofs`]: YuvTransactionsRpcServer::provide_list_yuv_proofs
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProvideYuvProofRequest {
    pub txid: Txid,
    #[serde(serialize_with = "tx_type_to_hex", deserialize_with = "hex_to_tx_type")]
    pub tx_type: YuvTxType,
    pub blockhash: Option<BlockHash>,
}

pub fn tx_type_to_hex<S>(tx_type: &YuvTxType, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&tx_type.hex())
}

pub fn hex_to_tx_type<'de, D>(deserializer: D) -> Result<YuvTxType, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex = String::deserialize(deserializer)?;
    YuvTxType::from_hex(hex).map_err(serde::de::Error::custom)
}

impl ProvideYuvProofRequest {
    pub fn new(txid: Txid, tx_type: YuvTxType, blockhash: Option<BlockHash>) -> Self {
        Self {
            txid,
            tx_type,
            blockhash,
        }
    }
}
