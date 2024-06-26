use std::mem::size_of;

use async_trait::async_trait;
use bitcoin::{hashes::Hash, Txid};
use serde_bytes::ByteArray;
use yuv_types::YuvTransaction;

use crate::{KeyValueResult, KeyValueStorage};

const KEY_PREFIX: &str = "inv-";
const KEY_PREFIX_SIZE: usize = KEY_PREFIX.len();

/// Invalid transactions key size is:
///
/// 4 bytes (`INVALID_TXS_PREFIX`) + 32 bytes (`txid`) = 36 bytes long
const INVALID_TXS_KEY_SIZE: usize = KEY_PREFIX_SIZE + size_of::<Txid>();

fn invalid_txs_storage_key(txid: Txid) -> ByteArray<INVALID_TXS_KEY_SIZE> {
    let mut bytes = [0u8; INVALID_TXS_KEY_SIZE];

    bytes[..KEY_PREFIX_SIZE].copy_from_slice(KEY_PREFIX.as_bytes());
    bytes[KEY_PREFIX_SIZE..].copy_from_slice(txid.as_raw_hash().as_byte_array());

    ByteArray::new(bytes)
}

#[async_trait]
pub trait InvalidTxsStorage:
    KeyValueStorage<ByteArray<INVALID_TXS_KEY_SIZE>, YuvTransaction>
{
    async fn get_invalid_tx(&self, txid: Txid) -> KeyValueResult<Option<YuvTransaction>> {
        self.get(invalid_txs_storage_key(txid)).await
    }

    async fn put_invalid_tx(&self, tx: YuvTransaction) -> KeyValueResult<()> {
        self.put(invalid_txs_storage_key(tx.bitcoin_tx.txid()), tx)
            .await
    }

    async fn put_invalid_txs(&self, txs: Vec<YuvTransaction>) -> KeyValueResult<()> {
        for tx in txs {
            self.put_invalid_tx(tx).await?;
        }

        Ok(())
    }

    async fn delete_invalid_tx(&self, txid: Txid) -> KeyValueResult<()> {
        self.delete(invalid_txs_storage_key(txid)).await
    }
}
