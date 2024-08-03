use bitcoin::address::WitnessVersion;
use bitcoin::secp256k1::constants::SCHNORR_PUBLIC_KEY_SIZE;
use core::fmt;

use bitcoin::secp256k1;

#[cfg(feature = "bulletproof")]
use crate::proof::bulletproof::errors::BulletproofError;
use crate::proof::common::lightning::commitment::errors::LightningCommitmentProofError;
use crate::proof::common::lightning::htlc::LightningHtlcProofError;
use crate::proof::common::multisig::errors::MultisigPixelProofError;
use crate::proof::p2wpkh::errors::P2WPKHProofError;
use crate::proof::p2wsh::errors::P2WSHProofError;
use crate::{CHROMA_SIZE, PIXEL_SIZE};

#[derive(Debug)]
pub enum PixelKeyError {
    Secp256k1(secp256k1::Error),

    /// Error during operations with key.
    PublicKeyError(bitcoin::key::Error),

    /// Scalar created from pixel hash is out of range.
    /// NOTE: usually this should never happen, but it's better to handle this case.
    PixelHashOutOfRange,

    /// Uncompressed public key used when only compressed one is supported
    UncompressedKey,
}

impl fmt::Display for PixelKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PixelKeyError::Secp256k1(e) => write!(f, "Secp256k1 error: {}", e),
            PixelKeyError::PublicKeyError(e) => write!(f, "Failed to decode public key: {}", e),
            PixelKeyError::PixelHashOutOfRange => write!(f, "Pixel hash is out of range"),
            PixelKeyError::UncompressedKey => write!(f, "Uncompressed key"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for PixelKeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PixelKeyError::Secp256k1(e) => Some(e),
            PixelKeyError::PublicKeyError(e) => Some(e),
            PixelKeyError::PixelHashOutOfRange => None,
            PixelKeyError::UncompressedKey => None,
        }
    }
}

impl From<secp256k1::Error> for PixelKeyError {
    fn from(err: secp256k1::Error) -> Self {
        PixelKeyError::Secp256k1(err)
    }
}

impl From<bitcoin::key::Error> for PixelKeyError {
    fn from(err: bitcoin::key::Error) -> Self {
        PixelKeyError::PublicKeyError(err)
    }
}

#[derive(Debug)]
pub enum LumaParseError {
    InvalidSize(usize),
}

impl fmt::Display for LumaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LumaParseError::InvalidSize(size) => write!(f, "Invalid luma size: {}", size),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for LumaParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LumaParseError::InvalidSize(_) => None,
        }
    }
}

#[derive(Debug)]
pub enum PixelParseError {
    IncorrectSize(usize),
    InvalidLuma(LumaParseError),
    InvalidChroma(ChromaParseError),
}

impl fmt::Display for PixelParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PixelParseError::IncorrectSize(size) => {
                write!(f, "Invalid pixel size: {}, required: {}", size, PIXEL_SIZE)
            }
            PixelParseError::InvalidLuma(e) => write!(f, "Invalid luma: {}", e),
            PixelParseError::InvalidChroma(e) => write!(f, "Invalid chroma: {}", e),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for PixelParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PixelParseError::IncorrectSize(_) => None,
            PixelParseError::InvalidLuma(e) => Some(e),
            PixelParseError::InvalidChroma(e) => Some(e),
        }
    }
}

impl From<LumaParseError> for PixelParseError {
    fn from(err: LumaParseError) -> Self {
        PixelParseError::InvalidLuma(err)
    }
}

impl From<ChromaParseError> for PixelParseError {
    fn from(err: ChromaParseError) -> Self {
        PixelParseError::InvalidChroma(err)
    }
}

#[derive(Debug)]
pub enum ChromaParseError {
    InvalidSize(usize),
    InvalidXOnlyKey(secp256k1::Error),
    InvalidAddressType,
    InvalidWitnessProgramVersion(WitnessVersion),
    InvalidWitnessProgramLength(usize),
}

impl fmt::Display for ChromaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChromaParseError::InvalidSize(size) => {
                write!(f, "Invalid bytes size: {}, required: {}", size, CHROMA_SIZE)
            }
            ChromaParseError::InvalidXOnlyKey(e) => {
                write!(f, "Invalid x only public key structure: {}", e)
            }
            ChromaParseError::InvalidAddressType => {
                write!(f, "Invalid address type")
            }
            ChromaParseError::InvalidWitnessProgramVersion(version) => {
                write!(f, "Invalid witness program version: {}", version)
            }
            ChromaParseError::InvalidWitnessProgramLength(length) => {
                write!(
                    f,
                    "Invalid witness program length: {}, expected {}",
                    length, SCHNORR_PUBLIC_KEY_SIZE
                )
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for ChromaParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ChromaParseError::InvalidSize(_) => None,
            ChromaParseError::InvalidXOnlyKey(e) => Some(e),
            ChromaParseError::InvalidAddressType => None,
            ChromaParseError::InvalidWitnessProgramVersion(_) => None,
            ChromaParseError::InvalidWitnessProgramLength(_) => None,
        }
    }
}

impl From<secp256k1::Error> for ChromaParseError {
    fn from(err: secp256k1::Error) -> Self {
        ChromaParseError::InvalidXOnlyKey(err)
    }
}

#[derive(Debug)]
pub enum PixelProofError {
    /// P2WPKH error
    P2WPKH(P2WPKHProofError),

    /// P2WSH error
    P2WSH(P2WSHProofError),

    /// EmptyPixelProof
    EmptyPixel(P2WPKHProofError),

    Multisig(MultisigPixelProofError),

    Lightning(LightningCommitmentProofError),

    LightningHtlc(LightningHtlcProofError),

    #[cfg(feature = "bulletproof")]
    /// Bulletproof error
    Bulletproof(BulletproofError),
}

impl From<MultisigPixelProofError> for PixelProofError {
    fn from(v: MultisigPixelProofError) -> Self {
        Self::Multisig(v)
    }
}

impl From<LightningHtlcProofError> for PixelProofError {
    fn from(v: LightningHtlcProofError) -> Self {
        Self::LightningHtlc(v)
    }
}

impl From<LightningCommitmentProofError> for PixelProofError {
    fn from(v: LightningCommitmentProofError) -> Self {
        Self::Lightning(v)
    }
}

impl fmt::Display for PixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PixelProofError::P2WPKH(e) => write!(f, "P2WPKH: {}", e),
            PixelProofError::P2WSH(e) => write!(f, "P2WSH: {}", e),
            PixelProofError::EmptyPixel(e) => write!(f, "EmptyPixel: {}", e),
            PixelProofError::Multisig(e) => write!(f, "Multisig: {}", e),
            PixelProofError::Lightning(e) => write!(f, "Lightning: {}", e),
            PixelProofError::LightningHtlc(e) => write!(f, "LightningHtlc: {}", e),
            #[cfg(feature = "bulletproof")]
            PixelProofError::Bulletproof(e) => write!(f, "Bulletproof: {}", e),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for PixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PixelProofError::P2WPKH(e) => Some(e),
            PixelProofError::P2WSH(e) => Some(e),
            PixelProofError::EmptyPixel(e) => Some(e),
            PixelProofError::Multisig(e) => Some(e),
            PixelProofError::Lightning(e) => Some(e),
            PixelProofError::LightningHtlc(e) => Some(e),
            #[cfg(feature = "bulletproof")]
            PixelProofError::Bulletproof(e) => Some(e),
        }
    }
}

impl From<P2WPKHProofError> for PixelProofError {
    fn from(err: P2WPKHProofError) -> Self {
        PixelProofError::P2WPKH(err)
    }
}

impl From<P2WSHProofError> for PixelProofError {
    fn from(err: P2WSHProofError) -> Self {
        PixelProofError::P2WSH(err)
    }
}

#[cfg(feature = "bulletproof")]
impl From<BulletproofError> for PixelProofError {
    fn from(err: BulletproofError) -> Self {
        PixelProofError::Bulletproof(err)
    }
}
