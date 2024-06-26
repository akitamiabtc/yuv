//! Node handles are created from nodes by users of the library, to communicate with the underlying
//! protocol instance.
use std::net;
use std::net::SocketAddr;

use async_trait::async_trait;
use flume as chan;
use thiserror::Error;

use yuv_types::{
    messages::p2p::{Inventory, NetworkMessage},
    YuvTransaction,
};

use crate::{client::handle, fsm::handler::Command, fsm::handler::Peer, fsm::handler::PeerId};

/// An error resulting from a handle method.
#[derive(Error, Debug)]
pub enum Error {
    /// The command channel disconnected.
    #[error("command channel disconnected")]
    Disconnected,
    /// The command returned an error.
    #[error("command failed")]
    Command,
    /// The operation timed out.
    #[error("the operation timed out")]
    Timeout,
    /// An I/O error occured.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<chan::RecvError> for Error {
    fn from(_: chan::RecvError) -> Self {
        Self::Disconnected
    }
}

impl<T> From<chan::SendError<T>> for Error {
    fn from(_: chan::SendError<T>) -> Self {
        Self::Disconnected
    }
}

/// A handle for communicating with a node process.
#[async_trait]
pub trait Handle: Sized + Send + Sync + Clone {
    /// Send a command to the client.
    async fn command(&self, cmd: Command) -> Result<(), Error>;

    /// Broadcast a message to peers matching the predicate.
    /// To only broadcast to outbound peers, use Peer::is_outbound.
    async fn broadcast(
        &self,
        msg: NetworkMessage,
        predicate: fn(Peer) -> bool,
    ) -> Result<Vec<net::SocketAddr>, Error>;

    /// Send a message to a random *outbound* peer. Return the chosen
    /// peer or nothing if no peer was available.
    async fn query(&self, msg: NetworkMessage) -> Result<Option<net::SocketAddr>, Error>;
    async fn send_inv(&self, txids: Vec<Inventory>) -> Result<(), handle::Error>;
    async fn send_get_data(&self, txids: Vec<Inventory>, addr: PeerId)
        -> Result<(), handle::Error>;
    async fn send_yuv_txs(
        &self,
        txs: Vec<YuvTransaction>,
        addr: PeerId,
    ) -> Result<(), handle::Error>;
    async fn ban_peer(&self, addr: SocketAddr) -> Result<(), handle::Error>;
}

#[cfg(any(test, feature = "mocks"))]
mockall::mock! {
    pub Handle {}

    impl Clone for Handle {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl Handle for Handle {
        async fn command(&self, cmd: Command) -> Result<(), Error>;
        async fn broadcast(
            &self,
            msg: NetworkMessage,
            predicate: fn(Peer) -> bool,
        ) -> Result<Vec<net::SocketAddr>, Error>;
        async fn query(&self, msg: NetworkMessage) -> Result<Option<net::SocketAddr>, Error>;
        async fn send_inv(&self, txids: Vec<Inventory>) -> Result<(), handle::Error>;
        async fn send_get_data(&self, txids: Vec<Inventory>, addr: PeerId)
            -> Result<(), handle::Error>;
        async fn send_yuv_txs(
            &self,
            txs: Vec<YuvTransaction>,
            addr: PeerId,
        ) -> Result<(), handle::Error>;
        async fn ban_peer(&self, addr: SocketAddr) -> Result<(), handle::Error>;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_mock() {
        let mut _mock = MockHandle::new();
    }
}
