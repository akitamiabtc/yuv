use std::time::Duration;

use bdk::{
    bitcoincore_rpc::RpcApi,
    blockchain::{GetHeight, RpcBlockchain},
};
use bitcoin::Address;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct Miner {
    receiver: Address,
    rpc_blockchain: RpcBlockchain,
}

impl Miner {
    pub fn new(receiver: Address, rpc_blockchain: RpcBlockchain) -> Self {
        Self {
            receiver,
            rpc_blockchain,
        }
    }

    pub async fn run(
        self,
        interval: Duration,
        cancellation_token: CancellationToken,
    ) -> eyre::Result<()> {
        let mut timer = tokio::time::interval(interval);

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("cancellation received, stopping");
                    return Ok(());
                }
                _ = timer.tick() => {},
            };

            self.rpc_blockchain.generate_to_address(1, &self.receiver)?;
            let height = self.rpc_blockchain.get_height()?;

            info!(
                "Just mined a block at height {}. Will mine the next one in {} seconds",
                height,
                interval.as_secs()
            );
        }
    }
}
