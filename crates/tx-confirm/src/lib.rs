use bitcoin::{BlockHash, Txid};
use bitcoin_client::json::GetBlockTxResult;
use bitcoin_client::{BitcoinRpcApi, JsonRpcError};
use event_bus::{typeid, EventBus};
use eyre::bail;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;
use yuv_types::{ControllerMessage, TxConfirmMessage};

/// `TxConfirmator` is responsible for waiting confirmations of transactions in Bitcoin.
pub struct TxConfirmator<BC>
where
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    event_bus: EventBus,
    bitcoin_client: Arc<BC>,
    /// Confirmations queue. Contains transactions that are waiting confirmation.
    queue: HashMap<Txid, SystemTime>,
    /// Max time that transaction can wait confirmation before it will be removed from the queue.
    max_confirmation_time: Duration,
    /// Interval between waiting txs clean up.
    clean_up_interval: Duration,
    /// Contains the number of confirmations required to consider a transaction as confirmed.
    confirmations_number: u8,
    /// Contains the latest indexed blocks and is used to handle reorgs.
    latest_blocks: VecDeque<BlockInfo>,
}

/// An abstraction over `GetBlockTxResult` that is used by the `TxConfirmator` to keep track
/// of the recent blocks.
#[derive(Debug, Clone)]
struct BlockInfo {
    /// Hash of the block.
    hash: BlockHash,
    /// Transactions inside the block.
    txs: Vec<Txid>,
}

impl From<GetBlockTxResult> for BlockInfo {
    fn from(block_result: GetBlockTxResult) -> Self {
        let txs = block_result.tx.iter().map(|tx| tx.txid()).collect();
        Self {
            hash: block_result.block_data.hash,
            txs,
        }
    }
}

impl<BC> TxConfirmator<BC>
where
    BC: BitcoinRpcApi + Send + Sync + 'static,
{
    pub fn new(
        event_bus: &EventBus,
        bitcoin_client: Arc<BC>,
        max_confirmation_time: Duration,
        clean_up_interval: Duration,
        confirmations_number: u8,
    ) -> Self {
        let event_bus = event_bus
            .extract(&typeid![ControllerMessage], &typeid![TxConfirmMessage])
            .expect("event channels must be presented");

        Self {
            event_bus,
            queue: Default::default(),
            max_confirmation_time,
            bitcoin_client,
            clean_up_interval,
            confirmations_number,
            latest_blocks: Default::default(),
        }
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) {
        let mut clean_up_timer = tokio::time::interval(self.clean_up_interval);
        let events = self.event_bus.subscribe::<TxConfirmMessage>();

        loop {
            tokio::select! {
                event_received = events.recv() => {
                    let Ok(event) = event_received else {
                        tracing::trace!("All incoming events senders are dropped");
                        return;
                    };

                    if let Err(err) = self.handle_event(event).await {
                        tracing::error!("failed to handle event: {:#}", err);
                        cancellation_token.cancel();
                    };
                },
                _ = clean_up_timer.tick() => {
                    if let Err(err) = self.clean_up_waiting_txs().await {
                        tracing::error!("failed to handle waiting transactions: {:#}", err);
                    };
                },
                _ = cancellation_token.cancelled() => {
                    tracing::trace!("cancellation received, stopping confirmator");
                    return;
                }
            }
        }
    }

    async fn handle_event(&mut self, event: TxConfirmMessage) -> eyre::Result<()> {
        match event {
            TxConfirmMessage::Txs(txids) => {
                for txid in txids {
                    self.handle_tx_to_confirm(txid).await?;
                }
            }
            TxConfirmMessage::Block(block) => self.handle_new_block(*block).await?,
        }

        Ok(())
    }

    async fn handle_new_block(&mut self, block: GetBlockTxResult) -> eyre::Result<()> {
        tracing::debug!(
            block_hash = block.block_data.hash.to_string(),
            "Handling new block"
        );

        if let (Some(new_block_prev_hash), Some(last_indexed_block)) = (
            block.block_data.previousblockhash,
            self.latest_blocks.back(),
        ) {
            // If there is a hash mismatch, handle the reorg.
            if last_indexed_block.hash != new_block_prev_hash {
                tracing::warn!(
                    "Latest indexed block is not a parent of the new block to index. Possibly \
                    a reorg happened. Last indexed block hash: {:?}, new block previous hash: \
                    {:?}, new block hash: {:?}",
                    last_indexed_block.hash,
                    new_block_prev_hash,
                    block.block_data.hash,
                );

                return self.handle_reorg(&block).await;
            }
        };

        let block_info = block.into();
        let mined_txs = self.extract_waiting_txs_from_block(&block_info);
        self.latest_blocks.push_back(block_info.clone());
        self.handle_mined_txs(mined_txs).await?;

        // If there is a block that reached enough confirmations, send its txs to the
        // tx checker for a full check.
        if self.latest_blocks.len() == self.confirmations_number as usize {
            let confirmed_block = self
                .latest_blocks
                .pop_front()
                .expect("at least one block should be present");

            let yuv_txs = self.extract_waiting_txs_from_block(&confirmed_block);
            if !yuv_txs.is_empty() {
                self.new_confirmed_txs(&yuv_txs).await;

                tracing::debug!(
                    block_hash = confirmed_block.hash.to_string(),
                    "New block confirmed",
                );
            }
        }

        Ok(())
    }

    /// Handle new transaction to confirm it. If transaction is already confirmed, then it will be
    /// sent to the `TxChecker`. Otherwise it will be added to the queue.
    async fn handle_tx_to_confirm(&mut self, txid: Txid) -> eyre::Result<()> {
        self.queue.entry(txid).or_insert(SystemTime::now());

        let got_tx_result = self
            .bitcoin_client
            .get_raw_transaction_info(&txid, None)
            .await;

        let tx = match got_tx_result {
            Err(bitcoin_client::Error::JsonRpc(JsonRpcError::Rpc(err))) if err.code == -5 => {
                tracing::error!("Couldn't find the tx {:?} in the blockchain", txid);
                return Ok(());
            }
            res => res?,
        };

        if let Some(confirmations) = tx.confirmations {
            self.handle_mined_txs(vec![txid]).await?;

            if confirmations >= self.confirmations_number as u32 {
                self.new_confirmed_txs(&[txid]).await;
                return Ok(());
            }
        }

        Ok(())
    }

    async fn handle_reorg(&mut self, new_block: &GetBlockTxResult) -> eyre::Result<()> {
        // List of transactions that are members of orphan blocks and should be handled again.
        let mut reorged_txs = Vec::new();
        let mut prev_block_hash = new_block.block_data.previousblockhash;
        let mut new_indexing_height = new_block.block_data.height;

        loop {
            let Some(last_block) = self.latest_blocks.pop_back() else {
                bail!("Failed to handle the reorg: fork length is too big");
            };

            let Some(current_block_hash) = prev_block_hash else {
                bail!("Failed to handle the reorg: previous block hash is missing");
            };

            new_indexing_height -= 1;

            // If the popped block hash is equal to the current block hash, all the orphan blocks
            // were handled.
            if last_block.hash == current_block_hash {
                self.latest_blocks.push_back(last_block);
                break;
            }

            let prev_block = self
                .bitcoin_client
                .get_block_info(&current_block_hash)
                .await?;
            prev_block_hash = prev_block.block_data.previousblockhash;

            let current_block_reorged_txs = self.extract_waiting_txs_from_block(&last_block);
            reorged_txs.extend(current_block_reorged_txs);
        }

        for reorged_tx in &reorged_txs {
            self.queue.remove(reorged_tx);
        }

        self.event_bus
            .send(ControllerMessage::Reorganization {
                txs: reorged_txs,
                new_indexing_height,
            })
            .await;

        Ok(())
    }

    async fn handle_mined_txs(&self, txids: Vec<Txid>) -> eyre::Result<()> {
        if !txids.is_empty() {
            self.event_bus
                .send(ControllerMessage::MinedTxs(txids))
                .await;
        }

        Ok(())
    }

    fn extract_waiting_txs_from_block(&self, block: &BlockInfo) -> Vec<Txid> {
        block
            .txs
            .clone()
            .into_iter()
            .filter(|txid| self.queue.contains_key(txid))
            .collect()
    }

    /// Find transactions that are waiting confirmation in the block. If transaction is appeared in
    /// the block, then it is confirmed and can be sent to the checkers. Otherwise it will be
    /// removed from the queue if it is waiting confirmation for too long.
    pub async fn clean_up_waiting_txs(&mut self) -> eyre::Result<()> {
        if self.queue.is_empty() {
            return Ok(());
        }

        // Remove transactions that are waiting confirmation for too long.
        for (txid, created_at) in self.queue.clone().into_iter() {
            if created_at.elapsed().unwrap() > self.max_confirmation_time {
                tracing::debug!(
                    "Transaction {:?} is waiting confirmation for too long. Removing from queue.",
                    txid
                );

                self.queue.remove(&txid);
            }
        }

        Ok(())
    }

    async fn new_confirmed_txs(&mut self, yuv_tx_ids: &[Txid]) {
        tracing::debug!("Transactions confirmed: {:?}", yuv_tx_ids);
        for tx_id in yuv_tx_ids {
            self.queue.remove(tx_id);
        }

        self.event_bus
            .send(ControllerMessage::ConfirmedTxs(yuv_tx_ids.to_vec()))
            .await;
    }
}
