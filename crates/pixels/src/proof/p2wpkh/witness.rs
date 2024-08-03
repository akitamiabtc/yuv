use bitcoin::{ecdsa::Signature, secp256k1::PublicKey, Witness};

use super::errors::P2WPKHWitnessParseError;

/// Data that spends a P2WPKH output.
pub struct P2WPKHWitness {
    /// Signature of the transaction.
    pub signature: Signature,

    /// Public key of the transaction.
    pub pubkey: PublicKey,
}

impl P2WPKHWitness {
    pub fn new(signature: Signature, pubkey: PublicKey) -> Self {
        Self { signature, pubkey }
    }

    /// Parse a witness into a [`P2WPKHWitness`].
    pub fn from_witness(witness: &Witness) -> Result<Self, P2WPKHWitnessParseError> {
        if witness.len() != 2 {
            return Err(P2WPKHWitnessParseError::StackLengthMismatch);
        }

        let mut witness_iter = witness.iter();

        // Get signature from witness
        let signature = witness_iter
            .next()
            .ok_or(P2WPKHWitnessParseError::StackLengthMismatch)?;

        let signature = Signature::from_slice(signature)?;

        // Get public key from witness
        let pubkey = witness_iter
            .next()
            .ok_or(P2WPKHWitnessParseError::StackLengthMismatch)?;

        let pubkey = PublicKey::from_slice(pubkey)?;

        Ok(Self { signature, pubkey })
    }
}

impl TryFrom<&Witness> for P2WPKHWitness {
    type Error = P2WPKHWitnessParseError;

    fn try_from(witness: &Witness) -> Result<Self, Self::Error> {
        P2WPKHWitness::from_witness(witness)
    }
}

impl From<P2WPKHWitness> for Witness {
    fn from(value: P2WPKHWitness) -> Self {
        let mut witness = Witness::new();

        witness.push_bitcoin_signature(
            &value.signature.sig.serialize_der(),
            value.signature.hash_ty,
        );
        witness.push(value.pubkey.serialize());

        witness
    }
}
