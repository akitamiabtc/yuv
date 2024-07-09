use crate::Subindexer;
use async_trait::async_trait;
use bitcoin_client::json::GetBlockTxResult;
use event_bus::{typeid, EventBus};
use yuv_types::TxConfirmMessage;

/// Is responsible for waiting confirmations of transactions in Bitcoin.
pub struct ConfirmationIndexer {
    event_bus: EventBus,
}

impl ConfirmationIndexer {
    pub fn new(full_event_bus: &EventBus) -> Self {
        let event_bus = full_event_bus
            .extract(&typeid![TxConfirmMessage], &typeid![])
            .expect("event channels must be presented");

        Self { event_bus }
    }

    /// Handle transactions that are waiting confirmation in the block.
    pub async fn handle_new_block(&mut self, block: GetBlockTxResult) -> eyre::Result<()> {
        self.event_bus
            .send(TxConfirmMessage::Block(Box::new(block)))
            .await;

        Ok(())
    }
}

#[async_trait]
impl Subindexer for ConfirmationIndexer {
    async fn index(&mut self, block: &GetBlockTxResult) -> eyre::Result<()> {
        self.handle_new_block(block.clone()).await
    }
}
