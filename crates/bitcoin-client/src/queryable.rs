use crate::json;
use crate::{BitcoinRpcApi, Result};
use async_trait::async_trait;

/// A type that can be queried from Bitcoin Core.
#[async_trait]
pub trait Queryable<C: BitcoinRpcApi>: Sized + Send + Sync {
    /// Type of the ID used to query the item.
    type Id;
    /// Query the item using `rpc` and convert to `Self`.
    async fn query(rpc: &C, id: &Self::Id) -> Result<Self>;
}

#[async_trait]
impl<C: BitcoinRpcApi + Sync> Queryable<C> for bitcoin::blockdata::block::Block {
    type Id = bitcoin::BlockHash;

    async fn query(rpc: &C, id: &Self::Id) -> Result<Self> {
        let rpc_name = "getblock";
        let hex: String = rpc
            .call(rpc_name, &[serde_json::to_value(id)?, 0.into()])
            .await?;
        let bytes: Vec<u8> = bitcoin::hashes::hex::FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }
}

#[async_trait]
impl<C: BitcoinRpcApi + Sync> Queryable<C> for bitcoin::blockdata::transaction::Transaction {
    type Id = bitcoin::Txid;

    async fn query(rpc: &C, id: &Self::Id) -> Result<Self> {
        let rpc_name = "getrawtransaction";
        let hex: String = rpc.call(rpc_name, &[serde_json::to_value(id)?]).await?;
        let bytes: Vec<u8> = bitcoin::hashes::hex::FromHex::from_hex(&hex)?;
        Ok(bitcoin::consensus::encode::deserialize(&bytes)?)
    }
}

#[async_trait]
impl<C: BitcoinRpcApi + Sync> Queryable<C> for Option<json::GetTxOutResult> {
    type Id = bitcoin::OutPoint;

    async fn query(rpc: &C, id: &Self::Id) -> Result<Self> {
        rpc.get_tx_out(&id.txid, id.vout, Some(true)).await
    }
}
