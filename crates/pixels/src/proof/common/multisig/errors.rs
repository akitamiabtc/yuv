use alloc::fmt;
use bitcoin::secp256k1;

use crate::{proof::p2wsh::errors::P2WSHWitnessParseError, PixelKeyError};

#[derive(Debug)]
pub enum MultisigPixelProofError {
    /// The witness does not contain a valid pixel key.
    PixelKeyError(PixelKeyError),

    /// The number of inner keys in the witness does not match the number of
    /// keys in the inner script.
    InvalidNumberOfInnerKeys(usize, usize),

    /// Failed to parse witness
    WitnessParseError(P2WSHWitnessParseError),

    /// The number of signatures in the witness does not match the number of
    /// keys in the inner script.
    InvalidNumberOfSignatures(usize, usize),

    /// Mismatch of redeem scripts in witness and inner script
    RedeemScriptMismatch,
}

impl From<PixelKeyError> for MultisigPixelProofError {
    fn from(e: PixelKeyError) -> Self {
        MultisigPixelProofError::PixelKeyError(e)
    }
}

impl From<P2WSHWitnessParseError> for MultisigPixelProofError {
    fn from(e: P2WSHWitnessParseError) -> Self {
        MultisigPixelProofError::WitnessParseError(e)
    }
}

impl fmt::Display for MultisigPixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MultisigPixelProofError::PixelKeyError(e) => write!(f, "Pixel key error: {}", e),
            MultisigPixelProofError::InvalidNumberOfInnerKeys(expected, actual) => write!(
                f,
                "Invalid number of inner keys: expected {}, got {}",
                expected, actual
            ),
            MultisigPixelProofError::WitnessParseError(e) => {
                write!(f, "Witness parse error: {}", e)
            }
            MultisigPixelProofError::InvalidNumberOfSignatures(expected, actual) => write!(
                f,
                "Invalid number of signatures: expected {}, got {}",
                expected, actual
            ),
            MultisigPixelProofError::RedeemScriptMismatch => {
                write!(f, "Mismatch of redeem scripts in witness and inner script")
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for MultisigPixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MultisigPixelProofError::PixelKeyError(e) => Some(e),
            MultisigPixelProofError::InvalidNumberOfInnerKeys(_, _) => None,
            MultisigPixelProofError::InvalidNumberOfSignatures(_, _) => None,
            MultisigPixelProofError::WitnessParseError(e) => Some(e),
            MultisigPixelProofError::RedeemScriptMismatch => None,
        }
    }
}

#[derive(Debug)]
pub enum MultisigScriptError {
    /// Invalid structure of multisig script
    InvalidScript,

    /// Failed to parse pubkey from p2wsh redeem script
    ParsePubkeyError(secp256k1::Error),
}

impl From<secp256k1::Error> for MultisigScriptError {
    fn from(e: secp256k1::Error) -> Self {
        MultisigScriptError::ParsePubkeyError(e)
    }
}

impl fmt::Display for MultisigScriptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MultisigScriptError::InvalidScript => write!(f, "Invalid multisig script"),
            MultisigScriptError::ParsePubkeyError(e) => {
                write!(f, "Failed to parse pubkey from p2wsh redeem script: {}", e)
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for MultisigScriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MultisigScriptError::InvalidScript => None,
            MultisigScriptError::ParsePubkeyError(e) => Some(e),
        }
    }
}
