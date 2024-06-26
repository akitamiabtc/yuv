use std::{net::SocketAddr, str::FromStr};

use bitcoin::network::{constants::ServiceFlags, Address};
use yuv_types::network::Network;

use crate::common::peer::{KnownAddress, Source, Store};

use super::peer::Cache;

const TESTNET: &[&str] = &[];
const MAINNET: &[&str] = &[];
const MUTINY: &[&str] = &[];

/// Update the list of peers with the hard coded boot nodes for the given [Network].
pub(crate) fn insert_boot_nodes(peers: &mut Cache, network: Network) {
    match network {
        Network::Bitcoin => {
            tracing::debug!("Adding {} mainnet boot nodes", MAINNET.len());
            insert(peers, MAINNET);
        }
        Network::Testnet => {
            tracing::debug!("Adding {} testnet boot nodes", TESTNET.len());
            insert(peers, TESTNET)
        }
        Network::Mutiny => {
            tracing::debug!("Adding {} Mutiny boot nodes", MUTINY.len());
            insert(peers, MUTINY)
        }
        _ => {
            tracing::debug!("No boot nodes provided for the given network");
        }
    }
}

fn insert(peers: &mut Cache, boot_nodes: &[&str]) {
    boot_nodes.iter().for_each(|boot_node_url| {
        let boot_node_addr = SocketAddr::from_str(boot_node_url).expect("Address should be valid");

        peers.insert(
            &boot_node_addr,
            KnownAddress::new(
                Address::new(&boot_node_addr, ServiceFlags::NONE),
                Source::Imported,
                None,
            ),
        );
    });
}
