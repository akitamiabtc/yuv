//! This module provides a main indexer: [`BitcoinBlockIndexer`].

use bitcoin::BlockHash;
use bitcoin_client::{json::GetBlockTxResult, BitcoinRpcApi, BitcoinRpcClient};
use event_bus::{typeid, EventBus};
use eyre::{bail, Context};
use futures::TryFutureExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use yuv_storage::BlockIndexerStorage;
use yuv_types::{ControllerMessage, IndexerMessage};

use crate::{
    blockloader::{BlockLoaderConfig, IndexBlocksEvent},
    params::RunParams,
    BlockLoader, IndexingParams, Subindexer,
};

/// The default number of indexed blocks after which a message about indexing progress is logged.
const LOG_BLOCK_CHUNK_SIZE: u64 = 1000;
/// Channel size between `Indexer` and `Blockloader`.  
const LOADED_BLOCKS_CHANNEL_SIZE: usize = 1;
/// The number of restart attempts for the `Indexer` in case of an error.
const MAX_NUMBER_OF_RESTART_ATTEMPTS: usize = 6;
/// The time to sleep between restart attempts of the `Indexer`.
const RESTART_ATTEMPT_INTERVAL: Duration = Duration::from_secs(10);

/// Using polling indexes blocks from Bitcoin and broadcasts it to inner indexers.
pub struct BitcoinBlockIndexer<BS, BC>
where
    BS: BlockIndexerStorage,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    /// Bitcoin RPC Client.
    bitcoin_client: Arc<BC>,
    /// Storage for block indexer.
    storage: BS,
    /// Subindexers for block indexer.
    subindexers: Vec<Box<dyn Subindexer>>,
    /// Contains the height of the best confirmed block.
    confirmed_block_height: usize,
    /// Contains the hash of the best confirmed block.
    confirmed_block_hash: Option<BlockHash>,
    /// Event bus for receiving messages about the blockchain reorganization.
    event_bus: EventBus,
}

impl<BS, BC> BitcoinBlockIndexer<BS, BC>
where
    BS: BlockIndexerStorage + Send + Sync + 'static,
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn new(bitcoin_client: Arc<BC>, storage: BS, event_bus: &EventBus) -> Self {
        let event_bus = event_bus
            .extract(&typeid![ControllerMessage], &typeid![IndexerMessage])
            .expect("event channels must be presented");

        Self {
            bitcoin_client,
            storage,
            subindexers: Vec::new(),
            confirmed_block_height: 0,
            confirmed_block_hash: None,
            event_bus,
        }
    }

    /// Add a new [`Subindexer`] to the indexer.
    pub fn add_subindexer<I>(&mut self, indexer: I)
    where
        I: Subindexer + Send + Sync + 'static,
    {
        self.subindexers.push(Box::new(indexer));
    }

    /// Start indexing missed blocks from Bitcoin.
    ///
    /// At start of the node, call this functions to index missed blocks and be up to date.
    #[instrument(skip_all)]
    pub async fn init(
        &mut self,
        params: IndexingParams,
        block_loader_config: BlockLoaderConfig,
        bitcoin_client: Arc<BitcoinRpcClient>,
        confirmations_number: usize,
        cancellation: CancellationToken,
    ) -> eyre::Result<()> {
        let starting_block_height = self
            .get_starting_block_height(&params)
            .await?
            .saturating_sub(confirmations_number - 1);

        tracing::info!(
            from_height = starting_block_height.saturating_sub(1),
            "Start initial blocks indexing"
        );

        let block_loader = BlockLoader::new(
            bitcoin_client,
            block_loader_config.workers_number,
            block_loader_config.chunk_size,
        );

        let (sender_to_indexer, rx_indexer) = mpsc::channel(LOADED_BLOCKS_CHANNEL_SIZE);

        let handle = tokio::spawn(block_loader.run(
            starting_block_height,
            sender_to_indexer,
            block_loader_config.worker_time_sleep as u64,
            cancellation.child_token(),
        ))
        .map_err(|err| eyre::eyre!("failed to run block loader: {}", err));

        let (blockloader_result, indexer_result) = tokio::join!(
            handle,
            self.handle_initial_blocks(rx_indexer, starting_block_height)
        );

        // 1 condition - Blockloader's join handle and just blockloader error weren't received but indexer's error was
        // 2 condition - Either blockloader's join handle error weren't received but blockloader
        // error and indexer errors were or blockloader's join handle and indexer errors were received
        // 3 conditon - Either received only blockloader join handle error or only blockloader error
        match (blockloader_result, indexer_result) {
            (Ok(Ok(_)), Err(indexer_error)) => return Err(indexer_error),
            (Ok(Err(blockloader_error)), Err(indexer_error))
            | (Err(blockloader_error), Err(indexer_error)) => {
                bail!(
                    "BlockLoader error: {}, Indexer error: {}",
                    blockloader_error,
                    indexer_error
                )
            }
            (Err(blockloader_error), Ok(_)) | (Ok(Err(blockloader_error)), Ok(_)) => {
                return Err(blockloader_error)
            }

            _ => {}
        }

        let last_indexed_hash = match self.storage.get_last_indexed_hash().await? {
            Some(block_hash) => block_hash,
            None => self.bitcoin_client.get_block_hash(0).await?,
        };

        let last_block_info = self
            .bitcoin_client
            .get_block_header_info(&last_indexed_hash)
            .await?;

        self.confirmed_block_hash = Some(last_indexed_hash);
        self.confirmed_block_height = last_block_info.height;

        tracing::info!(
            height = ?last_block_info.height,
            block_hash = ?last_indexed_hash,
            "Finished initial blocks indexing",
        );

        Ok(())
    }

    /// Returns YUV genesis block height for the given network
    /// if [`IndexingParams::starting_block_hash`] is not provided and there is no `last_indexed_hash` in the storage.
    /// Returns `last_indexed_height` if `starting_block_hash` is not provided
    /// and vice versa
    async fn get_starting_block_height(&self, params: &IndexingParams) -> eyre::Result<usize> {
        let mut starting_block_height = 0;

        if let Some(last_indexed_hash) = self.storage.get_last_indexed_hash().await? {
            let last_indexed_height = self.get_block_height(&last_indexed_hash).await?;
            starting_block_height = last_indexed_height + 1;
        }

        // Starting block can be overridden by the block hash specified in the node config.
        if let Some(staring_block_hash) = params.starting_block_hash {
            starting_block_height = self.get_block_height(&staring_block_hash).await?;
        }

        Ok(starting_block_height)
    }

    /// Run indexer in loop, polling new blocks from Bitcoin RPC.
    pub async fn run(mut self, params: RunParams, cancellation: CancellationToken) {
        tracing::info!("Starting bitcoin indexer, parameters: {:?}", params);

        let mut timer = time::interval(params.polling_period);
        let mut restart_number = 0;
        let events = self.event_bus.subscribe::<IndexerMessage>();

        loop {
            tokio::select! {
                event_received = events.recv() => {
                    let Ok(event) = event_received else {
                        tracing::trace!("All incoming event senders are dropped");
                        return;
                    };

                    if let Err(err) = self.handle_event(event).await {
                        tracing::error!("Failed to handle an event: {}", err);
                    }
                },
                _ = timer.tick() => {},
                _ = cancellation.cancelled() => {
                    tracing::trace!("Cancellation received, stopping indexer");
                    return;
                }
            }

            if let Err(err) = self.handle_new_blocks().await {
                if restart_number >= MAX_NUMBER_OF_RESTART_ATTEMPTS {
                    tracing::error!("Indexer restart attempts number exceeded");
                    break;
                }

                restart_number += 1;

                tracing::error!(
                    "Failed to run indexer. Restart attempt {}/{} after {}s error={:#}",
                    restart_number,
                    MAX_NUMBER_OF_RESTART_ATTEMPTS,
                    RESTART_ATTEMPT_INTERVAL.as_secs(),
                    err
                );

                timer.reset_after(RESTART_ATTEMPT_INTERVAL);

                continue;
            }

            if restart_number > 0 {
                tracing::info!("Indexer returned to normal operation");
                restart_number = 0;
            }
        }

        cancellation.cancel()
    }

    async fn handle_event(&mut self, event: IndexerMessage) -> eyre::Result<()> {
        use IndexerMessage as Message;
        tracing::trace!("New event: {:?}", event);

        match event {
            Message::Reorganization(height) => self.handle_reorganization(height).await?,
        }

        Ok(())
    }

    async fn handle_reorganization(&mut self, new_height: usize) -> eyre::Result<()> {
        tracing::info!(
            "Changing indexing height from {} to {}",
            self.confirmed_block_height,
            new_height
        );

        self.confirmed_block_height = new_height;
        let block = self.get_block_by_height(new_height as u64).await?;
        self.confirmed_block_hash = Some(block.block_data.hash);

        Ok(())
    }

    /// Index blocks from [`BlockLoader`]. It appears in `Indexer` init function. Handles blocks
    /// loading.
    ///
    /// # Errors
    ///
    /// Return an error, when cancellation event was received or if indexing of blocks failed
    async fn handle_initial_blocks(
        &mut self,
        mut rx_indexer: mpsc::Receiver<IndexBlocksEvent>,
        mut indexer_last_block_height: usize,
    ) -> eyre::Result<()> {
        while let Some(event) = rx_indexer.recv().await {
            match event {
                IndexBlocksEvent::FinishLoading => {
                    tracing::debug!("Finished loading the blocks");
                    break;
                }
                IndexBlocksEvent::LoadedBlocks(blocks) => {
                    self.init_blocks_handle(blocks, &mut indexer_last_block_height)
                        .await?;
                }
                IndexBlocksEvent::Cancelled => {
                    bail!("Cancelled node running, failed to index new blocks")
                }
            }
        }

        Ok(())
    }

    /// Initial blocks indexing. Receives blocks chunk from [`BlockLoader`] and indexes them.
    /// Returns an error, when blocks are not sequential.
    async fn init_blocks_handle(
        &mut self,
        blocks: Vec<GetBlockTxResult>,
        indexer_last_block_height: &mut usize,
    ) -> eyre::Result<()> {
        for block in blocks {
            if block.block_data.height.ne(indexer_last_block_height) {
                bail!(
                    "Blocks must be sequential, indexer_last_block_height: {} != block height: {}",
                    indexer_last_block_height,
                    block.block_data.height
                );
            }

            self.index_block(&block).await?;

            *indexer_last_block_height += 1;

            let height = block.block_data.height;
            tracing::trace!("Indexed block at height {}", height);
            if height != 0 && height as u64 % LOG_BLOCK_CHUNK_SIZE == 0 {
                tracing::info!("Indexed blocks at height: {}", height);
            }
        }

        Ok(())
    }

    /// Takes block, indexes it and puts its hash to storage as a `last_indexed_hash`.
    async fn index_block(&mut self, block: &GetBlockTxResult) -> eyre::Result<()> {
        for indexer in self.subindexers.iter_mut() {
            indexer
                .index(block)
                .await
                .wrap_err("failed to handle new block")?;
        }

        self.storage
            .put_last_indexed_hash(block.block_data.hash)
            .await?;

        Ok(())
    }

    /// Handle new block from Bitcoin RPC.
    ///
    /// # Flow
    ///
    /// 1. Check if [there is a new confirmed block].
    ///     - If there is no new confirmed block, then return.
    ///     - If there is a new confirmed block, then go to step 2.
    /// 2. Get the next block by height [confirmed block height] + 1.
    /// 3. Check if the hash of the latest confirmed block is equal to the previous hash of the new
    ///    block.
    /// 4. Provide the block to every subindexer and update the storage.
    /// 5. Go to the step 1.
    ///
    /// [confirmed block height]: BitcoinBlockIndexer::check_new_confirmed_block
    async fn handle_new_blocks(&mut self) -> eyre::Result<()> {
        let best_block_height = self.bitcoin_client.get_block_count().await?;
        if best_block_height == self.confirmed_block_height as u64 {
            return Ok(());
        }

        let block = self
            .get_block_by_height(self.confirmed_block_height as u64 + 1)
            .await
            .wrap_err("failed to get block by hash")?;

        if let Some(last_indexed_block_hash) = self.confirmed_block_hash {
            if last_indexed_block_hash == block.block_data.hash {
                return Ok(());
            }
        }

        let new_block_hash = block.block_data.hash;
        let new_block_height = block.block_data.height;

        tracing::trace!(
            height = ?new_block_height,
            hash = ?new_block_hash,
            "New confirmed block",
        );

        self.index_block(&block).await?;

        self.confirmed_block_height = new_block_height;
        self.confirmed_block_hash = Some(new_block_hash);

        Ok(())
    }

    /// Returns the best block height by block hash.
    async fn get_block_height(&self, hash: &BlockHash) -> eyre::Result<usize> {
        let block = self.bitcoin_client.get_block_info(hash).await?;
        Ok(block.block_data.height)
    }

    /// Returns the block with transactions by height.
    async fn get_block_by_height(&self, height: u64) -> eyre::Result<GetBlockTxResult> {
        let block_hash = self.bitcoin_client.get_block_hash(height).await?;
        self.get_block(block_hash).await
    }

    /// Returns block with transactions by block hash.
    async fn get_block(&self, hash: BlockHash) -> eyre::Result<GetBlockTxResult> {
        self.bitcoin_client
            .get_block_txs(&hash)
            .await
            .wrap_err("failed to get block info by hash")
    }
}
