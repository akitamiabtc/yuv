#![allow(deprecated)]
use std::{mem::size_of, net::SocketAddr};

use async_trait::async_trait;
use bitcoin::{hashes::Hash, Txid};
use serde_bytes::ByteArray;
use yuv_types::YuvTransaction;

use crate::{KeyValueResult, KeyValueStorage};

const MEMPOOL_KEY_SIZE: usize = 8;
const MEMPOOL_KEY: &[u8; MEMPOOL_KEY_SIZE] = b"mempool-";

const MEMPOOL_ENTRY_KEY_SIZE: usize = MEMPOOL_KEY_SIZE + size_of::<Txid>();

fn mempool_entry_key(txid: &Txid) -> ByteArray<MEMPOOL_ENTRY_KEY_SIZE> {
    let mut bytes = [0u8; MEMPOOL_ENTRY_KEY_SIZE];

    bytes[..MEMPOOL_KEY_SIZE].copy_from_slice(MEMPOOL_KEY);
    bytes[MEMPOOL_KEY_SIZE..].copy_from_slice(txid.as_raw_hash().as_byte_array());

    ByteArray::new(bytes)
}

#[async_trait]
pub trait MempoolStorage: KeyValueStorage<[u8; MEMPOOL_KEY_SIZE], Vec<Txid>> {
    async fn get_mempool(&self) -> KeyValueResult<Option<Vec<Txid>>> {
        self.get(*MEMPOOL_KEY).await
    }

    async fn put_mempool(&self, mempool: Vec<Txid>) -> KeyValueResult<()> {
        self.put(*MEMPOOL_KEY, mempool).await
    }
}

#[async_trait]
pub trait MempoolEntryStorage:
    KeyValueStorage<ByteArray<MEMPOOL_ENTRY_KEY_SIZE>, MempoolTxEntry>
{
    async fn get_mempool_entry(&self, txid: &Txid) -> KeyValueResult<Option<MempoolTxEntry>> {
        self.get(mempool_entry_key(txid)).await
    }

    async fn delete_mempool_entry(&self, txid: &Txid) -> KeyValueResult<()> {
        self.delete(mempool_entry_key(txid)).await
    }

    async fn put_mempool_entry(&self, entry: MempoolTxEntry) -> KeyValueResult<()> {
        self.put(mempool_entry_key(&entry.txid()), entry).await
    }
}

/// A mempool entry that is used to store data about transactions that are being handled.
///
/// Consists of:
/// - yuv_tx: full YUV transaction data [`YuvTransaction`].
/// - status: current status of the transaction [`MempoolStatus`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MempoolTxEntry {
    /// YUV transaction itself.
    pub yuv_tx: YuvTransaction,
    /// Current YUV transaction status.
    pub status: MempoolStatus,
    /// Peer id of the sender:
    /// * Some if transactions received from p2p network
    /// * None if transactions received via json rpc
    pub sender: Option<SocketAddr>,
}

impl MempoolTxEntry {
    pub fn new(yuv_tx: YuvTransaction, status: MempoolStatus, sender: Option<SocketAddr>) -> Self {
        Self {
            yuv_tx,
            status,
            sender,
        }
    }

    /// Returns the [`Txid`] of the entry's YUV transaction.
    pub fn txid(&self) -> Txid {
        self.yuv_tx.bitcoin_tx.txid()
    }
}

impl From<YuvTransaction> for MempoolTxEntry {
    fn from(yuv_tx: YuvTransaction) -> Self {
        MempoolTxEntry::new(yuv_tx, MempoolStatus::Initialized, None)
    }
}

/// Represents the status of a YUV transaction that is being handled, i.e. a transaction that is
/// in the mempool.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MempoolStatus {
    Initialized,
    WaitingMined,
    Mined,
    Attaching,
    #[deprecated]
    Pending,
}
