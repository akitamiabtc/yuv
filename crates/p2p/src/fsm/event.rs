//! State machine events.
use crate::net::LocalTime;
use bitcoin::locktime::absolute::Height;
use std::sync::Arc;
use yuv_types::messages::p2p::NetworkMessage;

use crate::fsm::{
    addrmgr::Event as AddressEvent, invmgr::Event as InventoryEvent, peermgr::Event as PeerEvent,
    pingmgr::Event as PingEvent,
};

/// A peer-to-peer event.
#[derive(Debug, Clone)]
pub enum Event {
    /// The node is initializing its state machine and about to start network activity.
    Initializing,
    /// The node is initialized and ready to receive commands.
    Ready {
        /// Block header height.
        height: Height,
        /// Filter header height.
        filter_height: Height,
        /// Local time.
        time: LocalTime,
    },
    /// Received a message from a peer.
    Received(NetworkMessage),
    /// An address manager event.
    Address(AddressEvent),
    /// A peer manager event.
    Peer(PeerEvent),
    /// An inventory manager event.
    Inventory(InventoryEvent),
    /// A ping manager event.
    Ping(PingEvent),
    Error(Arc<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<PeerEvent> for Event {
    fn from(e: PeerEvent) -> Self {
        Self::Peer(e)
    }
}

impl From<AddressEvent> for Event {
    fn from(e: AddressEvent) -> Self {
        Self::Address(e)
    }
}

impl From<InventoryEvent> for Event {
    fn from(e: InventoryEvent) -> Self {
        Self::Inventory(e)
    }
}

impl From<PingEvent> for Event {
    fn from(e: PingEvent) -> Self {
        Self::Ping(e)
    }
}
