use bitcoin::{ecdsa::Signature, secp256k1::PublicKey, TxIn, TxOut};

use crate::{CheckableProof, Pixel, PixelKey, PixelKeyError};

use self::{errors::P2WPKHProofError, witness::P2WPKHWitness};

#[cfg(feature = "consensus")]
pub mod consensus;
pub mod errors;
pub mod witness;

pub type SigPixelProof = P2WPKHProof;

/// The proof of ownership with single signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct P2WPKHProof {
    /// Pixel that proof verifies.
    pub pixel: Pixel,
    /// Key of current owner of the pixel.
    pub inner_key: PublicKey,
}

impl P2WPKHProof {
    pub fn empty(pubkey: impl Into<PublicKey>) -> Self {
        Self::new(Pixel::empty(), pubkey.into())
    }

    pub const fn new(pixel: Pixel, inner_key: PublicKey) -> Self {
        Self { pixel, inner_key }
    }

    /// Check proof by parsed witness data.
    pub(crate) fn check_by_parsed_witness_data(
        &self,
        _signature: &Signature,
        pubkey: &PublicKey,
    ) -> Result<(), P2WPKHProofError> {
        let pixel_key = PixelKey::new(self.pixel, &self.inner_key)?;

        if *pixel_key != *pubkey {
            return Err(P2WPKHProofError::PublicKeyMismatch);
        }

        // TODO: verify signature.

        Ok(())
    }
}

impl CheckableProof for P2WPKHProof {
    type Error = P2WPKHProofError;

    /// Get from input witness signature and public key and check that public
    /// key is equal to the tweaked one from proof.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = P2WPKHWitness::from_witness(&txin.witness)?;

        self.check_by_parsed_witness_data(&data.signature, &data.pubkey)?;

        Ok(())
    }

    /// Get from transaction output `script_pubkey` and create P2WPKH script
    /// from tweaked public key from proof and compare it with `script_pubkey`.
    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        let pixel_key = PixelKey::new(self.pixel, &self.inner_key)?;

        let expected_script_pubkey = pixel_key
            .to_p2wpkh()
            .ok_or(PixelKeyError::UncompressedKey)?;

        if txout.script_pubkey != expected_script_pubkey {
            return Err(P2WPKHProofError::ScriptPubKeyMismatch);
        }

        Ok(())
    }
}
