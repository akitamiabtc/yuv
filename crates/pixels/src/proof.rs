use alloc::boxed::Box;
use core::hash::Hash;

use bitcoin::secp256k1::PublicKey;
use bitcoin::{secp256k1, ScriptBuf, TxIn, TxOut};

use crate::errors::PixelProofError;
use crate::{LightningCommitmentProof, MultisigPixelProof, P2WPKHProof, Pixel};

use self::common::lightning::htlc::LightningHtlcProof;
use self::empty::EmptyPixelProof;
use self::p2wpkh::SigPixelProof;
use self::p2wsh::P2WSHProof;

#[cfg(feature = "bulletproof")]
pub mod bulletproof;
pub mod common;
pub mod empty;
pub mod p2wpkh;
pub mod p2wsh;

/// The proof of ownership that user brings to check and attach particular transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", content = "data"))]
pub enum PixelProof {
    /// The proof of ownership of the satoshis only output.
    ///
    /// This type of proof doesn't hold a pixel.
    EmptyPixel(EmptyPixelProof),

    /// The proof of ownership with single signature.
    Sig(SigPixelProof),

    /// Pixel proof for multisignature transaction that uses P2WSH script.
    Multisig(MultisigPixelProof),

    /// Proof for transaction for Lightning network protocol commitment
    /// transaction.
    ///
    /// TODO: rename to `LightningCommitment`.
    Lightning(LightningCommitmentProof),

    /// Proof for spending lightning HTLC output at force-close.
    LightningHtlc(LightningHtlcProof),

    /// The proof for arbitary P2WSH address script.
    P2WSH(Box<p2wsh::P2WSHProof>),

    /// The bulletproof with a corresponsing Pedersen commitment
    #[cfg(feature = "bulletproof")]
    Bulletproof(alloc::boxed::Box<bulletproof::Bulletproof>),
}

impl PixelProof {
    #[inline]
    pub fn pixel(&self) -> Pixel {
        match self {
            Self::Sig(proof) => proof.pixel,
            Self::P2WSH(proof) => proof.pixel,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.pixel,
            Self::EmptyPixel(_) => Pixel::empty(),
            Self::Multisig(proof) => proof.pixel,
            Self::Lightning(proof) => proof.pixel,
            Self::LightningHtlc(proof) => proof.pixel,
        }
    }

    pub fn p2wsh(
        pixel: impl Into<Pixel>,
        inner_key: secp256k1::PublicKey,
        script: impl Into<ScriptBuf>,
    ) -> Self {
        Self::P2WSH(Box::new(p2wsh::P2WSHProof::new(
            pixel.into(),
            inner_key,
            script.into(),
        )))
    }

    pub fn sig(pixel: impl Into<Pixel>, inner_key: secp256k1::PublicKey) -> Self {
        Self::Sig(P2WPKHProof::new(pixel.into(), inner_key))
    }

    pub fn empty(pubkey: impl Into<PublicKey>) -> Self {
        Self::Sig(p2wpkh::P2WPKHProof::empty(pubkey))
    }

    pub fn amount(&self) -> u128 {
        self.pixel().luma.amount
    }

    /// Returns `true` if the proof amount is zero
    pub fn is_zero_amount(&self) -> bool {
        self.amount() == 0
    }

    #[cfg(feature = "bulletproof")]
    pub fn bulletproof(bulletproof: bulletproof::Bulletproof) -> Self {
        Self::Bulletproof(alloc::boxed::Box::new(bulletproof))
    }

    #[cfg(feature = "bulletproof")]
    pub fn is_bulletproof(&self) -> bool {
        matches!(self, Self::Bulletproof(_))
    }

    pub fn is_burn(&self) -> bool {
        let PixelProof::Sig(inner) = self else {
            return false;
        };

        inner.inner_key == crate::ZERO_PUBLIC_KEY.inner
    }

    pub fn is_empty_pixelproof(&self) -> bool {
        matches!(self, Self::EmptyPixel(_))
    }

    #[cfg(feature = "bulletproof")]
    pub fn get_bulletproof(&self) -> Option<&bulletproof::Bulletproof> {
        match self {
            Self::Bulletproof(bulletproof) => Some(bulletproof),
            _ => None,
        }
    }
}

/// Trait for proof that can be checked by transaction input or output.
pub trait CheckableProof {
    /// Check the proof by transaction with fallback to `false` on error.
    fn check_by_input(&self, txin: &TxIn) -> bool {
        self.checked_check_by_input(txin).is_ok()
    }

    /// Check the proof by transaction with fallback to `false` on error.
    fn check_by_output(&self, txout: &TxOut) -> bool {
        self.checked_check_by_output(txout).is_ok()
    }

    /// Error type that can be returned by check methods.
    type Error;

    /// Check the proof by transaction input.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error>;

    /// Check the proof by transaction output.
    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error>;
}

impl CheckableProof for PixelProof {
    type Error = PixelProofError;

    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        match self {
            Self::Sig(proof) => proof.checked_check_by_input(txin)?,
            Self::P2WSH(proof) => proof.checked_check_by_input(txin)?,
            Self::EmptyPixel(proof) => proof.checked_check_by_input(txin)?,
            Self::Multisig(proof) => proof.checked_check_by_input(txin)?,
            Self::Lightning(proof) => proof.checked_check_by_input(txin)?,
            Self::LightningHtlc(proof) => proof.checked_check_by_input(txin)?,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.checked_check_by_input(txin)?,
        };

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        match self {
            Self::Sig(proof) => proof.checked_check_by_output(txout)?,
            Self::EmptyPixel(proof) => proof.checked_check_by_output(txout)?,
            Self::Multisig(proof) => proof.checked_check_by_output(txout)?,
            Self::Lightning(proof) => proof.checked_check_by_output(txout)?,
            Self::LightningHtlc(proof) => proof.checked_check_by_output(txout)?,
            Self::P2WSH(proof) => proof.checked_check_by_output(txout)?,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.checked_check_by_output(txout)?,
        };

        Ok(())
    }
}

impl From<P2WPKHProof> for PixelProof {
    fn from(proof: P2WPKHProof) -> Self {
        Self::Sig(proof)
    }
}

impl<T> From<T> for PixelProof
where
    T: Into<P2WSHProof>,
{
    fn from(proof: T) -> Self {
        Self::P2WSH(Box::new(proof.into()))
    }
}
