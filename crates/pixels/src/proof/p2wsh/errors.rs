use alloc::{fmt, string::String};

#[derive(Debug)]
pub enum P2WSHProofError {
    /// Pubkey in P2WSH address script is missing
    MissingPubkey,

    /// Missmatch between pubkey in P2WSH address script and tweaked pubkey from
    /// proof.
    PubkeyMismatch,

    /// Missmatch between provided output address script and the one from proof.
    OutputScriptMismatch,

    /// Invalid witness in transaction input.
    InvalidWitness(P2WSHWitnessParseError),

    /// Mismatch between provided redeem script and the one from proof.
    RedeemScriptMismatch,
}

impl From<P2WSHWitnessParseError> for P2WSHProofError {
    fn from(err: P2WSHWitnessParseError) -> Self {
        P2WSHProofError::InvalidWitness(err)
    }
}

impl fmt::Display for P2WSHProofError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            P2WSHProofError::MissingPubkey => {
                write!(f, "Pubkey in P2WSH address script is missing")
            }
            P2WSHProofError::PubkeyMismatch => {
                write!(
                    f,
                    "Pubkey in P2WSH address script and tweaked pubkey mismatch"
                )
            }
            P2WSHProofError::OutputScriptMismatch => {
                write!(f, "Output script mismatch")
            }
            P2WSHProofError::InvalidWitness(err) => {
                write!(f, "Invalid witness: {}", err)
            }
            P2WSHProofError::RedeemScriptMismatch => {
                write!(f, "Redeem script mismatch")
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for P2WSHProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            P2WSHProofError::MissingPubkey => None,
            P2WSHProofError::PubkeyMismatch => None,
            P2WSHProofError::OutputScriptMismatch => None,
            P2WSHProofError::InvalidWitness(e) => Some(e),
            P2WSHProofError::RedeemScriptMismatch => None,
        }
    }
}

#[derive(Debug)]
pub enum P2WSHWitnessParseError {
    /// The witness is empty.
    EmptyWitness,

    /// Failed to parse redeem script
    Script(bitcoin::consensus::encode::Error),

    /// For custom parsing errors.
    Custom(String),
}

impl From<bitcoin::consensus::encode::Error> for P2WSHWitnessParseError {
    fn from(err: bitcoin::consensus::encode::Error) -> Self {
        P2WSHWitnessParseError::Script(err)
    }
}

impl fmt::Display for P2WSHWitnessParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            P2WSHWitnessParseError::EmptyWitness => {
                write!(f, "The witness is empty")
            }
            P2WSHWitnessParseError::Script(err) => {
                write!(f, "Failed to parse redeem script: {}", err)
            }
            P2WSHWitnessParseError::Custom(msg) => {
                write!(f, "{}", msg)
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for P2WSHWitnessParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            P2WSHWitnessParseError::EmptyWitness => None,
            P2WSHWitnessParseError::Script(e) => Some(e),
            P2WSHWitnessParseError::Custom(_) => None,
        }
    }
}
