use alloc::vec;
use alloc::vec::Vec;
use core::hash::Hash;
#[cfg(feature = "bulletproof")]
use {
    crate::Luma,
    alloc::boxed::Box,
    bitcoin::{
        hashes::{sha256::Hash as Sha256Hash, Hash as BitcoinHash, HashEngine},
        secp256k1::schnorr::Signature as SchnorrSignature,
    },
    bulletproof::{
        k256::{elliptic_curve::group::GroupEncoding, EncodedPoint, ProjectivePoint},
        RangeProof,
    },
    core::hash::Hasher,
};

use bitcoin::{
    blockdata::{opcodes, script::Builder},
    ecdsa::Signature,
    script::PushBytesBuf,
    secp256k1, PublicKey, Script, ScriptBuf, TxIn, TxOut, Witness,
};

#[cfg(all(feature = "bulletproof", feature = "serde"))]
use bulletproof::k256::elliptic_curve::sec1::FromEncodedPoint;

use crate::errors::{
    EmptyPixelProofError, LightningCommitmentProofError, LightningCommitmentWitnessParseError,
    MultisigPixelProofError, MultisigWitnessParseError, P2WPKHWitnessParseError, PixelKeyError,
    PixelProofError, SigPixelProofError,
};
use crate::script::ToLocalScript;
use crate::{Pixel, PixelKey};

use self::htlc::{LightningHtlcData, LightningHtlcProof};

pub mod htlc;

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

    /// The bulletproof with a corresponsing Pedersen commitment
    #[cfg(feature = "bulletproof")]
    Bulletproof(Box<Bulletproof>),

    /// Proof for spending lightning HTLC output at force-close.
    LightningHtlc(LightningHtlcProof),
}

impl PixelProof {
    #[inline]
    pub fn pixel(&self) -> Pixel {
        match self {
            Self::Sig(sig_proof) => sig_proof.pixel,
            Self::Multisig(multisig_proof) => multisig_proof.pixel,
            Self::Lightning(lightning_proof) => lightning_proof.pixel,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.pixel,
            Self::LightningHtlc(htlc) => htlc.pixel,
            Self::EmptyPixel(_) => Pixel::empty(),
        }
    }

    pub fn sig(pixel: impl Into<Pixel>, inner_key: secp256k1::PublicKey) -> Self {
        Self::Sig(SigPixelProof::new(pixel.into(), inner_key))
    }

    pub fn multisig(pixel: impl Into<Pixel>, inner_keys: Vec<secp256k1::PublicKey>, m: u8) -> Self {
        Self::Multisig(MultisigPixelProof::new(pixel, inner_keys, m))
    }

    pub fn lightning_htlc(pixel: impl Into<Pixel>, data: LightningHtlcData) -> Self {
        Self::LightningHtlc(LightningHtlcProof::new(pixel.into(), data))
    }

    pub fn lightning(
        pixel: impl Into<Pixel>,
        revocation_pubkey: PublicKey,
        to_self_delay: u16,
        local_delayed_pubkey: PublicKey,
    ) -> Self {
        Self::Lightning(LightningCommitmentProof::new(
            pixel.into(),
            revocation_pubkey,
            to_self_delay,
            local_delayed_pubkey,
        ))
    }

    #[cfg(feature = "bulletproof")]
    pub fn bulletproof(
        pixel: impl Into<Pixel>,
        inner_key: secp256k1::PublicKey,
        sender_key: secp256k1::PublicKey,
        commitment: ProjectivePoint,
        proof: RangeProof,
        signature: SchnorrSignature,
        chroma_signature: SchnorrSignature,
    ) -> Self {
        Self::Bulletproof(Box::new(Bulletproof::new(
            pixel.into(),
            inner_key,
            sender_key,
            commitment,
            proof,
            signature,
            chroma_signature,
        )))
    }

    #[cfg(feature = "bulletproof")]
    pub fn is_bulletproof(&self) -> bool {
        matches!(self, Self::Bulletproof(_))
    }

    pub fn is_empty_pixelproof(&self) -> bool {
        matches!(self, Self::EmptyPixel(_))
    }

    #[cfg(feature = "bulletproof")]
    pub fn get_bulletproof(&self) -> Option<&Bulletproof> {
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
            Self::Sig(sig_proof) => sig_proof.checked_check_by_input(txin)?,
            Self::Multisig(multisig_proof) => multisig_proof.checked_check_by_input(txin)?,
            Self::Lightning(lightning_proof) => lightning_proof.checked_check_by_input(txin)?,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.checked_check_by_input(txin)?,
            Self::LightningHtlc(htlc) => htlc.checked_check_by_input(txin)?,
            Self::EmptyPixel(empty_pixelproof) => empty_pixelproof.checked_check_by_input(txin)?,
        };

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        match self {
            Self::Sig(sig_proof) => sig_proof.checked_check_by_output(txout)?,
            Self::Multisig(multisig_proof) => multisig_proof.checked_check_by_output(txout)?,
            Self::Lightning(lightning_proof) => lightning_proof.checked_check_by_output(txout)?,
            #[cfg(feature = "bulletproof")]
            Self::Bulletproof(bulletproof) => bulletproof.checked_check_by_output(txout)?,
            Self::LightningHtlc(htlc) => htlc.checked_check_by_output(txout)?,
            Self::EmptyPixel(empty_pixelproof) => {
                empty_pixelproof.checked_check_by_output(txout)?
            }
        };

        Ok(())
    }
}

/// The bulletproof with a corresponsing Pedersen commitment
#[cfg(feature = "bulletproof")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bulletproof {
    /// Pixel that proof verifies.
    pub pixel: Pixel,
    /// Key of current owner of the pixel.
    pub inner_key: secp256k1::PublicKey,
    /// Key of of the sender.
    pub sender_key: secp256k1::PublicKey,
    /// Pedersen commitment of the pixel amount.
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "commitment_to_hex",
            deserialize_with = "hex_to_commitment"
        )
    )]
    pub commitment: ProjectivePoint,
    /// Bulletproof proof itself .
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "rangeproof_to_hex",
            deserialize_with = "hex_to_rangeproof"
        )
    )]
    pub proof: RangeProof,
    pub signature: SchnorrSignature,
    pub chroma_signature: SchnorrSignature,
}

#[cfg(feature = "bulletproof")]
impl From<Bulletproof> for PixelProof {
    fn from(value: Bulletproof) -> Self {
        Self::Bulletproof(Box::new(value))
    }
}

#[cfg(feature = "bulletproof")]
impl Hash for Bulletproof {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pixel.hash(state);
        self.inner_key.serialize().hash(state);

        let encoded_point = EncodedPoint::from(self.commitment.to_affine());

        encoded_point.hash(state);

        self.proof.hash(state);
    }
}

#[cfg(feature = "bulletproof")]
#[derive(Debug)]
pub enum BulletproofError {
    PixelKeyError(PixelKeyError),
    InvalidWitnessPublicKey(alloc::boxed::Box<PublicKey>, alloc::boxed::Box<PublicKey>),
    P2wkhWitnessParseError(P2WPKHWitnessParseError),
    InvalidScript(ScriptBuf, ScriptBuf),
    InvalidRangeProof,
    LumaMismatch,
}

#[cfg(feature = "bulletproof")]
impl From<PixelKeyError> for BulletproofError {
    fn from(err: PixelKeyError) -> Self {
        Self::PixelKeyError(err)
    }
}

#[cfg(feature = "bulletproof")]
impl From<P2WPKHWitnessParseError> for BulletproofError {
    fn from(err: P2WPKHWitnessParseError) -> Self {
        Self::P2wkhWitnessParseError(err)
    }
}

#[cfg(feature = "bulletproof")]
impl core::fmt::Display for BulletproofError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PixelKeyError(err) => write!(f, "PixelKeyError: {}", err),
            Self::InvalidWitnessPublicKey(expected, found) => write!(
                f,
                "InvalidWitnessPublicKey: expected: {}, found: {}",
                expected, found
            ),
            Self::P2wkhWitnessParseError(err) => write!(f, "P2wkhWitnessParseError: {}", err),
            Self::InvalidScript(expected, found) => {
                write!(f, "InvalidScript: expected: {}, found: {}", expected, found)
            }
            Self::InvalidRangeProof => write!(f, "InvalidRangeProof"),
            Self::LumaMismatch => write!(f, "Luma doesn't match the proof and commitment"),
        }
    }
}

#[cfg(all(not(feature = "no-std"), feature = "bulletproof"))]
impl std::error::Error for BulletproofError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PixelKeyError(err) => Some(err),
            Self::InvalidWitnessPublicKey(_, _) => None,
            Self::P2wkhWitnessParseError(err) => Some(err),
            Self::InvalidScript(_, _) => None,
            Self::InvalidRangeProof => None,
            Self::LumaMismatch => None,
        }
    }
}

#[cfg(feature = "bulletproof")]
impl Bulletproof {
    pub fn new(
        pixel: Pixel,
        inner_key: secp256k1::PublicKey,
        sender_key: secp256k1::PublicKey,
        commitment: ProjectivePoint,
        proof: RangeProof,
        signature: SchnorrSignature,
        chroma_signature: SchnorrSignature,
    ) -> Self {
        Self {
            pixel,
            inner_key,
            sender_key,
            commitment,
            proof,
            signature,
            chroma_signature,
        }
    }

    /// Check proof by parsed witness data.
    pub(crate) fn check_by_parsed_witness_data(
        &self,
        _signature: &Signature,
        pubkey: &PublicKey,
    ) -> Result<(), BulletproofError> {
        let pixel_key = PixelKey::new(self.pixel, &self.inner_key)?;

        if pixel_key.0 != *pubkey {
            return Err(BulletproofError::InvalidWitnessPublicKey(
                (*pubkey).into(),
                pixel_key.0.into(),
            ));
        }

        Ok(())
    }

    pub(crate) fn check_luma(&self) -> bool {
        let mut hash_engine = Sha256Hash::engine();

        hash_engine.input(&self.commitment.to_bytes());
        hash_engine.input(&self.proof.to_bytes());

        let bytes = Sha256Hash::from_engine(hash_engine);
        let value_proof_hash = bytes.to_byte_array();

        Luma::from(value_proof_hash) == self.pixel.luma
    }
}

#[cfg(feature = "bulletproof")]
impl CheckableProof for Bulletproof {
    type Error = BulletproofError;

    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = P2WPKHWitnessData::from_witness(&txin.witness)?;

        self.check_by_parsed_witness_data(&data.signature, &data.pubkey)?;

        if !bulletproof::verify(self.commitment, self.proof.clone()) {
            return Err(BulletproofError::InvalidRangeProof);
        }

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        let pixel_key = PixelKey::new(self.pixel, &self.inner_key)?;

        let expected_script_pubkey = pixel_key
            .to_p2wpkh()
            .ok_or(PixelKeyError::UncompressedKey)?;

        if txout.script_pubkey != expected_script_pubkey {
            return Err(BulletproofError::InvalidScript(
                txout.script_pubkey.clone(),
                expected_script_pubkey,
            ));
        }

        if !self.check_luma() {
            return Err(BulletproofError::LumaMismatch);
        }

        if !bulletproof::verify(self.commitment, self.proof.clone()) {
            return Err(BulletproofError::InvalidRangeProof);
        }

        Ok(())
    }
}

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
        pubkey: &PublicKey,
    ) -> Result<(), EmptyPixelProofError> {
        let pixel_key = PixelKey::new(Pixel::empty(), &self.inner_key)?;

        if pixel_key.0 != *pubkey {
            return Err(EmptyPixelProofError::InvalidWitnessPublicKey(
                (*pubkey).into(),
                pixel_key.0.into(),
            ));
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
    type Error = EmptyPixelProofError;

    /// Get from input witness signature and public key and check that public
    /// key is equal to the tweaked one from proof.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = P2WPKHWitnessData::from_witness(&txin.witness)?;

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
            return Err(EmptyPixelProofError::InvalidScript(
                txout.script_pubkey.clone(),
                expected_script_pubkey,
            ));
        }

        Ok(())
    }
}

/// The proof of ownership with single signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SigPixelProof {
    /// Pixel that proof verifies.
    pub pixel: Pixel,
    /// Key of current owner of the pixel.
    pub inner_key: secp256k1::PublicKey,
}

impl SigPixelProof {
    pub fn new(pixel: Pixel, inner_key: secp256k1::PublicKey) -> Self {
        Self { pixel, inner_key }
    }

    /// Check proof by parsed witness data.
    pub(crate) fn check_by_parsed_witness_data(
        &self,
        _signature: &Signature,
        pubkey: &PublicKey,
    ) -> Result<(), SigPixelProofError> {
        let pixel_key = PixelKey::new(self.pixel, &self.inner_key)?;

        if pixel_key.0 != *pubkey {
            return Err(SigPixelProofError::InvalidWitnessPublicKey(
                (*pubkey).into(),
                pixel_key.0.into(),
            ));
        }

        // TODO: verify signature.

        Ok(())
    }
}

pub struct P2WPKHWitnessData {
    pub signature: Signature,
    pub pubkey: PublicKey,
}

impl P2WPKHWitnessData {
    pub fn new(signature: Signature, pubkey: PublicKey) -> Self {
        Self { signature, pubkey }
    }

    pub fn from_witness(witness: &Witness) -> Result<Self, P2WPKHWitnessParseError> {
        let mut witness_iter = witness.iter();

        // Get signature from witness
        let signature = witness_iter
            .next()
            .ok_or(P2WPKHWitnessParseError::InvalidWitnessStructure)?;

        let signature = Signature::from_slice(signature)?;

        // Get public key from witness
        let pubkey = witness_iter
            .next()
            .ok_or(P2WPKHWitnessParseError::InvalidWitnessStructure)?;

        let pubkey = PublicKey::from_slice(pubkey)?;

        Ok(Self { signature, pubkey })
    }
}

impl From<P2WPKHWitnessData> for Witness {
    fn from(value: P2WPKHWitnessData) -> Self {
        let mut witness = Witness::new();

        witness.push_bitcoin_signature(
            &value.signature.sig.serialize_der(),
            value.signature.hash_ty,
        );
        let mut data = PushBytesBuf::new();
        let _ = data.extend_from_slice(&value.pubkey.inner.serialize());
        witness.push(data.as_push_bytes());
        witness
    }
}

impl CheckableProof for SigPixelProof {
    type Error = SigPixelProofError;

    /// Get from input witness signature and public key and check that public
    /// key is equal to the tweaked one from proof.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = P2WPKHWitnessData::from_witness(&txin.witness)?;

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
            return Err(SigPixelProofError::InvalidScript(
                txout.script_pubkey.clone(),
                expected_script_pubkey,
            ));
        }

        Ok(())
    }
}

impl From<SigPixelProof> for PixelProof {
    fn from(value: SigPixelProof) -> Self {
        Self::Sig(value)
    }
}

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

pub struct MultisigWintessData {
    pub signatures: Vec<Signature>,
    pub redeem_script: ScriptBuf,
}

impl MultisigWintessData {
    pub fn new(signatures: Vec<Signature>, redeem_script: ScriptBuf) -> Self {
        Self {
            signatures,
            redeem_script,
        }
    }

    pub fn from_witness(
        witness: &Witness,
        required_signatures: u8,
    ) -> Result<Self, MultisigWitnessParseError> {
        let mut witness_iter = witness.iter();

        // Get OP_0 from witness as first value
        witness_iter
            .next()
            .ok_or(MultisigWitnessParseError::NoOp0)?;

        // Get signatures from witness
        let signatures = witness_iter
            .by_ref()
            .take(required_signatures as usize)
            .map(Signature::from_slice)
            .collect::<Result<Vec<_>, _>>()?;

        // Get redeem script from witness
        let unparsed_redeem_script = witness_iter
            .next()
            .ok_or(MultisigWitnessParseError::NoRedeemScript)?;

        let redeem_script = ScriptBuf::from(unparsed_redeem_script.to_vec());

        Ok(Self {
            signatures,
            redeem_script,
        })
    }

    pub fn into_witness(self) -> Witness {
        let mut witness = Witness::new();

        // Add OP_0 as first value
        witness.push(vec![]);

        // Add signatures
        for signature in self.signatures {
            witness.push_bitcoin_signature(&signature.sig.serialize_der(), signature.hash_ty);
        }

        // Add redeem script
        witness.push(self.redeem_script.into_bytes());

        witness
    }

    /// The same as [`Self::into_witness`] but as [`ScriptBuf`].
    pub fn to_witness_script(&self) -> ScriptBuf {
        let mut script = Builder::new();

        // Add OP_0 as first value
        script = script.push_slice([]);

        // Add signatures
        for signature in &self.signatures {
            script = script.push_slice(signature.serialize());
        }

        // Add redeem script
        let mut data = PushBytesBuf::new();
        let _ = data.extend_from_slice(self.redeem_script.as_bytes());
        script = script.push_slice(&data);

        script.into_script()
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
        redeem_script: &Script,
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
            return Err(MultisigPixelProofError::InvalidRedeemScript);
        }

        // TODO: check signatures.

        Ok(())
    }

    /// Tweak first key from proof and create multisig redeem script from it and
    /// other keys.
    pub(crate) fn create_multisig_redeem_script(
        &self,
    ) -> Result<ScriptBuf, MultisigPixelProofError> {
        let mut keys = self.inner_keys.clone();

        keys.sort();

        let Some(first_key) = keys.first() else {
            return Err(MultisigPixelProofError::InvalidNumberOfInnerKeys(0, 1));
        };

        let pixel_key = PixelKey::new(self.pixel, first_key)?;

        // Replace first key with tweaked one.
        keys[0] = pixel_key.0.inner;

        Ok(ScriptBuf::new_multisig_redeem_script(&keys, self.m))
    }

    pub fn to_script_pubkey(&self) -> ScriptBuf {
        self.create_multisig_redeem_script()
            .expect("should create multisig redeem script")
            .to_v0_p2wsh()
    }

    pub fn to_reedem_script(&self) -> Result<ScriptBuf, MultisigPixelProofError> {
        self.create_multisig_redeem_script()
    }
}

impl CheckableProof for MultisigPixelProof {
    type Error = MultisigPixelProofError;

    /// Check proof, as it's was provided for the Bitcoin transaction input, by
    /// parsed from witness (or script) values.
    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let data = MultisigWintessData::from_witness(&txin.witness, self.m)?;

        self.check_by_parsed_witness_data(&data.signatures, &data.redeem_script)?;

        Ok(())
    }

    /// Check by proof by transaction output by comparing expected and got `script_pubkey`.
    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), MultisigPixelProofError> {
        let expected_redeem_script = self.create_multisig_redeem_script()?;

        if txout.script_pubkey != expected_redeem_script.to_v0_p2wsh() {
            return Err(MultisigPixelProofError::InvalidRedeemScript);
        }

        Ok(())
    }
}

impl From<MultisigPixelProof> for PixelProof {
    fn from(value: MultisigPixelProof) -> Self {
        Self::Multisig(value)
    }
}

/// Data that are stored in the Lightning network commitment transaction.
#[derive(Debug, PartialEq, Eq)]
pub struct LightningCommitmentWitness {
    /// Signature created by `revocation_pubkey` or `local_delayed_pubkey`.
    pub signature: Signature,

    /// Indicates if the signature is created by `revocation_pubkey` or
    /// `local_delayed_pubkey` should be used.
    pub is_revocation: bool,

    /// Redeem script of the `to_local` output.
    pub redeem_script: ToLocalScript,
}

impl LightningCommitmentWitness {
    pub fn new(signature: Signature, is_revocation: bool, redeem_script: ToLocalScript) -> Self {
        Self {
            signature,
            is_revocation,
            redeem_script,
        }
    }
}

impl TryFrom<&Witness> for LightningCommitmentWitness {
    type Error = LightningCommitmentWitnessParseError;

    fn try_from(witness: &Witness) -> Result<Self, Self::Error> {
        use LightningCommitmentWitnessParseError as Error;

        let mut witness_iter = witness.iter();

        // Get signature from witness
        let signature = witness_iter.next().ok_or(Error::WitnessStructure)?;

        let signature = Signature::from_slice(signature)?;

        // Get if revocation or local delayed key was used
        // We check that if OP_TRUE or OP_FALSE was used.
        let is_revocation = witness_iter
            .next()
            // NOTE: In case if Bitcoin OP_TRUE is 1 pushed on the stack, and
            // OP_FALSE is push of the empty array onto it. So in case of
            // witness we check that pushed value is not an empty array.
            .map(|bytes| !bytes.is_empty())
            .ok_or(Error::WitnessStructure)?;

        // Get redeem script from witness
        let unparsed_redeem_script = witness_iter.next().ok_or(Error::WitnessStructure)?;

        let redeem_script = ScriptBuf::from(unparsed_redeem_script.to_vec());

        let to_local_script = ToLocalScript::try_from(&redeem_script)?;

        Ok(Self {
            signature,
            is_revocation,
            redeem_script: to_local_script,
        })
    }
}

impl From<LightningCommitmentWitness> for Witness {
    fn from(value: LightningCommitmentWitness) -> Self {
        let mut witness = Witness::new();

        witness.push_bitcoin_signature(
            &value.signature.sig.serialize_der(),
            value.signature.hash_ty,
        );

        witness.push(vec![value.is_revocation as u8]);

        witness.push(ScriptBuf::from(&value.redeem_script).into_bytes());

        witness
    }
}

/// Proof for ouput/input of the Lightning network commitment transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightningCommitmentProof {
    /// Pixel that proof verifies.
    pub pixel: Pixel,

    /// Revocation public key from the commitment transaction without tweak.
    ///
    /// As for YUV protocol rules first key in script is always the tweaked one,
    /// so we can use it to check that the transaction is correct.
    pub revocation_pubkey: secp256k1::PublicKey,

    /// Delay after which the `to_local` output can be spent by the local.
    pub to_self_delay: u16,

    /// Local delayed public key from the commitment transaction.
    pub local_delayed_pubkey: secp256k1::PublicKey,
}

impl CheckableProof for LightningCommitmentProof {
    type Error = LightningCommitmentProofError;

    fn checked_check_by_input(&self, txin: &TxIn) -> Result<(), Self::Error> {
        let parsed_witness = LightningCommitmentWitness::try_from(&txin.witness)?;

        let expected_redeem_script = self.to_redeem_script()?;

        if expected_redeem_script != parsed_witness.redeem_script {
            return Err(Self::Error::RedeemScriptMismatch {
                expected: expected_redeem_script.into(),
                found: parsed_witness.redeem_script.into(),
            });
        }

        // TODO: check signature.

        Ok(())
    }

    fn checked_check_by_output(&self, txout: &TxOut) -> Result<(), Self::Error> {
        let expected_script_pubkey = self.to_script_pubkey()?;

        if txout.script_pubkey != expected_script_pubkey {
            return Err(Self::Error::MismatchScriptPubkey {
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
            revocation_pubkey: revocation_pubkey.inner,
            to_self_delay,
            local_delayed_pubkey: local_delayed_pubkey.inner,
        }
    }

    /// Tweak revocation pubkey and convert with other data to redeem script.
    pub fn to_redeem_script(&self) -> Result<ToLocalScript, LightningCommitmentProofError> {
        let tweaked_revocation_key = PixelKey::new(self.pixel, &self.revocation_pubkey)?;

        let redeem_script = ToLocalScript::new(
            tweaked_revocation_key,
            self.to_self_delay,
            self.local_delayed_pubkey,
        );

        Ok(redeem_script)
    }

    pub fn to_script_pubkey(&self) -> Result<ScriptBuf, LightningCommitmentProofError> {
        let redeem_script = self.to_redeem_script()?;

        Ok(ScriptBuf::from(&redeem_script).to_v0_p2wsh())
    }
}

// ===========================================================================
//  TODO: Some utility traits, methods that should be moved somewhere
//  in the future.
// ==========================================================================

/// Utility trait for [`Script`] that gives constructor for P2WSH multisig redeem script.
pub(crate) trait P2WSHScript {
    /// Create P2WSH redeem script from given public keys and required number of
    /// signatures.
    fn new_multisig_redeem_script(
        pubkeys: &[secp256k1::PublicKey],
        required_signatures_num: u8,
    ) -> ScriptBuf {
        let mut builder = Builder::new().push_smallint(pubkeys.len() as u8);

        // NOTE: we assume that public are already sorted and compressed.
        for pubkey in pubkeys {
            builder = builder.push_slice(pubkey.serialize());
        }

        builder
            .push_smallint(required_signatures_num)
            .push_opcode(opcodes::all::OP_CHECKMULTISIG)
            .into_script()
    }
}

impl P2WSHScript for ScriptBuf {}

/// Utility trait that gives method for [`Builder`] to push small integer using
/// [`opcodes::all::OP_1`]..[`opcodes::all::OP_16`].
pub(crate) trait PushSmallInt {
    fn push_smallint(self, n: u8) -> Self;
}

impl PushSmallInt for Builder {
    fn push_smallint(self, n: u8) -> Self {
        match n {
            1..=16 => self.push_opcode((opcodes::all::OP_PUSHNUM_1.to_u8() - 1 + n).into()),
            _ => panic!("Can't push small int > 16"),
        }
    }
}

#[cfg(all(feature = "serde", feature = "bulletproof"))]
pub fn commitment_to_hex<S>(commitment: &ProjectivePoint, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let encoded_point = EncodedPoint::from(commitment.to_affine());

    serializer.serialize_str(&hex::encode(encoded_point))
}

#[cfg(all(feature = "serde", feature = "bulletproof"))]
pub fn hex_to_commitment<'de, D>(deserializer: D) -> Result<ProjectivePoint, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: alloc::string::String = deserializer.deserialize_string(crate::HexVisitor)?;
    let data = hex::decode(s).map_err(serde::de::Error::custom)?;

    let encoded_point = EncodedPoint::from_bytes(data).map_err(serde::de::Error::custom)?;

    if let Some(commit) = ProjectivePoint::from_encoded_point(&encoded_point).into() {
        return Ok(commit);
    }

    Err(serde::de::Error::custom("invalid commitment received"))
}

#[cfg(all(feature = "serde", feature = "bulletproof"))]
pub fn rangeproof_to_hex<S>(rangeproof: &RangeProof, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&hex::encode(rangeproof.to_bytes()))
}

#[cfg(all(feature = "serde", feature = "bulletproof"))]
pub fn hex_to_rangeproof<'de, D>(deserializer: D) -> Result<RangeProof, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: alloc::string::String = deserializer.deserialize_string(crate::HexVisitor)?;
    let data = hex::decode(s).map_err(serde::de::Error::custom)?;

    let proof =
        RangeProof::from_bytes(&data).ok_or(serde::de::Error::custom("invalid proof received"))?;

    Ok(proof)
}
