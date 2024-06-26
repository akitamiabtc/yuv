use std::{sync::Arc, time::Duration};

use bitcoin_client::{json::GetBlockTxResult, BitcoinRpcApi};
use tokio::{select, sync::mpsc::Sender};
use tokio_util::sync::CancellationToken;

use super::events::{FetchLoadedBlockEvent, LoadBlockEvent};

/// Rate limit error. Occurs when worker sends to many requests to Bitcoin node.
const RATE_LIMIT_ERROR: &str = "JSON-RPC error: transport error: Couldn't connect to host: Can't assign requested address (os error 49)";

/// `Worker` loads blocks from the Bitcoin network. It takes block height loads it and sends it to
/// `BlockLoader`.
#[derive(Clone)]
pub(crate) struct Worker {
    /// Bitcoin RPC client.
    bitcoin_client: Arc<bitcoin_client::BitcoinRpcClient>,
    /// Loaded block sender to `BlockLoadder`
    loaded_block_sender: Sender<FetchLoadedBlockEvent>,
    /// Listener for blocks to load. Listens for the blocks from `BlockLoader`
    load_block_receiver: flume::Receiver<LoadBlockEvent>,
    /// Flag that shows if rate limit was reached
    rate_limit_reached: bool,
}

impl Worker {
    pub fn new(
        bitcoin_client: Arc<bitcoin_client::BitcoinRpcClient>,
        loaded_block_sender: Sender<FetchLoadedBlockEvent>,
        load_block_receiver: flume::Receiver<LoadBlockEvent>,
    ) -> Self {
        Self {
            rate_limit_reached: false,
            bitcoin_client,
            loaded_block_sender,
            load_block_receiver,
        }
    }

    /// Returns a block data from the Bitcoin network
    /// # Parameters
    ///
    /// * `block_height` - height of the block
    ///
    /// Returns an error if the block with passed `block_height` does not exist.
    async fn get_block(&self, block_height: usize) -> eyre::Result<GetBlockTxResult> {
        let block_hash = self
            .bitcoin_client
            .get_block_hash(block_height as u64)
            .await?;

        let txs = self.bitcoin_client.get_block_txs(&block_hash).await?;

        Ok(txs)
    }

    /// Loads block from height was sent by `BlockLoader` than sends block data to `BlockLoader`.
    /// Returns `true` if worker should stop.
    async fn handle_load_event(&mut self, event: LoadBlockEvent) -> eyre::Result<()> {
        match event {
            LoadBlockEvent::LoadBlock(block_height) => {
                let event = match self.get_block(block_height).await {
                    Ok(block) => FetchLoadedBlockEvent::Loaded(Box::new(block)),
                    Err(err) => {
                        if err.to_string().eq(RATE_LIMIT_ERROR) {
                            self.rate_limit_reached = true;
                        }

                        tracing::warn!(?block_height, %err, "Failed to get block");
                        FetchLoadedBlockEvent::FailedBlock(block_height)
                    }
                };

                self.loaded_block_sender.send(event).await?;
            }
        }

        Ok(())
    }
}

impl Worker {
    pub async fn run(
        mut self,
        time_to_sleep: u64,
        cancellation: CancellationToken,
    ) -> eyre::Result<()> {
        loop {
            if self.rate_limit_reached {
                tracing::warn!(
                    "Worker received rate limit error, retrying in {time_to_sleep} seconds"
                );
                tokio::time::sleep(Duration::from_secs(time_to_sleep)).await;
                self.rate_limit_reached = false;
            }

            select! {
                event = self.load_block_receiver.recv_async() => {
                    let Ok(event) = event else {
                        break;
                    };

                    self.handle_load_event(event).await?;
                }
                _ = cancellation.cancelled() => {
                    break;
                }
            }
        }

        tracing::debug!("Finished worker");

        Ok(())
    }
}
