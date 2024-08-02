use bitcoin::{secp256k1, TxIn, TxOut};

use crate::{CheckableProof, P2WPKHWitness, Pixel, PixelKey, PixelKeyError, PixelProof};

use super::p2wpkh::errors::P2WPKHProofError;
#[cfg(feature = "consensus")]
pub mod consensus;

/// The proof of ownership of the change output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EmptyPixelProof {
    /// Key of current owner of the pixel.
    pub inner_key: secp256k1::PublicKey,
}

impl EmptyPixelProof {
    pub fn new(inner_key: secp256k1::PublicKey) -> Self {
        Self { inner_key }
    }

    pub(crate) fn check_by_parsed_witness_data(
        &self,
        pubkey: &secp256k1::PublicKey,
    ) -> Result<(), P2WPKHProofError> {
        let pixel_key = PixelKey::new(Pixel::empty(), &self.inner_key)?;

        if *pixel_key != *pubkey {
            return Err(P2WPKHProofError::PublicKeyMismatch);
        }

        Ok(())
    }
}

impl From<EmptyPixelProof> for PixelProof {
    fn from(value: EmptyPixelProof) -> Self {
        Self::EmptyPixel(value)
    }
}

impl CheckableProof for EmptyPixelProof {
    type Error = P2WPKHProofError;

    /// Get from input witness signature and public key and check that public
    /// key is equal to the tweaked one from proof.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = P2WPKHWitness::from_witness(&txin.witness)?;

        self.check_by_parsed_witness_data(&data.pubkey)?;

        Ok(())
    }

    /// Get from transaction output `script_pubkey` and create P2WPKH script
    /// from tweaked public key from proof and compare it with `script_pubkey`.
    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        let pixel_key = PixelKey::new(Pixel::empty(), &self.inner_key)?;

        let expected_script_pubkey = pixel_key
            .to_p2wpkh()
            .ok_or(PixelKeyError::UncompressedKey)?;

        if txout.script_pubkey != expected_script_pubkey {
            return Err(P2WPKHProofError::ScriptPubKeyMismatch);
        }

        Ok(())
    }
}
