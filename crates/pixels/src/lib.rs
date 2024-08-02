#![doc = include_str!("../README.md")]
#![cfg_attr(feature = "no-std", no_std)]

extern crate alloc;

#[cfg(feature = "bulletproof")]
pub use bulletproof::{
    generate as generate_bulletproof, k256, verify as verify_bulletproof, RangeProof,
};
pub use errors::{
    ChromaParseError, LightningCommitmentProofError, LightningCommitmentWitnessParseError,
    LumaParseError, MultisigPixelProofError, MultisigWitnessParseError, P2WPKHWitnessParseError,
    PixelKeyError, PixelParseError, PixelProofError, SigPixelProofError, ToLocalScriptParseError,
};
pub use hash::PixelHash;
pub use keys::{PixelKey, PixelPrivateKey, ToEvenPublicKey};
pub use pixel::{
    Chroma, Luma, Pixel, BLINDING_FACTOR_SIZE, CHROMA_SIZE, LUMA_SIZE, PIXEL_SIZE, ZERO_PUBLIC_KEY,
};
pub use proof::{
    htlc::{HtlcScriptKind, LightningHtlcData, LightningHtlcProof, LightningHtlcScript},
    CheckableProof, EmptyPixelProof, LightningCommitmentProof, LightningCommitmentWitness,
    MultisigPixelProof, MultisigWintessData, P2WPKHWitnessData, PixelProof, SigPixelProof,
};
#[cfg(feature = "bulletproof")]
pub use proof::{Bulletproof, BulletproofError};
pub use tweakable::Tweakable;

#[cfg(not(any(feature = "std", feature = "no-std")))]
compile_error!("at least one of the `std` or `no-std` features must be enabled");

#[cfg(feature = "consensus")]
mod consensus;

#[cfg(feature = "bulletproof")]
pub mod bulletproof_signing;

mod errors;
mod hash;
mod keys;
mod pixel;
mod proof;
mod script;
mod tweakable;

#[cfg(all(feature = "serde", feature = "bulletproof"))]
pub(crate) struct HexVisitor;

#[cfg(all(feature = "serde", feature = "bulletproof"))]
impl<'de> serde::de::Visitor<'de> for HexVisitor {
    type Value = alloc::string::String;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a hex string")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(s.into())
    }
}
