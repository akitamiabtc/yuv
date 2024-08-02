//! Sub-indexer for announcements.

use async_trait::async_trait;

use bitcoin_client::json::GetBlockTxResult;
use event_bus::{typeid, EventBus};
use yuv_types::announcements::{announcement_from_script, ParseOpReturnError};
use yuv_types::{network::Network, ControllerMessage, YuvTransaction, YuvTxType};

use super::Subindexer;

/// A sub-indexer which gets announcements from blocks and sends them to message handler.
pub struct AnnouncementsIndexer {
    /// Event bus to notify controller about new announcements.
    event_bus: EventBus,
    network: Network,
}

impl AnnouncementsIndexer {
    pub fn new(full_event_bus: &EventBus, network: Network) -> Self {
        let event_bus = full_event_bus
            .extract(&typeid![ControllerMessage], &[])
            .expect("message to message handler must be registered");

        Self { event_bus, network }
    }

    /// Finds announcements in a block and sends them to message handler.
    async fn find_announcements(&self, block: &GetBlockTxResult) -> eyre::Result<()> {
        let mut txs = Vec::new();

        // For each transaction, try to find announcements.
        for tx in &block.tx {
            if tx.is_coin_base() {
                continue;
            }

            let mut announcement_opt = None;

            // In each transaction output: If it's not an OP_RETURN script - skip it, otherwise
            // push it to announcements.
            for output in tx.output.iter() {
                match announcement_from_script(&output.script_pubkey) {
                    Ok(announcement) => {
                        announcement_opt = Some(announcement.clone());
                    }
                    Err(ParseOpReturnError::InvaliOpReturnData(err)) => {
                        tracing::debug!("Found invalid announcement: {err}");
                    }
                    _ => {}
                };
            }

            let Some(announcement) = announcement_opt else {
                continue;
            };

            tracing::debug!("Found announcement in tx {}", tx.txid());

            if announcement.minimal_block_height(self.network) > block.block_data.height {
                tracing::debug!(
                    "Skipping invalid announcement, minimal block height requirement not met"
                );
                continue;
            }

            txs.push(YuvTransaction {
                bitcoin_tx: tx.clone(),
                tx_type: YuvTxType::Announcement(announcement),
            })
        }

        if !txs.is_empty() {
            self.event_bus
                .send(ControllerMessage::InitializeTxs(txs))
                .await;
        }

        Ok(())
    }
}

#[async_trait]
impl Subindexer for AnnouncementsIndexer {
    async fn index(&mut self, block: &GetBlockTxResult) -> eyre::Result<()> {
        self.find_announcements(block).await
    }
}
