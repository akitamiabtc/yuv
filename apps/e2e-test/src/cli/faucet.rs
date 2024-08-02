use std::{cmp, sync::Arc, time::Duration};

use bitcoin::PublicKey;

use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use ydk::wallet::SyncOptions;
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;

use bdk::{bitcoincore_rpc::RpcApi, blockchain::RpcBlockchain};

use crate::cli::account::FEE_RATE_STARTEGY;

use super::account::Account;

const FUNDING_LOWER_BOUND: u64 = 200_000;
const BLOCKS_PER_ACCOUNT: usize = 10;
const COINBASE_MATURITY: u64 = 101;

pub struct Faucet {
    funder: Account,
    rpc_blockchain: RpcBlockchain,
}

impl Faucet {
    pub fn new(funder: Account, rpc_blockchain: RpcBlockchain) -> Self {
        Self {
            funder,
            rpc_blockchain,
        }
    }

    /// Fund the addresses with satoshis.
    pub async fn fund_accounts(&self, recipients: Arc<Vec<PublicKey>>) -> eyre::Result<()> {
        info!("Funding the accounts with satoshis, the process can take a while");

        // Sync to fetch the Bitcoin UTXOs.
        self.funder
            .wallet()
            .sync(SyncOptions::bitcoin_only())
            .await?;
        let mut balance = self.funder.wallet().bitcoin_balances()?.confirmed;

        // If the balance is insufficient, generate some blocks and sync again.
        while balance < FUNDING_LOWER_BOUND {
            // Generate at least 101 blocks.
            self.rpc_blockchain.generate_to_address(
                cmp::max(
                    (recipients.len() * BLOCKS_PER_ACCOUNT).try_into().unwrap(),
                    COINBASE_MATURITY,
                ),
                &self.funder.p2wpkh_address()?,
            )?;

            self.funder
                .wallet()
                .sync(SyncOptions::bitcoin_only())
                .await?;
            balance = self.funder.wallet().bitcoin_balances()?.confirmed;
        }

        // Calculate the amount to fund each address with.
        let funding_amount = cmp::min(FUNDING_LOWER_BOUND, balance) / (recipients.len() + 1) as u64;

        info!(
            r#"
        Distributing {} satoshis
        Will fund each account with {} satoshis"#,
            balance, funding_amount
        );

        // Build the funding tx.
        let tx = {
            let mut builder = self.funder.wallet().build_transfer()?;

            // Add each address as a recipient to the tx.
            for recipient in recipients.iter() {
                builder.add_sats_recipient(&recipient.inner, funding_amount);
            }

            builder.set_fee_rate_strategy(FEE_RATE_STARTEGY);
            builder.finish(&self.rpc_blockchain).await?
        };

        self.funder.yuv_client().send_yuv_tx(tx.hex(), None).await?;
        self.rpc_blockchain
            .generate_to_address(6, &self.funder.p2wpkh_address()?)?;

        info!("Successfully funded the accounts with satoshis");

        Ok(())
    }

    /// Run the faucet that will fund the selected addresses with satoshis each N seconds.
    pub async fn run(
        self,
        interval: Duration,
        recipients: Arc<Vec<PublicKey>>,
        cancellation_token: CancellationToken,
    ) -> eyre::Result<()> {
        // Sleeping after the initial funding.
        tokio::time::sleep(interval).await;

        let mut timer = tokio::time::interval(interval);

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("cancellation received, stopping");
                    return Ok(());
                }
                _ = timer.tick() => {},
            };

            if let Err(err) = self.fund_accounts(Arc::clone(&recipients)).await {
                error!("Funding iteration failed: {}", err);
            } else {
                info!("Funding iteration succeeded");
            };

            info!("Next funding will happen in {} seconds", interval.as_secs());
        }
    }
}
