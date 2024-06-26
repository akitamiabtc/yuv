mod nodes;
use std::{path::PathBuf, time::Duration};

use bdk::blockchain::{
    rpc::{Auth, RpcSyncParams},
    ConfigurableBlockchain, RpcBlockchain, RpcConfig,
};
use eyre::OptionExt;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
pub(crate) use nodes::{BitcoinNode, NodesConfig};

mod accounts;
mod checker;
mod miner;
mod report;
use accounts::AccountsConfig;
use checker::CheckerConfig;
use miner::MinerConfig;
use rand::{seq::IteratorRandom, thread_rng};
use report::ReportConfig;
use serde::Deserialize;

use config::Config;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct TestConfig {
    pub duration: Option<Duration>,
    pub nodes: NodesConfig,
    pub accounts: AccountsConfig,
    pub checker: CheckerConfig,
    pub report: ReportConfig,
    pub miner: MinerConfig,
}

impl TestConfig {
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config = Config::builder()
            .add_source(config::File::from(path))
            .build()?;

        Ok(config.try_deserialize()?)
    }

    // Get a random YUV node.
    pub fn get_yuv_node(&self) -> eyre::Result<(String, HttpClient)> {
        let node = self
            .nodes
            .yuv
            .iter()
            .choose(&mut thread_rng())
            .ok_or_eyre("At least one YUV node should be present")?;

        Ok((node.to_string(), HttpClientBuilder::new().build(node)?))
    }

    // Get a random Bitcoin node.
    pub fn get_bitcoin_node(
        &self,
        wallet_name: &str,
    ) -> eyre::Result<(BitcoinNode, RpcBlockchain)> {
        self.nodes
            .bitcoin
            .iter()
            .choose(&mut thread_rng())
            .map(|node| {
                let auth = match &node.auth {
                    Some(auth) => Auth::UserPass {
                        username: auth.username.to_string(),
                        password: auth.password.to_string(),
                    },
                    None => Auth::None,
                };

                let wallet_name = format!("test_wallet_{wallet_name}");

                Ok((
                    node.clone(),
                    RpcBlockchain::from_config(&RpcConfig {
                        url: node.url.to_string(),
                        auth,
                        network: bitcoin::Network::Regtest,
                        wallet_name,
                        sync_params: Some(RpcSyncParams {
                            start_time: 0,
                            ..Default::default()
                        }),
                    })?,
                ))
            })
            .expect("At least one Bitcoin node should be present")
    }
}
