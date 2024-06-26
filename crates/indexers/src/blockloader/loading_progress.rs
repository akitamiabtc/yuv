/// `LoadingProgress` keeps track of current chunk loading progress
#[derive(Default, Debug)]
pub(crate) struct LoadingProgress {
    received_blocks: usize,
}

impl LoadingProgress {
    pub(crate) fn next_chunk(&mut self) {
        self.received_blocks = 0;
    }

    /// Increments received blocks
    pub(crate) fn update_received_blocks(&mut self) {
        self.received_blocks += 1;
    }

    /// Returns `true` if all blocks from chunk were loaded. It appears in `BlockLoader` when cancel
    /// event wasn't received. Uses in case when `BlockLoader` should load another chunk.
    pub(crate) fn finish_chunk(&self, chunk_size: &usize) -> bool {
        self.received_blocks.eq(chunk_size)
    }
}
