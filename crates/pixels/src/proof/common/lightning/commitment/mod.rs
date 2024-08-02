use bitcoin::{secp256k1::PublicKey, ScriptBuf, TxIn, TxOut};

use crate::{CheckableProof, Pixel, PixelKey};

pub use self::{
    errors::LightningCommitmentProofError,
    script::{LightningCommitmentProofData, ToLocalScript},
    witness::LightningCommitmentWitness,
};

#[cfg(feature = "consensus")]
pub mod consensus;
pub mod errors;
pub mod script;
pub mod witness;

/// Proof for ouput/input of the Lightning network commitment transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightningCommitmentProof {
    /// Pixel that proof verifies.
    pub pixel: Pixel,

    #[cfg_attr(feature = "serde", serde(flatten))]
    pub data: LightningCommitmentProofData,
}

impl CheckableProof for LightningCommitmentProof {
    type Error = LightningCommitmentProofError;

    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        use LightningCommitmentProofError as Error;

        let parsed_witness = LightningCommitmentWitness::try_from(&txin.witness)?;

        let expected_redeem_script = self.to_redeem_script()?;
        let expected_redeem_script_raw = ScriptBuf::from(&expected_redeem_script);

        if expected_redeem_script_raw != parsed_witness.redeem_script {
            return Err(Error::RedeemScriptMismatch {
                expected: expected_redeem_script.into(),
                found: parsed_witness.redeem_script,
            });
        }

        // TODO: check signature.

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        use LightningCommitmentProofError as Error;

        let expected_script_pubkey = self.to_script_pubkey()?;

        if txout.script_pubkey != expected_script_pubkey {
            return Err(Error::MismatchScriptPubkey {
                expected: expected_script_pubkey,
                found: txout.script_pubkey.clone(),
            });
        }

        Ok(())
    }
}

impl LightningCommitmentProof {
    pub fn new(
        pixel: Pixel,
        revocation_pubkey: PublicKey,
        to_self_delay: u16,
        local_delayed_pubkey: PublicKey,
    ) -> Self {
        Self {
            pixel,
            data: LightningCommitmentProofData::new(
                revocation_pubkey,
                to_self_delay,
                local_delayed_pubkey,
            ),
        }
    }

    /// Tweak revocation pubkey and convert with other data to redeem script.
    pub fn to_redeem_script(&self) -> Result<ToLocalScript, LightningCommitmentProofError> {
        let tweaked_revocation_pubkey = PixelKey::new(self.pixel, &self.data.revocation_pubkey)?;

        let redeem_script = ToLocalScript::new(
            *tweaked_revocation_pubkey,
            self.data.to_self_delay,
            self.data.local_delayed_pubkey,
        );

        Ok(redeem_script)
    }

    pub fn to_script_pubkey(&self) -> Result<ScriptBuf, LightningCommitmentProofError> {
        let redeem_script = self.to_redeem_script()?;

        Ok(ScriptBuf::from(&redeem_script).to_v0_p2wsh())
    }
}


