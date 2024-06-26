use alloc::boxed::Box;
use bitcoin::address::WitnessVersion;
use bitcoin::secp256k1::constants::SCHNORR_PUBLIC_KEY_SIZE;
use core::fmt::{self, Display};

use bitcoin::blockdata::{opcodes, script};
use bitcoin::secp256k1::scalar::OutOfRangeError;
use bitcoin::{ecdsa::Error as EcdsaSigError, secp256k1, PublicKey, ScriptBuf};

use crate::proof::htlc::LightningHtlcProofError;
#[cfg(feature = "bulletproof")]
use crate::proof::BulletproofError;
use crate::{CHROMA_SIZE, PIXEL_SIZE};

#[derive(Debug)]
pub enum PixelKeyError {
    ScalarOutOfRange(OutOfRangeError),
    Secp256k1(secp256k1::Error),
    PublicKeyError(bitcoin::key::Error),
    UncompressedKey,
}

impl Display for PixelKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PixelKeyError::ScalarOutOfRange(e) => write!(f, "Scalar out of range: {}", e),
            PixelKeyError::Secp256k1(e) => write!(f, "Secp256k1 error: {}", e),
            PixelKeyError::PublicKeyError(e) => write!(f, "Failed to decode public key: {}", e),
            PixelKeyError::UncompressedKey => write!(f, "Uncompressed key"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for PixelKeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PixelKeyError::ScalarOutOfRange(e) => Some(e),
            PixelKeyError::Secp256k1(e) => Some(e),
            PixelKeyError::PublicKeyError(e) => Some(e),
            PixelKeyError::UncompressedKey => None,
        }
    }
}

impl From<OutOfRangeError> for PixelKeyError {
    fn from(err: OutOfRangeError) -> Self {
        PixelKeyError::ScalarOutOfRange(err)
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

impl Display for LumaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LumaParseError::InvalidSize(size) => write!(f, "Invalid luma size: {}", size),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for LumaParseError {}

#[derive(Debug)]
pub enum PixelParseError {
    IncorrectSize(usize),
    InvalidLuma(LumaParseError),
    InvalidChroma(ChromaParseError),
}

impl Display for PixelParseError {
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
pub enum PixelProofError {
    EmptyPixelProofError(EmptyPixelProofError),
    SigPixelProofError(SigPixelProofError),
    MultisigPixelProofError(MultisigPixelProofError),
    LightningCommitmentProofError(LightningCommitmentProofError),
    #[cfg(feature = "bulletproof")]
    BulletproofError(BulletproofError),
    LightningHtlcError(LightningHtlcProofError),
}

impl Display for PixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PixelProofError::SigPixelProofError(e) => write!(f, "SigPixelProofError: {}", e),
            PixelProofError::MultisigPixelProofError(e) => {
                write!(f, "MultisigPixelProofError: {}", e)
            }
            PixelProofError::LightningCommitmentProofError(e) => {
                write!(f, "LightningCommitmentProofError: {}", e)
            }
            #[cfg(feature = "bulletproof")]
            PixelProofError::BulletproofError(e) => write!(f, "BulletproofError: {}", e),
            PixelProofError::LightningHtlcError(e) => write!(f, "LightningHtlcError: {}", e),
            PixelProofError::EmptyPixelProofError(e) => write!(f, "EmptyPixelProofError: {}", e),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for PixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PixelProofError::SigPixelProofError(e) => Some(e),
            PixelProofError::MultisigPixelProofError(e) => Some(e),
            PixelProofError::LightningCommitmentProofError(e) => Some(e),
            #[cfg(feature = "bulletproof")]
            PixelProofError::BulletproofError(e) => Some(e),
            PixelProofError::LightningHtlcError(e) => Some(e),
            PixelProofError::EmptyPixelProofError(e) => Some(e),
        }
    }
}

impl From<SigPixelProofError> for PixelProofError {
    fn from(err: SigPixelProofError) -> Self {
        PixelProofError::SigPixelProofError(err)
    }
}

impl From<EmptyPixelProofError> for PixelProofError {
    fn from(err: EmptyPixelProofError) -> Self {
        PixelProofError::EmptyPixelProofError(err)
    }
}

impl From<MultisigPixelProofError> for PixelProofError {
    fn from(err: MultisigPixelProofError) -> Self {
        PixelProofError::MultisigPixelProofError(err)
    }
}

impl From<LightningCommitmentProofError> for PixelProofError {
    fn from(err: LightningCommitmentProofError) -> Self {
        PixelProofError::LightningCommitmentProofError(err)
    }
}

#[cfg(feature = "bulletproof")]
impl From<BulletproofError> for PixelProofError {
    fn from(err: BulletproofError) -> Self {
        PixelProofError::BulletproofError(err)
    }
}

#[derive(Debug)]
pub enum EmptyPixelProofError {
    InvalidScript(ScriptBuf, ScriptBuf),
    P2wkhWitnessParseError(P2WPKHWitnessParseError),
    InvalidWitnessPublicKey(Box<PublicKey>, Box<PublicKey>),
    PixelKeyError(PixelKeyError),
}

impl Display for EmptyPixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmptyPixelProofError::InvalidScript(script, expected) => {
                write!(f, "Invalid script: {}, expected: {}", script, expected)
            }
            EmptyPixelProofError::InvalidWitnessPublicKey(witness, expected) => {
                write!(
                    f,
                    "Invalid public key in witness: {}, expected: {}",
                    witness, expected
                )
            }
            EmptyPixelProofError::P2wkhWitnessParseError(e) => {
                write!(f, "Failed to parse witness: {}", e)
            }
            EmptyPixelProofError::PixelKeyError(e) => {
                write!(f, "Failed to create pixel key: {}", e)
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for EmptyPixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EmptyPixelProofError::InvalidScript(_, _) => None,
            EmptyPixelProofError::InvalidWitnessPublicKey(_, _) => None,
            EmptyPixelProofError::P2wkhWitnessParseError(e) => Some(e),
            EmptyPixelProofError::PixelKeyError(e) => Some(e),
        }
    }
}

impl From<PixelKeyError> for EmptyPixelProofError {
    fn from(err: PixelKeyError) -> Self {
        EmptyPixelProofError::PixelKeyError(err)
    }
}

impl From<P2WPKHWitnessParseError> for EmptyPixelProofError {
    fn from(err: P2WPKHWitnessParseError) -> Self {
        EmptyPixelProofError::P2wkhWitnessParseError(err)
    }
}

#[derive(Debug)]
pub enum SigPixelProofError {
    PixelKeyError(PixelKeyError),
    InvalidSignature(EcdsaSigError),
    InvalidWitnessPublicKey(Box<PublicKey>, Box<PublicKey>),
    P2wkhWitnessParseError(P2WPKHWitnessParseError),
    InvalidScript(ScriptBuf, ScriptBuf),
}

impl Display for SigPixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SigPixelProofError::PixelKeyError(e) => write!(f, "Failed to create pixel key: {}", e),
            SigPixelProofError::InvalidSignature(e) => write!(f, "Invalid signature: {}", e),
            SigPixelProofError::InvalidWitnessPublicKey(witness, expected) => {
                write!(
                    f,
                    "Invalid public key in witness: {}, expected: {}",
                    witness, expected
                )
            }
            SigPixelProofError::P2wkhWitnessParseError(e) => {
                write!(f, "Failed to parse witness: {}", e)
            }
            SigPixelProofError::InvalidScript(script, expected) => {
                write!(f, "Invalid script: {}, expected: {}", script, expected)
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for SigPixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SigPixelProofError::PixelKeyError(e) => Some(e),
            SigPixelProofError::InvalidSignature(e) => Some(e),
            SigPixelProofError::InvalidWitnessPublicKey(_, _) => None,
            SigPixelProofError::P2wkhWitnessParseError(e) => Some(e),
            SigPixelProofError::InvalidScript(_, _) => None,
        }
    }
}

impl From<PixelKeyError> for SigPixelProofError {
    fn from(err: PixelKeyError) -> Self {
        SigPixelProofError::PixelKeyError(err)
    }
}

impl From<EcdsaSigError> for SigPixelProofError {
    fn from(err: EcdsaSigError) -> Self {
        SigPixelProofError::InvalidSignature(err)
    }
}

impl From<P2WPKHWitnessParseError> for SigPixelProofError {
    fn from(err: P2WPKHWitnessParseError) -> Self {
        SigPixelProofError::P2wkhWitnessParseError(err)
    }
}

#[derive(Debug)]
pub enum LightningCommitmentProofError {
    PixelKeyError(PixelKeyError),
    InvalidWitnessData(LightningCommitmentWitnessParseError),
    RedeemScriptMismatch {
        expected: ScriptBuf,
        found: ScriptBuf,
    },
    MismatchScriptPubkey {
        expected: ScriptBuf,
        found: ScriptBuf,
    },
}

impl Display for LightningCommitmentProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LightningCommitmentProofError::PixelKeyError(e) => {
                write!(f, "Failed to create pixel key: {}", e)
            }
            LightningCommitmentProofError::InvalidWitnessData(e) => {
                write!(f, "Invalid witness data: {}", e)
            }
            LightningCommitmentProofError::RedeemScriptMismatch { expected, found } => write!(
                f,
                "Redeem script mismatch expected {}, found {}",
                expected, found
            ),
            LightningCommitmentProofError::MismatchScriptPubkey { expected, found } => write!(
                f,
                "Mismatch script pubkey expected {}, found {}",
                expected, found
            ),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for LightningCommitmentProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LightningCommitmentProofError::PixelKeyError(e) => Some(e),
            LightningCommitmentProofError::InvalidWitnessData(e) => Some(e),
            LightningCommitmentProofError::RedeemScriptMismatch {
                expected: _,
                found: _,
            } => None,
            LightningCommitmentProofError::MismatchScriptPubkey {
                expected: _,
                found: _,
            } => None,
        }
    }
}

impl From<PixelKeyError> for LightningCommitmentProofError {
    fn from(err: PixelKeyError) -> Self {
        LightningCommitmentProofError::PixelKeyError(err)
    }
}

impl From<LightningCommitmentWitnessParseError> for LightningCommitmentProofError {
    fn from(err: LightningCommitmentWitnessParseError) -> Self {
        LightningCommitmentProofError::InvalidWitnessData(err)
    }
}

#[derive(Debug)]
pub enum LightningCommitmentWitnessParseError {
    WitnessStructure,
    Signature(EcdsaSigError),
    ScriptFormat(ToLocalScriptParseError),
}

impl Display for LightningCommitmentWitnessParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LightningCommitmentWitnessParseError::WitnessStructure => {
                write!(f, "Invalid witness structure")
            }
            LightningCommitmentWitnessParseError::Signature(e) => {
                write!(f, "Invalid signature: {}", e)
            }
            LightningCommitmentWitnessParseError::ScriptFormat(e) => {
                write!(f, "Invalid script format: {}", e)
            }
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for LightningCommitmentWitnessParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LightningCommitmentWitnessParseError::WitnessStructure => None,
            LightningCommitmentWitnessParseError::Signature(e) => Some(e),
            LightningCommitmentWitnessParseError::ScriptFormat(e) => Some(e),
        }
    }
}

impl From<EcdsaSigError> for LightningCommitmentWitnessParseError {
    fn from(err: EcdsaSigError) -> Self {
        LightningCommitmentWitnessParseError::Signature(err)
    }
}

impl From<ToLocalScriptParseError> for LightningCommitmentWitnessParseError {
    fn from(err: ToLocalScriptParseError) -> Self {
        LightningCommitmentWitnessParseError::ScriptFormat(err)
    }
}

#[derive(Debug)]
pub enum P2WPKHWitnessParseError {
    InvalidPublicKey(bitcoin::key::Error),
    InvalidSignature(EcdsaSigError),
    InvalidWitnessStructure,
}

impl Display for P2WPKHWitnessParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P2WPKHWitnessParseError::InvalidPublicKey(e) => write!(f, "Invalid public key: {}", e),
            P2WPKHWitnessParseError::InvalidSignature(e) => write!(f, "Invalid signature: {}", e),
            P2WPKHWitnessParseError::InvalidWitnessStructure => {
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
            P2WPKHWitnessParseError::InvalidWitnessStructure => None,
        }
    }
}

impl From<bitcoin::key::Error> for P2WPKHWitnessParseError {
    fn from(err: bitcoin::key::Error) -> Self {
        P2WPKHWitnessParseError::InvalidPublicKey(err)
    }
}

impl From<EcdsaSigError> for P2WPKHWitnessParseError {
    fn from(err: EcdsaSigError) -> Self {
        P2WPKHWitnessParseError::InvalidSignature(err)
    }
}

#[derive(Debug)]
pub enum ToLocalScriptParseError {
    Instruction {
        expected: opcodes::All,
        found: Option<opcodes::All>,
        index: usize,
    },
    Script(script::Error),
    PublicKey(secp256k1::Error),
    ToSelfDelay,
}

impl Display for ToLocalScriptParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToLocalScriptParseError::Instruction {
                expected,
                found,
                index,
            } => {
                write!(
                    f,
                    "Invalid instruction. Expected {:?}, found {:?} at index {}",
                    expected, found, index
                )
            }
            ToLocalScriptParseError::Script(e) => write!(f, "Invalid script: {}", e),
            ToLocalScriptParseError::PublicKey(e) => write!(f, "Invalid public key: {}", e),
            ToLocalScriptParseError::ToSelfDelay => write!(f, "Invalid `to_self_delay`"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for ToLocalScriptParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ToLocalScriptParseError::Instruction {
                expected: _,
                found: _,
                index: _,
            } => None,
            ToLocalScriptParseError::Script(e) => Some(e),
            ToLocalScriptParseError::PublicKey(e) => Some(e),
            ToLocalScriptParseError::ToSelfDelay => None,
        }
    }
}

impl From<script::Error> for ToLocalScriptParseError {
    fn from(err: script::Error) -> Self {
        ToLocalScriptParseError::Script(err)
    }
}

impl From<secp256k1::Error> for ToLocalScriptParseError {
    fn from(err: secp256k1::Error) -> Self {
        ToLocalScriptParseError::PublicKey(err)
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

impl Display for ChromaParseError {
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
pub enum MultisigPixelProofError {
    PixelKeyError(PixelKeyError),
    InvalidNumberOfInnerKeys(usize, usize),
    MultisigWitnessParseError(MultisigWitnessParseError),
    InvalidNumberOfSignatures(usize, usize),
    InvalidRedeemScript,
}

impl Display for MultisigPixelProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultisigPixelProofError::PixelKeyError(e) => {
                write!(f, "Failed to create pixel key: {}", e)
            }
            MultisigPixelProofError::InvalidNumberOfInnerKeys(num, expected) => write!(
                f,
                "Invalid number of inner keys: {}, expected: {}",
                num, expected
            ),
            MultisigPixelProofError::MultisigWitnessParseError(e) => {
                write!(f, "Failed to parse witness: {}", e)
            }
            MultisigPixelProofError::InvalidNumberOfSignatures(num, expected) => write!(
                f,
                "Invalid number of signatures: {}, expected: {}",
                num, expected
            ),
            MultisigPixelProofError::InvalidRedeemScript => write!(f, "Invalid redeem script"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for MultisigPixelProofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MultisigPixelProofError::PixelKeyError(e) => Some(e),
            MultisigPixelProofError::InvalidNumberOfInnerKeys(_, _) => None,
            MultisigPixelProofError::MultisigWitnessParseError(e) => Some(e),
            MultisigPixelProofError::InvalidNumberOfSignatures(_, _) => None,
            MultisigPixelProofError::InvalidRedeemScript => None,
        }
    }
}

impl From<PixelKeyError> for MultisigPixelProofError {
    fn from(err: PixelKeyError) -> Self {
        MultisigPixelProofError::PixelKeyError(err)
    }
}

impl From<MultisigWitnessParseError> for MultisigPixelProofError {
    fn from(err: MultisigWitnessParseError) -> Self {
        MultisigPixelProofError::MultisigWitnessParseError(err)
    }
}

#[derive(Debug)]
pub enum MultisigWitnessParseError {
    NoOp0,
    InvalidSignature(EcdsaSigError),
    NoRedeemScript,
}

impl Display for MultisigWitnessParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultisigWitnessParseError::NoOp0 => write!(f, "No OP_0 in witness"),
            MultisigWitnessParseError::InvalidSignature(e) => write!(f, "Invalid signature: {}", e),
            MultisigWitnessParseError::NoRedeemScript => write!(f, "No redeem script in witness"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for MultisigWitnessParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MultisigWitnessParseError::NoOp0 => None,
            MultisigWitnessParseError::InvalidSignature(e) => Some(e),
            MultisigWitnessParseError::NoRedeemScript => None,
        }
    }
}

impl From<EcdsaSigError> for MultisigWitnessParseError {
    fn from(err: EcdsaSigError) -> Self {
        MultisigWitnessParseError::InvalidSignature(err)
    }
}

impl From<LightningHtlcProofError> for PixelProofError {
    fn from(err: LightningHtlcProofError) -> Self {
        PixelProofError::LightningHtlcError(err)
    }
}
