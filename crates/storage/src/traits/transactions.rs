use std::mem::size_of;

use async_trait::async_trait;
use bitcoin::{hashes::Hash, Txid};
use serde_bytes::ByteArray;
use yuv_types::YuvTransaction;

use crate::{KeyValueResult, KeyValueStorage};

const KEY_PREFIX: &str = "txs-";
const KEY_PREFIX_SIZE: usize = KEY_PREFIX.len();

/// Transaction storage key size is 4(`TXS_PREFIX:[u8; 4]`) + 32(`Txid`) = 36 bytes long
const TXS_STORAGE_KEY_SIZE: usize = KEY_PREFIX_SIZE + size_of::<Txid>();

fn tx_storage_key(txid: &Txid) -> ByteArray<TXS_STORAGE_KEY_SIZE> {
    let mut bytes = [0u8; TXS_STORAGE_KEY_SIZE];

    bytes[..KEY_PREFIX_SIZE].copy_from_slice(KEY_PREFIX.as_bytes());
    bytes[KEY_PREFIX_SIZE..].copy_from_slice(txid.as_raw_hash().as_byte_array());

    ByteArray::new(bytes)
}

#[async_trait]
pub trait TransactionsStorage:
    KeyValueStorage<ByteArray<TXS_STORAGE_KEY_SIZE>, YuvTransaction>
{
    async fn get_yuv_tx(&self, txid: &Txid) -> KeyValueResult<Option<YuvTransaction>> {
        self.get(tx_storage_key(txid)).await
    }

    async fn put_yuv_tx(&self, tx: YuvTransaction) -> KeyValueResult<()> {
        self.put(tx_storage_key(&tx.bitcoin_tx.txid()), tx).await
    }

    async fn delete_yuv_tx(&self, txid: &Txid) -> KeyValueResult<()> {
        self.delete(tx_storage_key(txid)).await
    }
}
