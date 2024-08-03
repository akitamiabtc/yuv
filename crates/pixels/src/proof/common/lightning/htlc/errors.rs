use alloc::fmt;
use bitcoin::{
    blockdata::script,
    hashes::{self, sha256, Hash},
    secp256k1, WScriptHash,
};

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum LightningHtlcProofError {
    /// Type of the script at check wasn't P2WSH, so it can't be HTLC script.
    InvalidScriptKind,
    /// Witness program of the script is invalid in some way.
    ///
    /// For example, the witness programs is empty.
    InvalidWitnessProgramStructure,
    /// Script iteration error
    Script(script::Error),
    /// Invalid script structure
    InvalidScriptStructure,
    /// Failed to parse hash from the script
    Hash(hashes::Error),
    /// Failed to parse public key from the script
    PublicKey(secp256k1::Error),
    /// Got invalid witness program hash size from output's `script_pubkey`.
    InvalidWScriptHashSize(usize),
    /// Received invaild hash of the witness program that do no match to
    /// the expected one.
    ScriptHashMismatch {
        got: WScriptHash,
        expected: WScriptHash,
    },
}

impl From<script::Error> for LightningHtlcProofError {
    fn from(err: script::Error) -> Self {
        Self::Script(err)
    }
}

impl From<hashes::Error> for LightningHtlcProofError {
    fn from(err: hashes::Error) -> Self {
        Self::Hash(err)
    }
}

impl From<secp256k1::Error> for LightningHtlcProofError {
    fn from(err: secp256k1::Error) -> Self {
        Self::PublicKey(err)
    }
}

impl fmt::Display for LightningHtlcProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScriptKind => write!(f, "Invalid script kind, should be P2WSH"),
            Self::InvalidWitnessProgramStructure => write!(f, "Invalid witness program structure"),
            Self::Script(err) => write!(f, "Script error: {}", err),
            Self::Hash(err) => write!(f, "Hash error: {}", err),
            Self::PublicKey(err) => write!(f, "Public key error: {}", err),
            Self::InvalidScriptStructure => write!(f, "Invalid script structure"),
            Self::ScriptHashMismatch { got, expected } => write!(
                f,
                "Script hash mismatch got: {}, received: {}",
                got, expected
            ),
            Self::InvalidWScriptHashSize(got_size) => write!(
                f,
                "Invalid script wscript hash size: {} expected: {}",
                got_size,
                sha256::Hash::LEN,
            ),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for LightningHtlcProofError {}
