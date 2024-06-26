use bitcoin_client::json::GetBlockTxResult;

/// Events emitted by `Worker` to the `BlockLoader`. The workers send loaded blocks or index of failed loaded
/// blocks. Failed blocks is blocks that `bitcoin_client` failed to load.
pub(crate) enum FetchLoadedBlockEvent {
    /// Loaded block
    Loaded(Box<GetBlockTxResult>),
    /// Block that was failed
    FailedBlock(usize),
}

/// Events emitted by `BlockLoader` to the `Worker`. `BlockLoader` sends blocks to be loaded and then indexed.
pub(crate) enum LoadBlockEvent {
    /// Block that needs to be loaded
    LoadBlock(usize),
}

/// Events emitted by `BlockLoader` to the `Indexer`. `BlockLoader` sends the loaded blocks chunk to the
/// `Indexer`.
pub enum IndexBlocksEvent {
    /// Loaded blocks chunk
    LoadedBlocks(Vec<GetBlockTxResult>),
    /// Marker that notifies that loading process is finished
    FinishLoading,
    /// Cancelled node running process
    Cancelled,
}
