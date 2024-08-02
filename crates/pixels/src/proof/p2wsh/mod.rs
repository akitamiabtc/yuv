use bitcoin::{
    blockdata::script::Instruction,
    secp256k1::{constants::PUBLIC_KEY_SIZE, PublicKey},
    ScriptBuf, TxIn, TxOut,
};

use alloc::vec::Vec;

use crate::{CheckableProof, Pixel, Tweakable};

use self::{errors::P2WSHProofError, witness::P2WSHWitness};

#[cfg(feature = "consensus")]
pub mod consensus;
pub mod errors;
pub mod witness;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct P2WSHProof {
    /// The pixel that is being spent
    pub pixel: Pixel,

    /// Untweaked key, which is used in [`P2WSHProof::script`] in
    /// tweaked representation.
    pub inner_key: PublicKey,

    /// The spending script of P2WSH address.
    pub script: ScriptBuf,
}

impl CheckableProof for P2WSHProof {
    type Error = P2WSHProofError;

    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        self.check_pubkey()?;

        let witness = P2WSHWitness::<Vec<Vec<u8>>>::from_witness(&txin.witness)?;

        if witness.redeem_script != self.script {
            return Err(P2WSHProofError::RedeemScriptMismatch);
        }

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        self.check_pubkey()?;

        let expected_script = self.p2wsh_address_script();
        let got_script = &txout.script_pubkey;

        if &expected_script != got_script {
            return Err(P2WSHProofError::OutputScriptMismatch);
        }

        Ok(())
    }
}

impl P2WSHProof {
    pub fn new(pixel: Pixel, pubkey: PublicKey, script: ScriptBuf) -> Self {
        Self {
            pixel,
            inner_key: pubkey,
            script,
        }
    }

    fn p2wsh_address_script(&self) -> ScriptBuf {
        let script_hash = self.script.wscript_hash();

        ScriptBuf::new_v0_p2wsh(&script_hash)
    }

    /// Find the first that appears in the script.
    fn find_first_pubkey(&self) -> Option<&[u8]> {
        let mut instructions = self.script.instructions();

        while let Some(Ok(instruction)) = instructions.next() {
            let Instruction::PushBytes(bytes) = instruction else {
                continue;
            };

            let bytes = bytes.as_bytes();
            if is_pubkey(bytes) {
                return Some(bytes);
            }
        }

        None
    }

    fn try_find_pubkey(&self) -> Result<&[u8], P2WSHProofError> {
        self.find_first_pubkey()
            .ok_or(P2WSHProofError::MissingPubkey)
    }

    /// Check that the pubkey in the script is the same as the pubkey after
    /// tweaking in the proof.
    fn check_pubkey(&self) -> Result<(), P2WSHProofError> {
        let proof_tweaked_pubkey = self.inner_key.tweak(self.pixel);
        let script_pubkey_bytes = self.try_find_pubkey()?;

        let proof_tweaked_pubkey_bytes = proof_tweaked_pubkey.serialize();

        if script_pubkey_bytes != proof_tweaked_pubkey_bytes {
            return Err(P2WSHProofError::PubkeyMismatch);
        }

        Ok(())
    }
}

const EVEN_PARITY_BYTE: u8 = 0x02;
const ODD_PARITY_BYTE: u8 = 0x03;

const PARITIES: [u8; 2] = [EVEN_PARITY_BYTE, ODD_PARITY_BYTE];

/// Check if the bytes are a compressed public key.
fn is_pubkey(bytes: &[u8]) -> bool {
    bytes.len() == PUBLIC_KEY_SIZE && PARITIES.contains(&bytes[0])
}
