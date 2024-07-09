use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::sync::mpsc::UnboundedSender;

use bdk::blockchain::{EsploraBlockchain, RpcBlockchain};
use bitcoin::{
    secp256k1::{
        rand::{seq::IteratorRandom, thread_rng},
        Secp256k1,
    },
    Address, PrivateKey,
};
use jsonrpsee::http_client::HttpClient;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use ydk::{
    types::FeeRateStrategy,
    wallet::{MemoryWallet, SyncOptions},
};
use yuv_pixels::{Chroma, Pixel};
use yuv_rpc_api::transactions::YuvTransactionsRpcClient;
use yuv_types::{YuvTransaction, YuvTxType};

use super::e2e::NETWORK;

/// Minimum transfer amount.
const TRANSFER_LOWER_BOUND: u128 = 1000;
const TRANFERS_PER_ISSUANCE: u32 = 6;

/// Amount of tokens to issue.
///
/// The formula is `ISSUE_AMOUNT=Q*2**N` where `Q` is `TRANSFER_LOWER_BOUND` and `N` is the desired number of transactions
/// that can be performed from a single issuance.
/// This makes sense as at each iteration it is checked if the balance is higher than `TRANSFER_LOWER_BOUND`.
/// If it is, then half the balance is sent. Otherwise - the whole balance is sent.
const ISSUE_AMOUNT: u128 = TRANSFER_LOWER_BOUND * 2u128.pow(TRANFERS_PER_ISSUANCE);

/// Amount of satoshis to put into each YUV output.
const SATOSHIS_AMOUNT: u64 = 1000;

const ERROR_SLEEP_DURATION: Duration = Duration::from_secs(1);
const CANCELLATION_DURATION: Duration = Duration::from_secs(5);
const TX_SENDING_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) static FEE_RATE_STARTEGY: FeeRateStrategy = FeeRateStrategy::Manual { fee_rate: 1.2 };

pub(crate) struct Account {
    private_key: PrivateKey,

    yuv_client: HttpClient,
    esplora: EsploraBlockchain,
    rpc_blockchain: Option<RpcBlockchain>,

    wallet: MemoryWallet,
}

impl Account {
    pub fn new(
        private_key: PrivateKey,
        yuv_client: HttpClient,
        esplora: EsploraBlockchain,
        rpc_blockchain: Option<RpcBlockchain>,
        wallet: MemoryWallet,
    ) -> Self {
        Self {
            private_key,
            yuv_client,
            esplora,
            rpc_blockchain,
            wallet,
        }
    }

    /// Start sending transactions.
    pub async fn run(
        self,
        recipients: Arc<[PrivateKey]>,
        tx_sender: UnboundedSender<YuvTransaction>,
        balance_sender: UnboundedSender<(PrivateKey, HashMap<Chroma, u128>)>,
        cancellation_token: CancellationToken,
    ) -> eyre::Result<()> {
        info!("Started sending transactions");

        let mut timer = tokio::time::interval(TX_SENDING_INTERVAL);

        loop {
            tokio::select! {
                _ = timer.tick() => {},
                // If a cancellation is received, stop sending transaction and send the balances to the `tx-checker`.
                _ = cancellation_token.cancelled() => {
                    self.finish(balance_sender).await?;
                    return Ok(());
                }
            }

            // Sync the wallet.
            self.wallet.sync(SyncOptions::default()).await?;

            // Create a raw YUV transaction.
            let tx: YuvTransaction = match self.build_transaction(&recipients).await {
                Ok(tx) => tx,
                // Report the error and sleep.
                Err(e) => {
                    warn!("tx failed: {}, sleeping", e);
                    sleep(ERROR_SLEEP_DURATION).await;
                    continue;
                }
            };

            let txid = tx.bitcoin_tx.txid();
            // Send the transaction.
            let response = self.yuv_client.send_yuv_tx(tx.hex(), None).await;
            if response.is_ok() {
                let tx_type = tx_type(&tx.tx_type);
                info!("{} tx sent | Txid: {}", tx_type, txid);

                // Send the TX to the tx checker.
                tx_sender.send(tx)?;
                continue;
            }

            warn!("Mempool conflict | Txid: {}", txid);
        }
    }

    async fn finish(
        mut self,
        balance_sender: UnboundedSender<(PrivateKey, HashMap<Chroma, u128>)>,
    ) -> eyre::Result<()> {
        debug!("Finished sending transactions, sending balances to the Tx checker and stopping");
        tokio::time::sleep(CANCELLATION_DURATION).await;

        self.send_balances(balance_sender).await
    }

    /// Builds a random YUV transaction.
    ///
    /// If there are no balances, it builds an issuance TX with a random recipient.
    /// If the address has balances, a transfer transaction will be built.
    async fn build_transaction(
        &self,
        recipients: &Arc<[PrivateKey]>,
    ) -> eyre::Result<YuvTransaction> {
        // Choose a random recipient.
        let recipient = recipients
            .iter()
            .choose(&mut thread_rng())
            .expect("Recipients should not be empty");

        let balances = self.wallet.balances().await?;
        // If there are no YUV tokens, issue some to the previously picked recipient.
        // Else send a transfer TX.
        if balances.yuv.is_empty() {
            self.issue(recipient).await
        } else {
            // Pick random Chroma and Luma.
            let (chroma, luma) = balances
                .yuv
                .iter()
                .choose(&mut thread_rng())
                .expect("At least one pixel should be present");

            self.transfer(recipient, Pixel::new(*luma, *chroma)).await
        }
    }

    /// Issue tokens to a random recipient.
    async fn issue(&self, recipient: &PrivateKey) -> eyre::Result<YuvTransaction> {
        let mut builder = self.wallet.build_issuance(None)?;
        let secp = Secp256k1::new();

        builder
            .add_recipient(
                &recipient.public_key(&secp).inner,
                ISSUE_AMOUNT,
                SATOSHIS_AMOUNT,
            )
            .set_fee_rate_strategy(FEE_RATE_STARTEGY)
            .set_drain_tweaked_satoshis(true);

        match &self.rpc_blockchain {
            Some(bc) => builder.finish(bc).await,
            None => builder.finish(&self.esplora).await,
        }
    }

    /// Transfer tokens to a random recipient.
    async fn transfer(&self, recipient: &PrivateKey, pixel: Pixel) -> eyre::Result<YuvTransaction> {
        let luma = pixel.luma.amount;

        // If the balance is bigger than the lower bound, send half of it.
        // Otherwise - send the whole balance in a single transfer.
        let amount = if luma > TRANSFER_LOWER_BOUND {
            luma / 2
        } else {
            luma
        };

        let mut builder = self.wallet.build_transfer()?;
        let secp = Secp256k1::new();

        builder
            .add_recipient(
                pixel.chroma,
                &recipient.public_key(&secp).inner,
                amount,
                SATOSHIS_AMOUNT,
            )
            .set_fee_rate_strategy(FEE_RATE_STARTEGY)
            .set_drain_tweaked_satoshis(true);

        match &self.rpc_blockchain {
            Some(bc) => builder.finish(bc).await,
            None => builder.finish(&self.esplora).await,
        }
    }

    /// `send_balances` sends the actual balances of the address to the `tx-checker` after the cancellation received.
    async fn send_balances(
        &mut self,
        balance_sender: UnboundedSender<(PrivateKey, HashMap<Chroma, u128>)>,
    ) -> eyre::Result<()> {
        self.wallet.sync(SyncOptions::yuv_only()).await?;

        let balances = self.wallet.balances().await?;
        balance_sender.send((self.private_key(), balances.yuv))?;

        Ok(())
    }

    // ==vvv== Getter methods ==vvv==

    pub(crate) fn private_key(&self) -> PrivateKey {
        self.private_key
    }

    pub(crate) fn wallet(&self) -> &MemoryWallet {
        &self.wallet
    }

    pub(crate) fn p2wpkh_address(&self) -> eyre::Result<Address> {
        let pubkey = self.private_key().public_key(&Secp256k1::new());
        Ok(Address::p2wpkh(&pubkey, NETWORK)?)
    }

    pub(crate) fn yuv_client(&self) -> &HttpClient {
        &self.yuv_client
    }

    pub(crate) fn connection_method(&self) -> String {
        if self.rpc_blockchain.is_some() {
            "RPC".into()
        } else {
            "Esplora".into()
        }
    }
}

/// String representation of the YUV transaction type.
pub(crate) fn tx_type(tx_type: &YuvTxType) -> String {
    match tx_type {
        YuvTxType::Issue { .. } => "Issuance".into(),
        YuvTxType::Transfer { .. } => "Transfer".into(),
        YuvTxType::Announcement(_) => "Announcement".into(),
    }
}
