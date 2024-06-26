use std::time::Duration;

use bitcoin::BlockHash;

/// Parameters to specify for initial indexing of blocks,
/// that node have skipped.
#[derive(Default)]
pub struct IndexingParams {
    /// The hash of block from which indexing should start if
    /// there is no last indexed block hash in storage.
    pub starting_block_hash: Option<BlockHash>,
}

/// Parameters that are passed to the `run` method of the indexer.
#[derive(Debug)]
pub struct RunParams {
    /// Period of time to wait between polling new blocks from Bitcoin.
    pub polling_period: Duration,
}

impl Default for RunParams {
    fn default() -> Self {
        Self {
            polling_period: Duration::from_secs(10),
        }
    }
}
