//! This module provides interface for sub-indexers, and some implementations.

pub use async_trait::async_trait;

pub use announcement::AnnouncementsIndexer;
use bitcoin_client::json::GetBlockTxResult;
pub use confirmation::ConfirmationIndexer;

mod announcement;
mod confirmation;

/// Represents a sub-indexer, which is responsible for indexing a specific items
/// from a block.
#[async_trait]
pub trait Subindexer: Send + Sync + 'static {
    async fn index(&mut self, block: &GetBlockTxResult) -> eyre::Result<()>;
}
