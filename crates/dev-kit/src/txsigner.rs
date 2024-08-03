use std::collections::HashMap;

use bdk::{
    miniscript::ToPublicKey,
    signer::{InputSigner, SignerContext, SignerWrapper},
    SignOptions,
};
use bitcoin::{
    key::XOnlyPublicKey,
    psbt::PartiallySignedTransaction,
    secp256k1::{self, All, PublicKey, Secp256k1},
    PrivateKey, ScriptBuf,
};
use eyre::bail;
use yuv_pixels::{
    MultisigPixelProof, MultisigWitness, P2WPKHWitness, Pixel, PixelPrivateKey, PixelProof,
};
use yuv_types::ProofMap;

pub struct TransactionSigner {
    /// Secp256k1 engine is used to execute all signature operations.
    ctx: Secp256k1<All>,
    private_key: PrivateKey,

    /// Key-value storage of signers that will participate in transaction
    /// signing. Where key is public key of the signer, and value is private key
    /// of the signer without any tweaking (for both keys).
    signers: HashMap<XOnlyPublicKey, secp256k1::SecretKey>,
}

impl TransactionSigner {
    pub fn new(ctx: Secp256k1<All>, private_key: PrivateKey) -> Self {
        TransactionSigner {
            ctx,
            private_key,
            signers: HashMap::new(),
        }
    }

    pub fn extend_signers(&mut self, signers: HashMap<XOnlyPublicKey, secp256k1::SecretKey>) {
        self.signers.extend(signers);
    }

    pub fn sign(
        self,
        psbt: &mut PartiallySignedTransaction,
        input_proofs: &ProofMap,
    ) -> Result<(), eyre::ErrReport> {
        for (index, proof) in input_proofs {
            match &proof {
                PixelProof::Sig(sigproof) => {
                    self.sign_input(sigproof.pixel, &sigproof.inner_key, psbt, *index)?;
                }
                PixelProof::Multisig(multisig_proof) => {
                    self.sign_multiproof_input(multisig_proof, psbt, *index)?;
                }
                #[cfg(feature = "bulletproof")]
                PixelProof::Bulletproof(proof) => {
                    self.sign_input(proof.pixel, &proof.inner_key, psbt, *index)?;
                }
                PixelProof::EmptyPixel(proof) => {
                    self.sign_input(Pixel::empty(), &proof.inner_key, psbt, *index)?;
                }
                PixelProof::LightningHtlc(_) | PixelProof::Lightning(_) => {
                    bail!(
                        r#"HTLC and Lightning inputs cannot be signed using BDK wallet. Only LDK node can
                        spend it, as it has all required information and keys."#
                    )
                }
                PixelProof::P2WSH(_p2wsh_proof) => {
                    bail!(r#"Signing P2WSH inputs is not supported yet."#)
                }
            };
        }

        Ok(())
    }

    /// Add witness (signatures, redeem script) for pixel multisig P2WSH input
    /// with tweaked by pixel key.
    fn sign_multiproof_input(
        &self,
        multisig_proof: &MultisigPixelProof,
        psbt: &mut PartiallySignedTransaction,
        index: u32,
    ) -> eyre::Result<()> {
        // clean partial sigs for this input
        psbt.inputs[index as usize].partial_sigs.clear();

        let mut key_pairs = multisig_proof
            .inner_keys
            .iter()
            .filter_map(|key| {
                self.signers
                    .get(&XOnlyPublicKey::from(*key))
                    .cloned()
                    .map(|secret| (secret, *key))
            })
            .collect::<Vec<_>>();

        if key_pairs.len() < multisig_proof.m as usize {
            bail!(
                "Not enough signers for multisig pixel: {} < {}",
                key_pairs.len(),
                multisig_proof.m
            );
        }

        key_pairs.sort_by(|(_, a_pubkey), (_, b_pubkey)| {
            a_pubkey.serialize().cmp(&b_pubkey.serialize())
        });

        let mut secret_keys: Vec<_> = key_pairs
            .into_iter()
            .map(|(secret_key, _)| secret_key)
            .collect();

        // Replace first with one tweaked by pixel to satisfy protocol rules.
        if let Some(first_key) = secret_keys.first_mut() {
            let tweaked =
                PixelPrivateKey::new_with_ctx(multisig_proof.pixel, first_key, &self.ctx)?;

            *first_key = tweaked.0;
        }

        for secret_key in secret_keys {
            let signer = SignerWrapper::new(
                PrivateKey::new(secret_key, self.private_key.network),
                SignerContext::Segwitv0,
            );

            signer.sign_input(
                psbt,
                index as usize,
                &SignOptions {
                    try_finalize: false,
                    trust_witness_utxo: true,
                    ..Default::default()
                },
                &self.ctx,
            )?;
        }

        let signed_input = psbt
            .inputs
            .get_mut(index as usize)
            .expect("Signed input should exist");

        let signatures = signed_input
            .partial_sigs
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let witness = MultisigWitness::new(signatures, multisig_proof.to_reedem_script()?);

        signed_input.final_script_sig = Some(ScriptBuf::new());
        signed_input.final_script_witness = Some(witness.into_witness());

        Ok(())
    }

    fn sign_input(
        &self,
        pixel: Pixel,
        inner_key: &secp256k1::PublicKey,
        psbt: &mut PartiallySignedTransaction,
        index: u32,
    ) -> Result<(), eyre::ErrReport> {
        // Tweak key with pixel and get public key
        let signing_key = self
            .signers
            .get(&XOnlyPublicKey::from(*inner_key))
            .expect("Singing key for proof should exist");

        let tweaked_key = PixelPrivateKey::new_with_ctx(pixel, signing_key, &self.ctx)?;
        let tweaked_pubkey = tweaked_key.0.public_key(&self.ctx).to_public_key();

        // Create a wrapper around private key which can sign transaction inputs.
        let signer = SignerWrapper::new(
            PrivateKey::new(tweaked_key.0, self.private_key.network),
            SignerContext::Segwitv0,
        );

        signer.sign_input(
            psbt,
            index as usize,
            &SignOptions {
                // Do not try to finalize, better to do it by our self as it
                // will always fail.
                try_finalize: false,
                trust_witness_utxo: true,
                ..Default::default()
            },
            &self.ctx,
        )?;

        // Get signed input from psbt
        let signed_input = psbt.inputs.get_mut(index as usize).unwrap();
        let signature = signed_input.partial_sigs.get(&tweaked_pubkey).unwrap();

        // And finalize it with witness data.
        let witness = P2WPKHWitness::new(
            *signature,
            PublicKey::from_slice(&tweaked_pubkey.to_bytes()).unwrap(),
        );

        signed_input.final_script_witness = Some(witness.into());
        signed_input.final_script_sig = Some(ScriptBuf::new());

        Ok(())
    }
}
