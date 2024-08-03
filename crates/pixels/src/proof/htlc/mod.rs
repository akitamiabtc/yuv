//! This module provides definitions for type of proof for HTLC lightning
//! outputs and inputs got from Lightning Network commitment transactions.

use bitcoin::{blockdata::script::Instruction, hashes::Hash, WScriptHash};

use crate::{CheckableProof, Pixel, Tweakable};

#[cfg(test)]
mod tests;

mod utils;

mod script;
pub use self::script::{HtlcScriptKind, LightningHtlcData, LightningHtlcScript};

mod errors;
pub use self::errors::LightningHtlcProofError;

/// Proof type for outputs/inputs of Bitcoin transactions that are using
/// HTLC scripts from Lightning Network protocol.
///
/// # Note
///
/// This proof should be used, when `force-close` occured on the channel,
/// and one of the sides want to `sweep` the funds from the channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightningHtlcProof {
    /// Pixel that is locked by HTLC.
    pub pixel: Pixel,

    pub data: LightningHtlcData,
}

impl LightningHtlcProof {
    pub fn new(pixel: Pixel, data: LightningHtlcData) -> Self {
        Self { pixel, data }
    }

    /// Convert proof into script by tweaking first key in the script with
    /// pixel.
    fn to_script(&self) -> LightningHtlcScript {
        let remote_key = self.data.remote_htlc_key.tweak(self.pixel);

        LightningHtlcScript {
            remote_htlc_key: remote_key,
            ..self.data
        }
    }
}

impl From<&LightningHtlcProof> for LightningHtlcScript {
    fn from(value: &LightningHtlcProof) -> Self {
        value.to_script()
    }
}

impl CheckableProof for LightningHtlcProof {
    type Error = LightningHtlcProofError;

    fn checked_check_by_input(&self, txin: &bitcoin::TxIn) -> Result<(), Self::Error> {
        // Last element in witness data for HTLC input should always be the wintess
        // program (the script).
        let Some(witness_program) = txin.witness.last() else {
            return Err(LightningHtlcProofError::InvalidWitnessProgramStructure);
        };

        let got = WScriptHash::hash(witness_program);
        let expected = WScriptHash::from(self.to_script());

        if got != expected {
            return Err(LightningHtlcProofError::ScriptHashMismatch { got, expected });
        }

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &bitcoin::TxOut) -> Result<(), Self::Error> {
        let expected: WScriptHash = self.to_script().into();

        let script_pubkey = &txout.script_pubkey;
        if !script_pubkey.is_v0_p2wsh() {
            return Err(LightningHtlcProofError::InvalidScriptKind);
        }

        // With `is_v0_p2wsh` check we know that the last instruction should be the
        // witness script hash (not an empty script).
        let Some(Ok(Instruction::PushBytes(hash))) = script_pubkey.instructions().last() else {
            return Err(LightningHtlcProofError::InvalidWitnessProgramStructure);
        };

        let got_script_hash = WScriptHash::from_slice(hash.as_bytes())
            .map_err(|_| LightningHtlcProofError::InvalidWScriptHashSize(hash.len()))?;

        if expected != got_script_hash {
            return Err(LightningHtlcProofError::ScriptHashMismatch {
                got: got_script_hash,
                expected,
            });
        }

        Ok(())
    }
}
