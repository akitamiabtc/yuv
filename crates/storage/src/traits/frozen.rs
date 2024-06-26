use std::mem::size_of;

use async_trait::async_trait;
use bitcoin::{hashes::Hash, OutPoint, Txid};
use serde_bytes::ByteArray;

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
    KeyValueStorage<ByteArray<FROZEN_TX_STORAGE_KEY_SIZE>, TxFreezesEntry>
{
    async fn get_frozen_tx(&self, outpoint: &OutPoint) -> KeyValueResult<Option<TxFreezesEntry>> {
        self.get(frozen_tx_storage_key(outpoint)).await
    }

    async fn put_frozen_tx(
        &self,
        outpoint: &OutPoint,
        freeze_txs: Vec<Txid>,
    ) -> KeyValueResult<()> {
        self.put(
            frozen_tx_storage_key(outpoint),
            TxFreezesEntry::from(freeze_txs),
        )
        .await
    }

    async fn delete_frozen_tx(&self, outpoint: &OutPoint) -> KeyValueResult<()> {
        self.delete(frozen_tx_storage_key(outpoint)).await
    }
}

/// Storage entry that stores the transaction identifiers that tried to freeze the output.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct TxFreezesEntry {
    /// Identifiers of transaction that tried to freeze the output.
    pub tx_ids: Vec<Txid>,
}

impl From<Vec<Txid>> for TxFreezesEntry {
    fn from(value: Vec<Txid>) -> Self {
        Self { tx_ids: value }
    }
}

impl TxFreezesEntry {
    /// Check if UTXO is frozen or not based on the number of txs that tried to freeze it.
    pub fn is_frozen(&self) -> bool {
        self.tx_ids.len() % 2 == 1
    }
}
