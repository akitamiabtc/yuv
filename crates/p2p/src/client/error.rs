//! Node error module.
use flume as chan;
use std::{io, process::Command};
use thiserror::Error;

/// A client error.
#[derive(Error, Debug)]
pub enum Error {
    /// An error occuring from a client handle.
    #[error(transparent)]
    Handle(#[from] crate::client::handle::Error),
    /// An error coming from the networking sub-system.
    #[error(transparent)]
    Net(#[from] crate::net::error::Error),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// An error coming from the peer store.
    #[error("error loading peers: {0}")]
    PeerStore(io::Error),
    /// A communication channel error.
    #[error("command channel disconnected")]
    Channel,
}

impl From<chan::SendError<Command>> for Error {
    fn from(_: chan::SendError<Command>) -> Self {
        Self::Channel
    }
}

impl From<chan::RecvError> for Error {
    fn from(_: chan::RecvError) -> Self {
        Self::Channel
    }
}
