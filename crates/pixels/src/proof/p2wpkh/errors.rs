use alloc::fmt;
use bitcoin::{ecdsa, secp256k1};

use crate::PixelKeyError;

#[derive(Debug)]
pub enum P2WPKHWitnessParseError {
    /// Invalid public key in the witness
    InvalidPublicKey(secp256k1::Error),

    /// Invalid signature in the witness
    InvalidSignature(ecdsa::Error),

    /// Stack in witness has invalid length
    StackLengthMismatch,
}

impl fmt::Display for P2WPKHWitnessParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P2WPKHWitnessParseError::InvalidPublicKey(e) => write!(f, "Invalid public key: {}", e),
            P2WPKHWitnessParseError::InvalidSignature(e) => write!(f, "Invalid signature: {}", e),
            P2WPKHWitnessParseError::StackLengthMismatch => {
                write!(f, "Invalid witness structure")
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for P2WPKHWitnessParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            P2WPKHWitnessParseError::InvalidPublicKey(e) => Some(e),
            P2WPKHWitnessParseError::InvalidSignature(e) => Some(e),
            P2WPKHWitnessParseError::StackLengthMismatch => None,
        }
    }
}

impl From<secp256k1::Error> for P2WPKHWitnessParseError {
    fn from(err: secp256k1::Error) -> Self {
        P2WPKHWitnessParseError::InvalidPublicKey(err)
    }
}

impl From<ecdsa::Error> for P2WPKHWitnessParseError {
    fn from(err: ecdsa::Error) -> Self {
        P2WPKHWitnessParseError::InvalidSignature(err)
    }
}

#[derive(Debug)]
pub enum P2WPKHProofError {
    /// Error related to tweaking the public key
    PixelKeyError(PixelKeyError),

    /// Failed to parse the witness data
    WitnessParseError(P2WPKHWitnessParseError),

    /// Provided and expected script pubkeys mismatch
    ScriptPubKeyMismatch,

    /// Mismatch of public keys in witness and provided public key.
    PublicKeyMismatch,
}

impl fmt::Display for P2WPKHProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P2WPKHProofError::PixelKeyError(e) => write!(f, "Failed to create pixel key: {}", e),
            P2WPKHProofError::WitnessParseError(e) => {
                write!(f, "Failed to parse witness: {}", e)
            }
            P2WPKHProofError::ScriptPubKeyMismatch => write!(f, "Script pubkey mismatch"),
            P2WPKHProofError::PublicKeyMismatch => write!(f, "Public key mismatch"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for P2WPKHProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            P2WPKHProofError::PixelKeyError(e) => Some(e),
            P2WPKHProofError::WitnessParseError(e) => Some(e),
            P2WPKHProofError::ScriptPubKeyMismatch => None,
            P2WPKHProofError::PublicKeyMismatch => None,
        }
    }
}

impl From<PixelKeyError> for P2WPKHProofError {
    fn from(err: PixelKeyError) -> Self {
        P2WPKHProofError::PixelKeyError(err)
    }
}

impl From<P2WPKHWitnessParseError> for P2WPKHProofError {
    fn from(err: P2WPKHWitnessParseError) -> Self {
        P2WPKHProofError::WitnessParseError(err)
    }
}
