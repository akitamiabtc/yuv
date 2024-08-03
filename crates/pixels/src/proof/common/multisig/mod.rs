//! Implementation of Multisig proof which can be converted into P2WSH proof.

use alloc::vec::Vec;
use bitcoin::ecdsa::Signature;
use bitcoin::secp256k1::PublicKey;
use bitcoin::{secp256k1, ScriptBuf, TxIn, TxOut};

use crate::proof::p2wsh::P2WSHProof;
use crate::{CheckableProof, Pixel, PixelKey};

use self::errors::MultisigPixelProofError;
use self::script::MultisigScript;
use self::witness::MultisigWitness;

#[cfg(feature = "consensus")]
pub mod consensus;
pub mod errors;
pub mod script;
pub mod witness;

/// Pixel proof for multisignature transaction that uses P2WSH script.
///
/// The main difference from normal multisignature transaction that it uses
/// tweaked with pixel public key as firstr key. The order of the is defined
/// lexigraphically.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MultisigPixelProof {
    /// Pixel for the first tweaked key.
    pub pixel: Pixel,

    /// Public keys that participate in the transaction.
    pub inner_keys: Vec<secp256k1::PublicKey>,

    /// Number of required signatures.
    pub m: u8,
}

impl CheckableProof for MultisigPixelProof {
    type Error = MultisigPixelProofError;

    /// Check proof, as it's was provided for the Bitcoin transaction input, by
    /// parsed from witness (or script) values.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = MultisigWitness::from_witness(&txin.witness)?;

        self.check_by_parsed_witness_data(&data.stack, &data.redeem_script)?;

        Ok(())
    }

    /// Check by proof by transaction output by comparing expected and got `script_pubkey`.
    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), MultisigPixelProofError> {
        let expected_redeem_script = self.create_multisig_redeem_script()?;

        if txout.script_pubkey != expected_redeem_script.to_v0_p2wsh() {
            return Err(MultisigPixelProofError::RedeemScriptMismatch);
        }

        Ok(())
    }
}

impl MultisigPixelProof {
    pub fn new(pixel: impl Into<Pixel>, mut inner_keys: Vec<secp256k1::PublicKey>, m: u8) -> Self {
        // Sort public keys lexigraphically
        inner_keys.sort();

        Self {
            pixel: pixel.into(),
            inner_keys,
            m,
        }
    }

    /// From known public keys of participants create `reedem_script` and check
    /// that it's equal to the script that was provided in the transaction. Also
    /// check that the number of signatures is correct.
    pub(crate) fn check_by_parsed_witness_data(
        &self,
        signatures: &[Signature],
        redeem_script: &ScriptBuf,
    ) -> Result<(), MultisigPixelProofError> {
        // Number of provided signatures must be equal to number of participants.
        if signatures.len() != self.m as usize {
            return Err(MultisigPixelProofError::InvalidNumberOfSignatures(
                signatures.len(),
                self.m as usize,
            ));
        }

        let expected_script = self.create_multisig_redeem_script()?;

        // Redeem script in transaction is not equal to expected one.
        if expected_script != *redeem_script {
            return Err(MultisigPixelProofError::RedeemScriptMismatch);
        }

        // TODO: check signatures.

        Ok(())
    }

    /// Return copy of inner keys sorted lexigraphically with first key tweaked.
    pub(crate) fn sort_and_tweak_keys(&self) -> Result<Vec<PublicKey>, MultisigPixelProofError> {
        let mut keys = self.inner_keys.clone();

        keys.sort();

        let Some(first_key) = keys.first() else {
            return Err(MultisigPixelProofError::InvalidNumberOfInnerKeys(0, 1));
        };

        let pixel_key = PixelKey::new(self.pixel, first_key)?;

        // Replace first key with tweaked one.
        keys[0] = *pixel_key;

        Ok(keys)
    }

    /// Tweak first key from proof and create multisig redeem script from it and
    /// other keys.
    pub(crate) fn create_multisig_redeem_script(
        &self,
    ) -> Result<ScriptBuf, MultisigPixelProofError> {
        let keys = self.sort_and_tweak_keys()?;

        Ok(MultisigScript::new(self.m, keys).to_script())
    }

    pub fn to_script_pubkey(&self) -> Result<ScriptBuf, MultisigPixelProofError> {
        self.create_multisig_redeem_script()
            .map(|script| script.to_v0_p2wsh())
    }

    pub fn to_reedem_script(&self) -> Result<ScriptBuf, MultisigPixelProofError> {
        self.create_multisig_redeem_script()
    }
}

impl TryFrom<MultisigPixelProof> for P2WSHProof {
    type Error = MultisigPixelProofError;

    fn try_from(proof: MultisigPixelProof) -> Result<Self, Self::Error> {
        let keys = proof.sort_and_tweak_keys()?;

        let first = keys
            .first()
            .ok_or(MultisigPixelProofError::InvalidNumberOfInnerKeys(0, 1))?;

        Ok(P2WSHProof::new(
            proof.pixel,
            *first,
            proof.to_reedem_script()?,
        ))
    }
}
