use std::sync::Arc;

use bitcoin_client::{json::GetBlockTxResult, BitcoinRpcApi, BitcoinRpcClient};
use eyre::Ok;
use tokio::{select, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::instrument;

use super::{
    events::{FetchLoadedBlockEvent, IndexBlocksEvent, LoadBlockEvent},
    loading_progress::LoadingProgress,
    worker::Worker,
};

/// Manager for loading blocks from Bitcoin network
pub struct BlockLoader {
    /// Bitcoin RPC Client
    bitcoin_client: Arc<BitcoinRpcClient>,
    /// The workers number, that will load blocks
    workers_number: usize,
    /// The size of the chunk that will be send to the `Indexer`
    chunk_size: usize,
    /// Loaded blocks of the chunk
    loaded_blocks: Vec<GetBlockTxResult>,
    /// Task tracker for workers
    task_tracker: TaskTracker,
    /// Loading progress, contains loading process of the chunk
    loading_progress: LoadingProgress,
    /// Number of confirmations that is required to consider block as confirmed.
    confirmation_number: u8,
}

impl BlockLoader {
    pub fn new(
        bitcoin_client: Arc<BitcoinRpcClient>,
        workers_number: usize,
        chunk_size: usize,
        confirmation_number: u8,
    ) -> Self {
        Self {
            bitcoin_client,
            workers_number,
            chunk_size,
            loaded_blocks: Vec::with_capacity(chunk_size),
            task_tracker: TaskTracker::new(),
            loading_progress: LoadingProgress::default(),
            confirmation_number,
        }
    }
}

impl BlockLoader {
    fn run_workers(
        &self,
        load_block_receiver: flume::Receiver<LoadBlockEvent>,
        loaded_block_sender: mpsc::Sender<FetchLoadedBlockEvent>,
        time_to_sleep: u64,
        cancellation: CancellationToken,
    ) {
        for _ in 0..self.workers_number {
            let worker = Worker::new(
                self.bitcoin_client.clone(),
                loaded_block_sender.clone(),
                load_block_receiver.clone(),
            );

            self.task_tracker
                .spawn(worker.run(time_to_sleep, cancellation.clone()));
        }

        self.task_tracker.close();
    }

    /// Handles loaded of failed blocks. In case of loaded block savess it in `loaded_blocks`, in case of
    /// failed block sends it to `Worker` to load it again.
    #[instrument(skip_all)]
    async fn handle_fetch_event(
        &mut self,
        event: FetchLoadedBlockEvent,
        load_block_sender: &flume::Sender<LoadBlockEvent>,
    ) -> eyre::Result<()> {
        match event {
            FetchLoadedBlockEvent::Loaded(block) => {
                tracing::trace!("Received block with height {}", block.block_data.height);
                self.loaded_blocks.push(*block);
                self.loading_progress.update_received_blocks();
            }
            FetchLoadedBlockEvent::FailedBlock(block_height) => {
                tracing::debug!("Resend failed block with height: {}", block_height);
                load_block_sender
                    .send_async(LoadBlockEvent::LoadBlock(block_height))
                    .await?
            }
        }

        Ok(())
    }

    /// Sends the height of block, which `Workers` should load
    #[instrument(skip_all)]
    async fn send_load_blocks(
        &mut self,
        load_block_sender: &flume::Sender<LoadBlockEvent>,
        blocks_chunk: &[usize],
    ) -> eyre::Result<()> {
        for block_height in blocks_chunk {
            tracing::trace!("Send block to workers: {}", block_height);
            load_block_sender
                .send_async(LoadBlockEvent::LoadBlock(*block_height))
                .await?;
        }

        Ok(())
    }

    /// Handles loaded blocks. Stops execution when `received_block` is equal `chunk_size`
    async fn handle_loaded_blocks(
        &mut self,
        loaded_block_listener: &mut mpsc::Receiver<FetchLoadedBlockEvent>,
        load_block_sender: flume::Sender<LoadBlockEvent>,
        chunk_size: usize,
    ) -> eyre::Result<()> {
        while let Some(event) = loaded_block_listener.recv().await {
            self.handle_fetch_event(event, &load_block_sender).await?;

            if self.loading_progress.finish_chunk(&chunk_size) {
                self.loading_progress.next_chunk();
                break;
            }
        }

        Ok(())
    }

    /// Sends loaded blocks to `Indexer` before it has sorted the loaded blocks.
    async fn send_loaded_blocks(
        &mut self,
        sender_to_indexer: mpsc::Sender<IndexBlocksEvent>,
    ) -> eyre::Result<()> {
        self.loaded_blocks
            .sort_by(|a, b| a.block_data.height.cmp(&b.block_data.height));

        let blocks_not_sequantial_index =
            self.loaded_blocks
                .windows(2)
                .enumerate()
                .find_map(|(index, blocks)| {
                    if blocks[0].block_data.height + 1 != blocks[1].block_data.height {
                        Some(index)
                    } else {
                        None
                    }
                });

        if let Some(index) = blocks_not_sequantial_index {
            self.loaded_blocks.truncate(index + 1);
        }

        sender_to_indexer
            .send(IndexBlocksEvent::LoadedBlocks(
                self.loaded_blocks.drain(..).collect(),
            ))
            .await?;

        Ok(())
    }

    /// Handles remained blocks. Stops when all remained blocks were handled.
    #[instrument(skip_all)]
    async fn handle_remained_blocks(
        &mut self,
        loaded_block_listener: &mut mpsc::Receiver<FetchLoadedBlockEvent>,
        load_block_sender: &flume::Sender<LoadBlockEvent>,
    ) -> eyre::Result<()> {
        tracing::info!("Waiting for remained loaded blocks...");
        while let Some(event) = loaded_block_listener.recv().await {
            self.handle_fetch_event(event, load_block_sender).await?;
        }

        Ok(())
    }

    /// Handles new blocks from `Bitcoin` network. When `BlockLoader` finished loading new blocks
    /// it sends `FinishLoading` them to `Indexer` and stops workers.
    async fn handle_new_blocks(
        &mut self,
        load_block_sender: &flume::Sender<LoadBlockEvent>,
        sender_to_indexer: mpsc::Sender<IndexBlocksEvent>,
        loaded_block_listener: &mut mpsc::Receiver<FetchLoadedBlockEvent>,
        start_height: usize,
    ) -> eyre::Result<()> {
        let confirmed_height = self.get_confirmed_height().await?;

        let blocks_to_load = (start_height..=(confirmed_height as usize)).collect::<Vec<usize>>();

        for blocks_chunk in blocks_to_load.chunks(self.chunk_size) {
            self.send_load_blocks(load_block_sender, blocks_chunk)
                .await?;

            self.handle_loaded_blocks(
                loaded_block_listener,
                load_block_sender.clone(),
                blocks_chunk.len(),
            )
            .await?;

            self.send_loaded_blocks(sender_to_indexer.clone()).await?;
        }

        sender_to_indexer
            .send(IndexBlocksEvent::FinishLoading)
            .await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn run(
        mut self,
        load_from_height: usize,
        sender_to_indexer: mpsc::Sender<IndexBlocksEvent>,
        time_to_sleep: u64,
        cancellation: CancellationToken,
    ) -> eyre::Result<()> {
        let (load_block_sender, load_block_receiver) =
            flume::bounded::<LoadBlockEvent>(self.chunk_size);

        let (loaded_block_sender, mut loaded_block_listener) =
            mpsc::channel::<FetchLoadedBlockEvent>(self.chunk_size);

        self.run_workers(
            load_block_receiver,
            loaded_block_sender,
            time_to_sleep,
            cancellation.clone(),
        );

        select! {
            _ = self.handle_new_blocks(
                &load_block_sender,
                sender_to_indexer.clone(),
                &mut loaded_block_listener,
                load_from_height,
            ) => {}

            _ = cancellation.cancelled() => {
                tracing::info!("Block loader cancelled. Finishing receiving blocks");
                self.handle_remained_blocks(&mut loaded_block_listener, &load_block_sender).await?;
                self.send_loaded_blocks(sender_to_indexer.clone()).await?;

                sender_to_indexer
                    .send(IndexBlocksEvent::Cancelled)
                    .await?;
            }
        }

        tracing::debug!("Finished loading blocks");

        drop(load_block_sender);

        self.task_tracker.wait().await;

        tracing::debug!("Block loader finished loading proccess");

        Ok(())
    }

    async fn get_confirmed_height(&self) -> eyre::Result<u64> {
        let best_block_height = self.bitcoin_client.get_block_count().await?;

        let confirmed_height =
            best_block_height.saturating_sub(self.confirmation_number as u64 - 1);

        Ok(confirmed_height)
    }
}
