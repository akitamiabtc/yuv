use crate::alloc::string::ToString;
use alloc::{format, vec, vec::Vec};

use crate::proof::p2wsh::{
    errors::P2WSHWitnessParseError,
    witness::{FromWitnessStack, IntoWitnessStack, P2WSHWitness},
};

use bitcoin::ecdsa::Signature as EcdsaSig;

pub type LightningCommitmentWitness = P2WSHWitness<LightningCommitmentWitnessStack>;

/// Data that is stored in the Lightning network commitment transaction.
///
/// The structure of witness stack usually looks like this:
///
/// 1. Signature created by `revocation_pubkey` or `local_delayed_pubkey`.
/// 2. Flag that indicates if the signature is created by `revocation_pubkey` or
///   `local_delayed_pubkey`.
///   - `0x00` if the signature is created by `local_delayed_pubkey`.
///   - `0x01` if the signature is created by `revocation_pubkey`.
#[derive(Debug, PartialEq, Eq)]
pub struct LightningCommitmentWitnessStack {
    /// Signature created by `revocation_pubkey` or `local_delayed_pubkey`.
    pub signature: EcdsaSig,

    /// Indicates if the signature is created by `revocation_pubkey` or
    /// `local_delayed_pubkey` should be used.
    pub is_revocation: bool,
}

impl LightningCommitmentWitnessStack {
    pub fn new(signature: EcdsaSig, is_revocation: bool) -> Self {
        Self {
            signature,
            is_revocation,
        }
    }

    fn is_revocation_as_byte(&self) -> u8 {
        if self.is_revocation {
            0x01
        } else {
            0x00
        }
    }
}

impl FromWitnessStack for LightningCommitmentWitnessStack {
    fn from_witness_stack(stack: &[Vec<u8>]) -> Result<Self, P2WSHWitnessParseError> {
        use P2WSHWitnessParseError as Error;

        if stack.len() != 2 {
            return Err(Error::Custom(format!(
                "invalid witness length should be 3, got {}",
                stack.len()
            )));
        }

        // Parse signature
        let signature = EcdsaSig::from_slice(&stack[0])
            .map_err(|_| Error::Custom("invalid signature in witness".to_string()))?;

        // Parse is_revocation flag
        if stack[1].len() != 1 {
            return Err(Error::Custom("invalid is_revocation flag".to_string()));
        }
        let is_revocation = stack[1][0] == 0x01;

        Ok(LightningCommitmentWitnessStack {
            signature,
            is_revocation,
        })
    }
}

impl IntoWitnessStack for LightningCommitmentWitnessStack {
    fn into_witness_stack(self) -> Vec<Vec<u8>> {
        vec![
            self.signature.serialize().to_vec(),
            vec![self.is_revocation_as_byte()],
        ]
    }
}
