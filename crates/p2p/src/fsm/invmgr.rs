//! Inventory manager.
//! Takes care of sending and fetching inventories.
//!
//! ## Handling of reverted blocks

use bitcoin::{
    locktime::absolute::Height, network::constants::ServiceFlags, Block, BlockHash, Txid,
};
use std::collections::HashMap;

use std::net::SocketAddr;

use super::output::{SetTimer, Wire};
use crate::{
    common::collections::AddressBook,
    fsm::handler::{PeerId, Socket},
    net::{LocalDuration, LocalTime},
};

/// An event emitted by the inventory manager.
#[derive(Debug, Clone)]
pub enum Event {
    /// A peer acknowledged one of our transaction inventories.
    Acknowledged {
        /// The acknowledged transaction ID.
        txid: Txid,
        /// The acknowledging peer.
        peer: PeerId,
    },
}

impl std::fmt::Display for Event {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Acknowledged { txid, peer } => {
                write!(
                    fmt,
                    "Transaction {} was acknowledged by peer {}",
                    txid, peer
                )
            }
        }
    }
}

/// Inventory manager peer.
#[derive(Debug)]
pub struct Peer {
    /// Is this peer a transaction relay?
    pub relay: bool,
    /// Peer announced services.
    pub services: ServiceFlags,
    /// Whether this peer use BIP-339
    pub wtxidrelay: bool,
    /// Whether this peer use YUV protocol
    pub ytxidrelay: bool,

    /// Number of times a certain block was requested.
    #[allow(dead_code)]
    requests: HashMap<BlockHash, usize>,

    /// Peer socket.
    _socket: Socket,
}

impl Peer {
    #[allow(dead_code)]
    fn requested(&mut self, hash: BlockHash) {
        *self.requests.entry(hash).or_default() += 1;
    }
}

/// Inventory manager state.
#[derive(Debug)]
pub struct InventoryManager<U> {
    /// Peer map.
    peers: AddressBook<PeerId, Peer>,
    /// Blocks requested and the time at which they were last requested.
    pub remaining: HashMap<BlockHash, Option<LocalTime>>,
    /// Blocks received, waiting to be processed.
    pub received: HashMap<Height, Block>,

    last_tick: Option<LocalTime>,
    upstream: U,
}

impl<U: Wire<Event> + SetTimer> InventoryManager<U> {
    /// Create a new inventory manager.
    pub fn new(upstream: U) -> Self {
        Self {
            peers: AddressBook::new(),
            remaining: HashMap::new(),
            received: HashMap::new(),
            last_tick: None,
            upstream,
        }
    }

    /// Called when a peer is negotiated.
    pub fn peer_negotiated(
        &mut self,
        socket: Socket,
        services: ServiceFlags,
        relay: bool,
        wtxidrelay: bool,
        ytxidrelay: bool,
    ) {
        self.schedule_tick();
        self.peers.insert(
            socket.addr,
            Peer {
                services,
                relay,
                requests: HashMap::new(),
                _socket: socket,
                wtxidrelay,
                ytxidrelay,
            },
        );
    }

    /// Called when a peer disconnected.
    pub fn peer_disconnected(&mut self, id: &PeerId) {
        self.peers.remove(id);
    }

    pub fn is_connected(&mut self, addr: &SocketAddr) -> bool {
        self.peers.contains_key(addr)
    }

    fn schedule_tick(&mut self) {
        self.last_tick = None; // Disable rate-limiting for the next tick.
        self.upstream.set_timer(LocalDuration::from_secs(1));
    }
}
