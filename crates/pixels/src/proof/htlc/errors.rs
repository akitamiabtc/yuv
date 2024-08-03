use alloc::fmt;
use bitcoin::{
    hashes::{sha256, Hash},
    WScriptHash,
};

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum LightningHtlcProofError {
    /// Type of the script at check wasn't P2WSH, so it can't be HTLC script.
    InvalidScriptKind,
    /// Witness program of the script is invalid in some way.
    ///
    /// For example, the witness programs is empty.
    InvalidWitnessProgramStructure,
    /// Got invalid witness program hash size from output's `script_pubkey`.
    InvalidWScriptHashSize(usize),
    /// Received invaild hash of the witness program that do no match to
    /// the expected one.
    ScriptHashMismatch {
        got: WScriptHash,
        expected: WScriptHash,
    },
}

impl fmt::Display for LightningHtlcProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScriptKind => write!(f, "Invalid script kind, should be P2WSH"),
            Self::InvalidWitnessProgramStructure => write!(f, "Invalid witness program structure"),
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
