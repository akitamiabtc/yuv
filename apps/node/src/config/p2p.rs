use eyre::{Context, OptionExt};
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, ToSocketAddrs};
use yuv_p2p::client;
use yuv_types::network::Network;

/// Default number of peers connected to this node.
pub const DEFAULT_MAX_INBOUND_CONNECTIONS: usize = 125;

/// Default number of peers this node is connected to.
pub const DEFAULT_MAX_OUTBOUND_CONNECTIONS: usize = 8;

#[derive(Serialize, Deserialize, Clone)]
pub struct P2pConfig {
    /// Address to listen to incoming connections
    pub address: String,
    /// Maximum amount of inbound connections
    #[serde(default = "default_max_inbound_connections")]
    pub max_inbound_connections: usize,
    /// Maximum amount of outbound connections
    #[serde(default = "default_max_outbound_connections")]
    pub max_outbound_connections: usize,
    /// List of nodes to connect to firstly.
    #[serde(default)]
    pub bootnodes: Vec<String>,
}

fn default_max_inbound_connections() -> usize {
    DEFAULT_MAX_INBOUND_CONNECTIONS
}

fn default_max_outbound_connections() -> usize {
    DEFAULT_MAX_OUTBOUND_CONNECTIONS
}

impl P2pConfig {
    pub fn to_client_config(&self, network: Network) -> eyre::Result<client::P2PConfig> {
        let bootnodes: Vec<SocketAddr> = self
            .bootnodes
            .iter()
            .map(|x| {
                x.to_socket_addrs()
                    .wrap_err("Failed to resolve bootnode address")
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();

        let address = self
            .address
            .to_socket_addrs()
            .wrap_err("Failed to resolve address")?
            .next()
            .ok_or_eyre("No address found in listen address")?;

        Ok(client::P2PConfig::new(
            network,
            address,
            bootnodes,
            self.max_inbound_connections,
            self.max_outbound_connections,
        ))
    }
}
