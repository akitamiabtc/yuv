use std::mem::size_of;

use async_trait::async_trait;
use bitcoin::{hashes::Hash, OutPoint, Txid};
use serde_bytes::ByteArray;
use yuv_pixels::Chroma;

use crate::{KeyValueResult, KeyValueStorage};

const TXID_SIZE: usize = size_of::<Txid>();
const KEY_PREFIX: &str = "frz-";
const KEY_PREFIX_SIZE: usize = KEY_PREFIX.len();

/// Frozen transactions storage key size is:
///
/// 4 bytes (`FROZEN_PREFIX`) + 32 bytes (`txid`) + 4 bytes (`vout`) = 40 bytes long
const FROZEN_TX_STORAGE_KEY_SIZE: usize = KEY_PREFIX_SIZE + TXID_SIZE + size_of::<u32>();

fn frozen_tx_storage_key(outpoint: &OutPoint) -> ByteArray<FROZEN_TX_STORAGE_KEY_SIZE> {
    let mut bytes = [0u8; FROZEN_TX_STORAGE_KEY_SIZE];

    bytes[..KEY_PREFIX_SIZE].copy_from_slice(KEY_PREFIX.as_bytes());
    bytes[KEY_PREFIX_SIZE..KEY_PREFIX_SIZE + TXID_SIZE]
        .copy_from_slice(outpoint.txid.as_raw_hash().as_byte_array());
    bytes[KEY_PREFIX_SIZE + TXID_SIZE..].copy_from_slice(&outpoint.vout.to_be_bytes());

    ByteArray::new(bytes)
}

#[async_trait]
pub trait FrozenTxsStorage:
    KeyValueStorage<ByteArray<FROZEN_TX_STORAGE_KEY_SIZE>, TxFreezeEntry>
{
    async fn get_frozen_tx(&self, outpoint: &OutPoint) -> KeyValueResult<Option<TxFreezeEntry>> {
        self.get(frozen_tx_storage_key(outpoint)).await
    }

    async fn put_frozen_tx(
        &self,
        outpoint: &OutPoint,
        freeze_tx_id: Txid,
        chroma: Chroma,
    ) -> KeyValueResult<()> {
        let freeze_entry = TxFreezeEntry::new(freeze_tx_id, chroma);
        self.put(frozen_tx_storage_key(outpoint), freeze_entry)
            .await
    }
}

/// Storage entry that stores the transaction identifiers that tried to freeze the output.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TxFreezeEntry {
    /// Identifier of transaction that tried to freeze the output.
    pub txid: Txid,

    /// Chroma of the output to freeze.
    pub chroma: Chroma,
}

impl TxFreezeEntry {
    pub fn new(txid: Txid, chroma: Chroma) -> Self {
        Self { txid, chroma }
    }
}
