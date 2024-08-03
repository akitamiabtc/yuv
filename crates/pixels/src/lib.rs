#![doc = include_str!("../README.md")]
#![cfg_attr(feature = "no-std", no_std)]

extern crate alloc;

#[cfg(feature = "bulletproof")]
pub use bulletproof::{
    generate as generate_bulletproof, k256, verify as verify_bulletproof, RangeProof,
};
pub use errors::{
    ChromaParseError, LumaParseError, PixelKeyError, PixelParseError, PixelProofError,
};
pub use hash::PixelHash;
pub use keys::{PixelKey, PixelPrivateKey, ToEvenPublicKey};
pub use pixel::{
    Chroma, Luma, Pixel, BLINDING_FACTOR_SIZE, CHROMA_SIZE, LUMA_SIZE, PIXEL_SIZE, ZERO_PUBLIC_KEY,
};
#[cfg(feature = "bulletproof")]
pub use proof::bulletproof::{
    errors::BulletproofError, signing as bulletproof_signing, Bulletproof,
};
pub use proof::common::lightning::commitment::{
    witness::{LightningCommitmentWitness, LightningCommitmentWitnessStack},
    LightningCommitmentProof,
};
pub use proof::common::lightning::htlc::{
    HtlcScriptKind, LightningHtlcData, LightningHtlcProof, LightningHtlcScript,
};
pub use proof::common::multisig::{witness::MultisigWitness, MultisigPixelProof};
pub use proof::empty::EmptyPixelProof;
pub use proof::p2wpkh::{witness::P2WPKHWitness, P2WPKHProof, SigPixelProof};
pub use proof::p2wsh::{witness::P2WSHWitness, P2WSHProof};
pub use proof::{CheckableProof, PixelProof};
pub use tweakable::Tweakable;

#[cfg(not(any(feature = "std", feature = "no-std")))]
compile_error!("at least one of the `std` or `no-std` features must be enabled");

#[cfg(feature = "consensus")]
pub mod consensus;

mod errors;
mod hash;
mod keys;
mod pixel;
mod proof;
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
