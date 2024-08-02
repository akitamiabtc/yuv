use crate::{proof::p2wpkh::errors::P2WPKHWitnessParseError, PixelKeyError};

#[derive(Debug)]
pub enum BulletproofError {
    /// Error related to tweaking the pixel key
    PixelKeyError(PixelKeyError),

    /// Error parsing the witness
    WitnessParseError(P2WPKHWitnessParseError),

    /// The range proof is invalid
    InvalidRangeProof,

    /// Mismatch of provided script and the script in the witness
    ScriptMismatch,

    /// The public key in the witness does not match the public key in the script
    PublicKeyMismatch,

    LumaMismatch,
}

impl From<PixelKeyError> for BulletproofError {
    fn from(err: PixelKeyError) -> Self {
        Self::PixelKeyError(err)
    }
}

impl From<P2WPKHWitnessParseError> for BulletproofError {
    fn from(err: P2WPKHWitnessParseError) -> Self {
        Self::WitnessParseError(err)
    }
}

impl core::fmt::Display for BulletproofError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PixelKeyError(err) => write!(f, "PixelKeyError: {}", err),
            Self::WitnessParseError(err) => write!(f, "WitnessParseError: {}", err),
            Self::InvalidRangeProof => write!(f, "Invalid range proof"),
            Self::ScriptMismatch => write!(
                f,
                "Mismatch of provided script and the script in the witness"
            ),
            Self::PublicKeyMismatch => write!(
                f,
                "The public key in the witness does not match the public key in the script"
            ),
            Self::LumaMismatch => write!(f, "Luma doesn't match the proof and commitment"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BulletproofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PixelKeyError(err) => Some(err),
            Self::WitnessParseError(err) => Some(err),
            Self::InvalidRangeProof => None,
            Self::PublicKeyMismatch => None,
            Self::ScriptMismatch => None,
            Self::LumaMismatch => None,
        }
    }
}
