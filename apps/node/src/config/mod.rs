use config::Config;
use serde::Deserialize;
use yuv_types::network::Network;

use std::path::PathBuf;

mod p2p;
pub use p2p::P2pConfig;

mod rpc;
pub use rpc::RpcConfig;

mod storage;
pub use storage::StorageConfig;

mod checkers;
pub use checkers::CheckersConfig;

mod bnode;
pub use bnode::BitcoinConfig;

mod logger;
pub use logger::LoggerConfig;

mod indexer;
pub use indexer::IndexerConfig;

mod controller;

pub use controller::ControllerConfig;

#[derive(Deserialize)]
pub struct NodeConfig {
    #[serde(default = "default_network")]
    pub network: Network,

    pub p2p: P2pConfig,
    pub rpc: RpcConfig,
    pub bnode: BitcoinConfig,
    pub storage: StorageConfig,

    #[serde(default)]
    pub shutdown_timeout: Option<u64>,

    #[serde(default)]
    pub checkers: CheckersConfig,

    #[serde(default)]
    pub logger: LoggerConfig,

    #[serde(default)]
    pub indexer: IndexerConfig,

    #[serde(default)]
    pub controller: ControllerConfig,
}

fn default_network() -> Network {
    Network::Bitcoin
}

impl NodeConfig {
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config = Config::builder()
            .add_source(config::File::from(path))
            .build()?;

        Ok(config.try_deserialize()?)
    }
}
