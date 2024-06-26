use std::{collections::HashMap, sync::Arc, time::Duration};

use bdk::blockchain::EsploraBlockchain;
use eyre::OptionExt;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use ydk::{
    bitcoin_provider::{BitcoinProviderConfig, BitcoinRpcConfig, EsploraConfig},
    wallet::{MemoryWallet, WalletConfig},
};

use yuv_types::YuvTransaction;

use crate::{
    cli::{faucet::Faucet, miner::Miner, tx_checker::TxChecker},
    config::TestConfig,
};
use bdk::blockchain::rpc::Auth;
use bitcoin::{
    secp256k1::{
        rand::{seq::IteratorRandom, thread_rng},
        All, Secp256k1,
    },
    Network, PrivateKey, PublicKey,
};
use tokio::sync::mpsc::unbounded_channel;
use tracing::{info, span, Instrument, Level};
use yuv_pixels::Chroma;

use super::account::Account;

pub(crate) const NETWORK: Network = Network::Regtest;
// Stop gap is a part of Esplora config.
const STOP_GAP: usize = 100000;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) struct E2e {
    config: TestConfig,

    task_tracker: TaskTracker,
    cancellation_token: CancellationToken,
}

impl E2e {
    pub async fn new(config: TestConfig) -> eyre::Result<Self> {
        Ok(Self {
            config,
            task_tracker: TaskTracker::new(),
            cancellation_token: CancellationToken::new(),
        })
    }

    /// Start the End-to-End test.
    pub async fn run(&self) -> eyre::Result<()> {
        info!(
            "Starting the end-to-end test with {} account(s), {} YUV node(s) and {} Bitcoin node(s)",
            self.config.accounts.number,
            self.config.nodes.yuv.len(),
            self.config.nodes.bitcoin.len()
        );
        if let Some(duration) = self.config.duration {
            info!("Test will end in {:?}", duration);
        };

        // Generate the accounts
        let accounts = Self::generate_accounts(&self.config).await?;

        let mut recipients = Vec::new();
        for account in &accounts {
            recipients.push(account.private_key());
        }

        // Convert private keys to `Vec<ScriptBuf>`, that will be used to fund the accounts with satoshis.
        let funding_recipients = Arc::new(
            recipients
                .clone()
                .iter()
                .map(|sk| sk.public_key(&Secp256k1::new()))
                .collect::<Vec<PublicKey>>(),
        );

        // Pick the funder.
        let funder = Self::generate_account(&self.config, &Secp256k1::new(), true).await?;
        // Init the miner.
        let miner = Miner::new(
            funder.p2wpkh_address()?,
            self.config.get_bitcoin_node("miner")?.1,
        );

        // Spawn the miner.
        let cancellation_token = self.cancellation_token.clone();
        self.task_tracker.spawn(
            miner
                .run(self.config.miner.interval, cancellation_token)
                .instrument(span!(Level::ERROR, "miner")),
        );
        // Init the faucet.
        let faucet = Faucet::new(funder, self.config.get_bitcoin_node("faucet")?.1);
        let faucet_span = span!(Level::ERROR, "faucet");

        // Perform the initial funding.
        faucet
            .fund_accounts(Arc::clone(&funding_recipients))
            .instrument(faucet_span.clone())
            .await?;

        // Spawn the faucet.
        let cancellation_token = self.cancellation_token.clone();
        let funding_interval = Duration::from_secs(self.config.accounts.funding_interval);
        self.task_tracker.spawn(
            faucet
                .run(
                    funding_interval,
                    Arc::clone(&funding_recipients),
                    cancellation_token,
                )
                .instrument(faucet_span),
        );

        // Initialize the queue to send Txids from accounts to the tx checker.
        let (tx_sender, tx_receiver) = unbounded_channel::<YuvTransaction>();
        // Initialize the queue to send balances from accounts to the tx checker.
        let (balance_sender, balance_receiver) =
            unbounded_channel::<(PrivateKey, HashMap<Chroma, u128>)>();

        // Initialize the tx checker.
        let tx_checker = TxChecker::new(
            self.config.clone(),
            self.config.get_yuv_node()?.1,
            self.config.get_bitcoin_node("tx-checker")?.1,
        );

        let cancellation_token = self.cancellation_token.clone();
        // Spawn the tx checker.
        self.task_tracker.spawn(
            tx_checker
                .run(cancellation_token, tx_receiver, balance_receiver)
                .instrument(span!(Level::ERROR, "tx-checker")),
        );

        // Just a performance optimization.
        let shared_recipients: Arc<[PrivateKey]> = recipients.into();
        // Run the accounts
        for account in accounts {
            let cancellation_token = self.cancellation_token.clone();
            let recipients = Arc::clone(&shared_recipients);

            let mode = account.connection_method();
            let span = span!(
                Level::ERROR,
                "account",
                private_key = account.private_key().to_string(),
                mode
            );

            let tx_sender = tx_sender.clone();
            let balance_sender = balance_sender.clone();

            self.task_tracker.spawn(
                account
                    .run(recipients, tx_sender, balance_sender, cancellation_token)
                    .instrument(span),
            );
        }

        self.task_tracker.close();

        Ok(())
    }

    /// `shutdown` handles the graceful shutdown of the test.
    pub async fn shutdown(self) {
        info!("Gracefully stopping the test");

        self.cancellation_token.cancel();

        tokio::select! {
            _ = self.task_tracker.wait() => {},
            _ = tokio::time::sleep(SHUTDOWN_TIMEOUT) => {
                info!("Shutdown timeout reached, exiting...");
            },
        }
    }

    /// Generate a keypair with random YUV and Bitcoin nodes.
    async fn generate_accounts(config: &TestConfig) -> eyre::Result<Vec<Account>> {
        let mut accounts = Vec::with_capacity(config.accounts.number as usize);

        // Specified amount of accounts should use a Bitcoin node. Others use Esplora.
        let threshold = (config.accounts.threshold * config.accounts.number as f32).ceil() as u32;
        info!(
            "{}/{} accounts are going to be connected to Bitcoin RPC",
            threshold, config.accounts.number
        );

        let secp = Secp256k1::new();

        // Generate N accounts.
        for i in 0..config.accounts.number {
            let btc_node = i < threshold;

            let account = Self::generate_account(config, &secp, btc_node).await?;

            info!(
                "Generated {} account (connected to {})",
                i + 1,
                account.connection_method()
            );

            accounts.push(account);
        }

        Ok(accounts)
    }

    async fn generate_account(
        config: &TestConfig,
        secp: &Secp256k1<All>,
        has_btc_node: bool,
    ) -> eyre::Result<Account> {
        let (seckey, _pubkey) = secp.generate_keypair(&mut thread_rng());

        let private_key = PrivateKey::new(seckey, NETWORK);
        let pubkey = private_key.public_key(secp);

        // Pick a random YUV node for the account.
        let (yuv_url, yuv_client) = config.get_yuv_node()?;

        // Pick a random Esplora URL for the account.
        let esplora_url = &config
            .nodes
            .esplora
            .iter()
            .choose(&mut thread_rng())
            .expect("At least one Esplora URL should be specified");

        let btc_node = if has_btc_node {
            Some(config.get_bitcoin_node(&pubkey.to_string())?)
        } else {
            None
        };

        // Set up the wallet. If the account uses Bitcoin RPC, the wallet should also
        // be constructed using `BitcoinRpcConfig`. Otherwise - `EsploraConfig`.
        let provider_config = if let Some(btc_node) = &btc_node {
            let auth = btc_node
                .0
                .auth
                .as_ref()
                .ok_or_eyre("Bitcoin auth should be specified")?;
            BitcoinProviderConfig::BitcoinRpc(BitcoinRpcConfig {
                url: btc_node.0.url.clone(),
                auth: Auth::UserPass {
                    username: auth.username.clone(),
                    password: auth.password.clone(),
                },
                network: NETWORK,
                start_time: 0,
            })
        } else {
            BitcoinProviderConfig::Esplora(EsploraConfig {
                url: esplora_url.to_string(),
                network: NETWORK,
                stop_gap: STOP_GAP,
            })
        };

        let wallet =
            Self::setup_wallet_from_provider(private_key, yuv_url, provider_config).await?;

        Ok(Account::new(
            private_key,
            yuv_client,
            EsploraBlockchain::new(esplora_url, STOP_GAP),
            btc_node.map(|node| node.1),
            wallet,
        ))
    }

    async fn setup_wallet_from_provider(
        privkey: PrivateKey,
        yuv_url: String,
        bitcoin_provider: BitcoinProviderConfig,
    ) -> eyre::Result<MemoryWallet> {
        let wallet = MemoryWallet::from_config(WalletConfig {
            privkey,
            network: NETWORK,
            yuv_url,
            bitcoin_provider,
        })
        .await?;

        Ok(wallet)
    }
}
