//! Peer-to-peer protocol errors.

use std::fmt::Debug;
use std::io;

use thiserror::Error;

/// An error occuring in peer-to-peer networking code.
#[derive(Error, Debug)]
pub enum Error {
    /// An I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),

    /// A channel send or receive error.
    #[error("channel error: {0}")]
    Channel(Box<dyn std::error::Error + Send + Sync + 'static>),
}
