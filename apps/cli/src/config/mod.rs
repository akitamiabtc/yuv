use std::path::PathBuf;

use bitcoin::{Network, PrivateKey};
use color_eyre::eyre;
use serde::{Deserialize, Serialize};

use ydk::bitcoin_provider::BitcoinProviderConfig;
use ydk::types::FeeRateStrategy;
use ydk::wallet::WalletConfig;
pub use yuvnode::YuvNodeConfig;

mod yuvnode;

#[derive(Deserialize, Clone, Serialize)]
pub struct Config {
    pub private_key: PrivateKey,

    pub bitcoin_provider: BitcoinProviderConfig,

    pub yuv_rpc: YuvNodeConfig,

    #[serde(default)]
    pub fee_rate_strategy: FeeRateStrategy,

    pub storage: PathBuf,
}

impl Config {
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::from(path))
            .build()?;

        Ok(config.try_deserialize()?)
    }

    pub fn network(&self) -> Network {
        match &self.bitcoin_provider {
            BitcoinProviderConfig::Esplora(cfg) => cfg.network,
            BitcoinProviderConfig::BitcoinRpc(cfg) => cfg.network,
        }
    }

    /// Serialize and save configuration to a file specified in `path`.
    pub fn save_to_file(&self, path: PathBuf) -> eyre::Result<()> {
        let serialized = toml::to_string_pretty(&self)?;

        std::fs::write(path, serialized)?;

        Ok(())
    }
}

impl From<Config> for WalletConfig {
    fn from(value: Config) -> Self {
        Self {
            privkey: value.private_key,
            network: value.network(),
            bitcoin_provider: value.bitcoin_provider,
            yuv_url: value.yuv_rpc.url,
        }
    }
}
