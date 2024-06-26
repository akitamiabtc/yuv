//! Message stream utilities.
use std::io;

use bitcoin::consensus::{deserialize_partial, encode::Error};
use yuv_types::messages::p2p::RawNetworkMessage;

/// Message stream decoder.
///
/// Used to for example_client turn a byte stream into network messages.
#[derive(Debug)]
pub struct Decoder {
    unparsed: Vec<u8>,
}

impl Decoder {
    /// Create a new stream decoder.
    pub fn new(capacity: usize) -> Self {
        Self {
            unparsed: Vec::with_capacity(capacity),
        }
    }

    /// Input bytes into the decoder.
    pub fn input(&mut self, bytes: &[u8]) {
        self.unparsed.extend_from_slice(bytes);
    }

    /// Decode and return the next message. Returns [`None`] if nothing was decoded.
    pub fn decode_next(&mut self) -> Result<Option<RawNetworkMessage>, Error> {
        match deserialize_partial(self.unparsed.as_slice()) {
            Ok((msg, index)) => {
                self.unparsed.drain(..index);
                Ok(Some(msg))
            }

            Err(Error::Io(ref err)) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(err) => Err(err),
        }
    }
}
