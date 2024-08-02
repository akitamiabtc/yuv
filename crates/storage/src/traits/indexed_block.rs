use crate::{KeyValueResult, KeyValueStorage};
use async_trait::async_trait;
use bitcoin::BlockHash;

const INDEXED_BLOCK_KEY_SIZE: usize = 13;
const INDEXED_BLOCK_KEY: &[u8; INDEXED_BLOCK_KEY_SIZE] = b"indexed_block";

#[async_trait]
pub trait BlockIndexerStorage: KeyValueStorage<[u8; INDEXED_BLOCK_KEY_SIZE], BlockHash> {
    async fn get_last_indexed_hash(&self) -> KeyValueResult<Option<BlockHash>> {
        Ok(self.get(*INDEXED_BLOCK_KEY).await?)
    }

    async fn put_last_indexed_hash(&self, block_hash: BlockHash) -> KeyValueResult<()> {
        self.put(*INDEXED_BLOCK_KEY, block_hash).await
    }
}
