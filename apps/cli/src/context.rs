use std::time::Duration;
use std::{path::PathBuf, sync::Arc};

use bitcoin::secp256k1::{All, Secp256k1};
use color_eyre::eyre::{self, bail, Context as EyreContext};
use indicatif::{ProgressBar, ProgressStyle};
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};

use crate::config::Config;
use bdk::blockchain::rpc::Auth;
use bdk::blockchain::AnyBlockchain;
use bitcoin_client::{BitcoinRpcAuth, BitcoinRpcClient};
use ydk::bitcoin_provider::{BitcoinProvider, BitcoinProviderConfig};
use ydk::wallet::{StorageWallet, StorageWalletConfig, SyncOptions, WalletConfig};
use ydk::AnyBitcoinProvider;

/// Context is a struct which holds all information that could be used globally, like info from
/// configuration file. All the data taken from context is evaluated lazily, so it's not a problem
/// to create it once and use it everywhere.
pub struct Context {
    /// Stored path to configuration file, ti lazy load it when needed.
    config_path: PathBuf,

    /// Global secp256k1 context, used for signing and verifying signatures.
    secp_ctx: Secp256k1<All>,

    /// Loaded configuration file.
    config: Option<Config>,

    /// Yuv RPC client.
    yuv_client: Option<HttpClient>,

    /// Provider of Bitcoin network
    bitcoin_provider: Option<Arc<AnyBitcoinProvider>>,

    /// YUV wallet.
    yuv_wallet: Option<Arc<StorageWallet>>,
}

impl Context {
    pub fn new(config: PathBuf) -> Self {
        let secp_ctx = Secp256k1::new();

        Self {
            config_path: config,
            secp_ctx,
            config: None,
            yuv_client: None,
            bitcoin_provider: None,
            yuv_wallet: None,
        }
    }

    pub fn config(&mut self) -> eyre::Result<Config> {
        if let Some(config) = &self.config {
            return Ok(config.clone());
        }

        let cfg: Config =
            Config::from_path(self.config_path.clone()).wrap_err("Failed to load config")?;

        self.config = Some(cfg.clone());

        Ok(cfg)
    }

    pub fn secp_ctx(&self) -> &Secp256k1<All> {
        &self.secp_ctx
    }

    pub fn yuv_client(&mut self) -> eyre::Result<HttpClient> {
        let cfg = self.config()?;

        let client = HttpClientBuilder::new().build(cfg.yuv_rpc.url.clone())?;

        self.yuv_client = Some(client.clone());

        Ok(client)
    }

    /// Returns the `Bitcoin RPC` client. The params that we pass to the function are more
    /// prioritized in a setup process. When params are `None`, we use `BitcoinProviderConfig`
    /// configuration to fill `BitcoinRpcClient` required fields.
    pub async fn bitcoin_client(
        &mut self,
        rpc_url: Option<String>,
        rpc_auth: Option<String>,
        route: Option<String>,
    ) -> eyre::Result<BitcoinRpcClient> {
        let config = self.config()?;
        let bitcoin_client = match config.bitcoin_provider {
            BitcoinProviderConfig::Esplora(_) | BitcoinProviderConfig::BitcoinRpc(_)
                if rpc_url.is_some() =>
            {
                let mut rpc_url = rpc_url.expect("rpc_url should exist, see condition above");
                rpc_url.push_str(&route.unwrap_or_default());
                BitcoinRpcClient::new(rpc_auth_from_string(rpc_auth)?, rpc_url, None).await?
            }

            BitcoinProviderConfig::Esplora(_) => bail!(
                "Unable to init a Bitcoin RPC client. Use --rpc-url and --rpc-auth with Esplora"
            ),

            BitcoinProviderConfig::BitcoinRpc(cfg) => {
                let mut rpc_url = cfg.url.clone();
                rpc_url.push_str(&route.unwrap_or_default());
                BitcoinRpcClient::new(bitcoin_rpc_auth_convert(cfg.auth), rpc_url, None).await?
            }
        };

        Ok(bitcoin_client)
    }

    pub fn blockchain(&mut self) -> eyre::Result<Arc<AnyBlockchain>> {
        let provider = self.bitcoin_provider()?;

        Ok(provider.blockchain())
    }

    pub fn bitcoin_provider(&mut self) -> eyre::Result<Arc<AnyBitcoinProvider>> {
        if let Some(provider) = &self.bitcoin_provider {
            return Ok(provider.clone());
        }

        let config = self.config()?;

        let wallet_cfg: WalletConfig = config.into();
        let provider = Arc::new(AnyBitcoinProvider::from_config(wallet_cfg.try_into()?)?);

        self.bitcoin_provider = Some(provider.clone());

        Ok(provider)
    }

    pub async fn wallet(&mut self) -> eyre::Result<Arc<StorageWallet>> {
        if let Some(wallet) = &self.yuv_wallet {
            return Ok(wallet.clone());
        }

        let config = self.config()?;

        let wallet = StorageWallet::from_storage_config(StorageWalletConfig {
            inner: WalletConfig {
                privkey: config.private_key,
                bitcoin_provider: config.bitcoin_provider.clone(),
                yuv_url: config.yuv_rpc.url.clone(),
                network: config.network(),
            },
            storage_path: config.storage.clone(),
        })
        .await?;

        let pb = setup_progress_bar("Syncing yuv and bitcoin wallets...".into());
        wallet.sync(SyncOptions::default()).await?;
        pb.finish();

        let wallet = Arc::new(wallet);
        self.yuv_wallet = Some(wallet.clone());

        Ok(wallet)
    }
}

/// Setups progress bar that will appear in console for an adjusted while
fn setup_progress_bar(message: String) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {spinner:.red} {msg}")
            .unwrap()
            .tick_strings(&[
                "▹▹▹▹▹",
                "▸▹▹▹▹",
                "▹▸▹▹▹",
                "▹▹▸▹▹",
                "▹▹▹▸▹",
                "▹▹▹▹▸",
                "▪▪▪▪▪",
            ]),
    );
    pb.set_message(message);

    pb
}

/// Converts `bdk::Auth` to `bitcoin_client::BitcoinRpcAuth`
fn bitcoin_rpc_auth_convert(auth: Auth) -> BitcoinRpcAuth {
    match auth {
        Auth::None => BitcoinRpcAuth::None,
        Auth::UserPass { username, password } => BitcoinRpcAuth::UserPass { username, password },
        Auth::Cookie { file } => BitcoinRpcAuth::Cookie { file },
    }
}

/// Converst rpc_auth string with format: `[rpc_user:rpc_password]` to BitcoinRpcAuth
fn rpc_auth_from_string(rpc_auth: Option<String>) -> eyre::Result<BitcoinRpcAuth> {
    let Some(auth) = rpc_auth else {
        return Ok(BitcoinRpcAuth::None);
    };

    let Some((user, pass)) = auth.rsplit_once(':') else {
        bail!(
            "Failed to parse --rpc-auth parameter, use the following format: [username]:[password]"
        )
    };

    Ok(BitcoinRpcAuth::UserPass {
        username: user.to_string(),
        password: pass.to_string(),
    })
}
