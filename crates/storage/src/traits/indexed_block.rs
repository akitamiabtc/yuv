use crate::{KeyValueResult, KeyValueStorage};
use async_trait::async_trait;
use bitcoin::BlockHash;

const INDEXED_BLOCK_KEY_SIZE: usize = 13;
const INDEXED_BLOCK_KEY: &[u8; INDEXED_BLOCK_KEY_SIZE] = b"indexed_block";

/// TODO: Remove this storage. This bugfix requires all the nodes to reindex the chain from
/// the genesis block.
const IS_INDEXED_KEY_SIZE: usize = 17;
const IS_INDEXED_KEY: &[u8; IS_INDEXED_KEY_SIZE] = b"06-03-2024-bugfix";

#[async_trait]
pub trait BlockIndexerStorage: KeyValueStorage<[u8; INDEXED_BLOCK_KEY_SIZE], BlockHash> {
    async fn get_last_indexed_hash(&self) -> KeyValueResult<Option<BlockHash>> {
        Ok(self.get(*INDEXED_BLOCK_KEY).await?)
    }

    async fn put_last_indexed_hash(&self, block_hash: BlockHash) -> KeyValueResult<()> {
        self.put(*INDEXED_BLOCK_KEY, block_hash).await
    }
}

#[async_trait]
/// TODO: Remove this storage in the future. This bugfix requires all the nodes to reindex the chain from
/// the genesis block if there is no data in this storage.
pub trait IsIndexedStorage: KeyValueStorage<[u8; IS_INDEXED_KEY_SIZE], ()> {
    async fn get_is_indexed(&self) -> KeyValueResult<Option<()>> {
        Ok(self.get(*IS_INDEXED_KEY).await?)
    }

    async fn put_is_indexed(&self) -> KeyValueResult<()> {
        self.put(*IS_INDEXED_KEY, ()).await
    }
}
