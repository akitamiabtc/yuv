use core::fmt;

use crate::{YuvTransaction, YuvTxType};

use alloc::{string::String, vec::Vec};

use bitcoin::consensus::{Decodable, Encodable};
use core2::io::Cursor;
use hex::FromHexError;

impl YuvTransaction {
    pub fn hex(&self) -> String {
        let mut bytes = Vec::new();
        self.consensus_encode(&mut bytes).expect("Should encode");

        hex::encode(bytes)
    }

    pub fn from_hex(hex: String) -> Result<YuvTransaction, YuvTransactionParseError> {
        let bytes = hex::decode(hex)?;
        let mut reader = Cursor::new(bytes);

        YuvTransaction::consensus_decode(&mut reader)
            .map_err(|_err| YuvTransactionParseError::InvalidTx)
    }
}

impl YuvTxType {
    pub fn hex(&self) -> String {
        let mut bytes = Vec::new();
        self.consensus_encode(&mut bytes).expect("Should encode");

        hex::encode(bytes)
    }

    pub fn from_hex(hex: String) -> Result<YuvTxType, YuvTransactionParseError> {
        let bytes = hex::decode(hex)?;
        let mut reader = Cursor::new(bytes);

        YuvTxType::consensus_decode(&mut reader)
            .map_err(|_err| YuvTransactionParseError::InvalidProofs)
    }
}

/// Error that can occur when converting hex data in a `YuvTransaction` and vice versa.
#[derive(Debug)]
pub enum YuvTransactionParseError {
    /// Wrong raw transaction hex data.
    Hex(FromHexError),
    /// Hex data contains a malformed [YuvTransaction].
    InvalidTx,
    /// Hex data contains a malformed [YuvTxType].
    InvalidProofs,
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for YuvTransactionParseError {}

impl fmt::Display for YuvTransactionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YuvTransactionParseError::Hex(err) => write!(f, "Invalid hex data: {}", err),
            YuvTransactionParseError::InvalidTx => write!(f, "The transaction is malformed"),
            YuvTransactionParseError::InvalidProofs => {
                write!(f, "Transaction proofs are malformed")
            }
        }
    }
}

impl From<FromHexError> for YuvTransactionParseError {
    fn from(e: FromHexError) -> Self {
        Self::Hex(e)
    }
}
