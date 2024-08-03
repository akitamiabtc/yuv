use alloc::vec::Vec;
use core2::io;

#[cfg(feature = "bulletproof")]
use {
    crate::Bulletproof,
    alloc::boxed::Box,
    bitcoin::secp256k1::constants::SCHNORR_SIGNATURE_SIZE,
    bulletproof::{
        k256::{elliptic_curve::group::GroupEncoding, ProjectivePoint},
        RangeProof,
    },
};

use bitcoin::{
    consensus::{encode::Error as EncodeError, Decodable, Encodable},
    hashes::{hash160, Hash},
    secp256k1::{self, constants::PUBLIC_KEY_SIZE},
};

use crate::{
    proof::{
        htlc::{HtlcScriptKind, LightningHtlcData, LightningHtlcProof},
        EmptyPixelProof,
    },
    LightningCommitmentProof, MultisigPixelProof, Pixel, PixelProof, SigPixelProof, PIXEL_SIZE,
};

// Pixel proof flags
const SIG_FLAG: u8 = 0u8;
const MULTISIG_FLAG: u8 = 1u8;
const LIGHTNING_FLAG: u8 = 2u8;
const LIGHTNING_HTLC_FLAG: u8 = 3u8;
#[cfg(feature = "bulletproof")]
const BULLETPROOF_FLAG: u8 = 4u8;
const EMPTY_PIXEL_FLAG: u8 = 5u8;

// Htlc script flags
const OFFERED_CONSENSUS_FLAG: u8 = 0u8;
const RECEIVED_CONSENSUS_FLAG: u8 = 1u8;

// hash160 size in bytes
const HASH_SIZE: usize = 20;

impl Encodable for Pixel {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&self.to_bytes())?;
        Ok(PIXEL_SIZE)
    }
}

impl Decodable for Pixel {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let mut bytes = [0u8; PIXEL_SIZE];
        reader.read_exact(&mut bytes)?;

        Pixel::from_bytes(&bytes).map_err(|_| EncodeError::ParseFailed("failed to parse the Pixel"))
    }
}

impl Encodable for PixelProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        match self {
            PixelProof::Sig(sig_proof) => {
                len += SIG_FLAG.consensus_encode(writer)?;
                len += sig_proof.consensus_encode(writer)?;
            }
            PixelProof::Multisig(multisig_proof) => {
                len += MULTISIG_FLAG.consensus_encode(writer)?;
                len += multisig_proof.consensus_encode(writer)?;
            }
            PixelProof::Lightning(lightning_proof) => {
                len += LIGHTNING_FLAG.consensus_encode(writer)?;
                len += lightning_proof.consensus_encode(writer)?;
            }
            #[cfg(feature = "bulletproof")]
            PixelProof::Bulletproof(bulletproof) => {
                len += BULLETPROOF_FLAG.consensus_encode(writer)?;
                len += bulletproof.consensus_encode(writer)?;
            }
            PixelProof::LightningHtlc(htlc_proof) => {
                len += LIGHTNING_HTLC_FLAG.consensus_encode(writer)?;
                len += htlc_proof.consensus_encode(writer)?;
            }
            PixelProof::EmptyPixel(empty_pixelproof) => {
                len += EMPTY_PIXEL_FLAG.consensus_encode(writer)?;
                len += empty_pixelproof.consensus_encode(writer)?;
            }
        }

        Ok(len)
    }
}

impl Decodable for PixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let flag: u8 = Decodable::consensus_decode(reader)?;

        match flag {
            SIG_FLAG => {
                let proof: SigPixelProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::Sig(proof))
            }
            MULTISIG_FLAG => {
                let proof: MultisigPixelProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::Multisig(proof))
            }
            LIGHTNING_FLAG => {
                let proof: LightningCommitmentProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::Lightning(proof))
            }
            LIGHTNING_HTLC_FLAG => {
                let proof: LightningHtlcProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::LightningHtlc(proof))
            }
            #[cfg(feature = "bulletproof")]
            BULLETPROOF_FLAG => {
                let proof: Bulletproof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::Bulletproof(Box::new(proof)))
            }
            EMPTY_PIXEL_FLAG => {
                let proof: EmptyPixelProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::EmptyPixel(proof))
            }
            _ => Err(EncodeError::ParseFailed("Unknown pixel proof")),
        }
    }
}

#[cfg(feature = "bulletproof")]
impl Encodable for Bulletproof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;

        len += writer.write(&self.inner_key.serialize())?;

        len += writer.write(&self.sender_key.serialize())?;

        let commitment_bytes = self.commitment.to_bytes();
        len += commitment_bytes.to_vec().consensus_encode(writer)?;

        len += self.proof.to_bytes().consensus_encode(writer)?;

        len += writer.write(self.signature.as_ref())?;

        len += writer.write(self.chroma_signature.as_ref())?;

        Ok(len)
    }
}

#[cfg(feature = "bulletproof")]
impl Decodable for Bulletproof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let inner_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let sender_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        let commitment_bytes: Vec<u8> = Decodable::consensus_decode(reader)?;
        let commitment: Option<ProjectivePoint> =
            ProjectivePoint::from_bytes(commitment_bytes.as_slice().into()).into();

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;

        let bytes: Vec<u8> = Decodable::consensus_decode(reader)?;
        let proof: RangeProof = RangeProof::from_bytes(bytes.as_slice())
            .ok_or_else(|| EncodeError::ParseFailed("Failed to parse the range proof"))?;

        let mut bytes = [0u8; SCHNORR_SIGNATURE_SIZE];
        reader.read_exact(&mut bytes)?;
        let signature = bitcoin::secp256k1::schnorr::Signature::from_slice(&bytes)
            .map_err(|_e| EncodeError::ParseFailed("Failed to parse the signature"))?;

        let mut bytes = [0u8; SCHNORR_SIGNATURE_SIZE];
        reader.read_exact(&mut bytes)?;
        let chroma_signature = bitcoin::secp256k1::schnorr::Signature::from_slice(&bytes)
            .map_err(|_e| EncodeError::ParseFailed("Failed to parse the chroma signature"))?;

        Ok(Bulletproof::new(
            pixel,
            inner_key,
            sender_key,
            commitment.ok_or_else(|| EncodeError::ParseFailed("Failed to parse the commitment"))?,
            proof,
            signature,
            chroma_signature,
        ))
    }
}

impl Encodable for SigPixelProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;
        len += writer.write(&self.inner_key.serialize())?;

        Ok(len)
    }
}

impl Decodable for SigPixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let inner_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        Ok(SigPixelProof::new(pixel, inner_key))
    }
}

impl Encodable for EmptyPixelProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write(&self.inner_key.serialize())
    }
}

impl Decodable for EmptyPixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let inner_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        Ok(EmptyPixelProof::new(inner_key))
    }
}

impl Encodable for MultisigPixelProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;

        len += (self.inner_keys.len() as u32).consensus_encode(writer)?;
        for inner_key in &self.inner_keys {
            len += writer.write(&inner_key.serialize())?;
        }

        len += self.m.consensus_encode(writer)?;

        Ok(len)
    }
}

impl Decodable for MultisigPixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let number_of_keys: u32 = Decodable::consensus_decode(reader)?;

        let inner_keys: Vec<secp256k1::PublicKey> = (0..number_of_keys)
            .map(|_i| {
                let mut bytes = [0u8; PUBLIC_KEY_SIZE];
                reader
                    .read_exact(&mut bytes)
                    .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;
                secp256k1::PublicKey::from_slice(&bytes)
                    .map_err(|_| EncodeError::ParseFailed("Failed to create public key from bytes"))
            })
            .collect::<Result<_, _>>()?;

        let m: u8 = Decodable::consensus_decode(reader)?;

        Ok(MultisigPixelProof::new(pixel, inner_keys, m))
    }
}

impl Encodable for LightningCommitmentProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;

        len += writer.write(&self.revocation_pubkey.serialize())?;

        len += self.to_self_delay.consensus_encode(writer)?;

        len += writer.write(&self.local_delayed_pubkey.serialize())?;

        Ok(len)
    }
}

impl Decodable for LightningCommitmentProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let revocation_pubkey = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        let to_self_delay: u16 = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let local_delayed_pubkey = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        Ok(LightningCommitmentProof {
            pixel,
            revocation_pubkey,
            to_self_delay,
            local_delayed_pubkey,
        })
    }
}

impl Encodable for LightningHtlcProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;
        len += self.data.consensus_encode(writer)?;

        Ok(len)
    }
}

impl Decodable for LightningHtlcProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;
        let data: LightningHtlcData = Decodable::consensus_decode(reader)?;

        Ok(LightningHtlcProof::new(pixel, data))
    }
}

impl Encodable for LightningHtlcData {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += writer.write(&self.revocation_key_hash.to_byte_array())?;
        len += writer.write(&self.remote_htlc_key.serialize())?;
        len += writer.write(&self.local_htlc_key.serialize())?;
        len += writer.write(&self.payment_hash.to_byte_array())?;
        len += self.kind.consensus_encode(writer)?;

        Ok(len)
    }
}

impl Decodable for LightningHtlcData {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let mut bytes = [0u8; HASH_SIZE];
        reader.read_exact(&mut bytes)?;
        let revocation_key_hash = hash160::Hash::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the hash"))?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let remote_htlc_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the public key"))?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let local_htlc_key = secp256k1::PublicKey::from_slice(&bytes)
            .map_err(|_| EncodeError::ParseFailed("Failed to parse the hash"))?;

        let mut bytes = [0u8; HASH_SIZE];
        reader.read_exact(&mut bytes)?;
        let payment_hash = hash160::Hash::from_byte_array(bytes);

        let kind: HtlcScriptKind = Decodable::consensus_decode(reader)?;

        Ok(LightningHtlcData::new(
            revocation_key_hash,
            remote_htlc_key,
            local_htlc_key,
            payment_hash,
            kind,
        ))
    }
}

impl Encodable for HtlcScriptKind {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        match self {
            HtlcScriptKind::Offered => {
                len += OFFERED_CONSENSUS_FLAG.consensus_encode(writer)?;
            }
            HtlcScriptKind::Received { cltv_expiry } => {
                len += RECEIVED_CONSENSUS_FLAG.consensus_encode(writer)?;
                len += cltv_expiry.consensus_encode(writer)?;
            }
        }

        Ok(len)
    }
}

impl Decodable for HtlcScriptKind {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let b: u8 = Decodable::consensus_decode(reader)?;

        match b {
            OFFERED_CONSENSUS_FLAG => Ok(HtlcScriptKind::Offered),
            RECEIVED_CONSENSUS_FLAG => {
                let cltv_expiry: u32 = Decodable::consensus_decode(reader)?;
                Ok(HtlcScriptKind::Received { cltv_expiry })
            }
            _ => Err(EncodeError::ParseFailed("Unknown HTLC script kind")),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;
    use core::str::FromStr;

    use bitcoin::{
        consensus::{Decodable, Encodable},
        hashes::hash160,
        key::XOnlyPublicKey,
        secp256k1,
    };
    use once_cell::sync::Lazy;

    use crate::proof::htlc;
    use crate::proof::htlc::LightningHtlcData;
    use crate::proof::htlc::LightningHtlcProof;
    #[cfg(feature = "bulletproof")]
    use crate::Bulletproof;
    use crate::Chroma;
    use crate::LightningCommitmentProof;
    use crate::MultisigPixelProof;
    use crate::Pixel;
    use crate::PixelProof;
    use crate::SigPixelProof;
    #[cfg(feature = "bulletproof")]
    use bitcoin::secp256k1::schnorr::Signature;

    static X_ONLY_PUBKEY: Lazy<XOnlyPublicKey> = Lazy::new(|| {
        XOnlyPublicKey::from_str("0677b5829356bb5e0c0808478ac150a500ceab4894d09854b0f75fbe7b4162f8")
            .expect("Should be valid address")
    });

    static PUBKEY: Lazy<secp256k1::PublicKey> = Lazy::new(|| {
        secp256k1::PublicKey::from_str(
            "03ab5575d69e46968a528cd6fa2a35dd7808fea24a12b41dc65c7502108c75f9a9",
        )
        .unwrap()
    });

    static HASH: Lazy<hash160::Hash> =
        Lazy::new(|| hash160::Hash::from_str("321ac998e78433e57a85171aa77bfad1d205ee3d").unwrap());

    #[cfg(feature = "bulletproof")]
    static SIG: Lazy<Signature> = Lazy::new(|| {
        Signature::from_str("32445f89b0fefe7dac06c6716c926ccd603cec8dd365a14ecb190a035617ec2700f0adad05e0d9912fb2eeaa336afd76fd752a1842c66d556d82f9f8c6e504aa")
            .unwrap()
    });

    #[cfg(feature = "bulletproof")]
    const BLINDING: [u8; 32] = [
        3, 123, 39, 117, 182, 201, 184, 57, 234, 12, 107, 82, 90, 37, 40, 13, 64, 45, 75, 160, 31,
        125, 243, 23, 141, 174, 13, 35, 231, 242, 197, 49,
    ];

    #[test]
    fn test_sig_pixel_proof_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);

        let proof = SigPixelProof::new(pixel, *PUBKEY);

        let mut bytes = Vec::new();

        proof
            .consensus_encode(&mut bytes)
            .expect("failed to encode the proof");

        let decoded_proof = SigPixelProof::consensus_decode(&mut bytes.as_slice())
            .expect("failed to decode the proof");

        assert_eq!(
            proof, decoded_proof,
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_multisig_pixel_proof_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);
        let inner_keys = vec![*PUBKEY, *PUBKEY, *PUBKEY];

        let proof = MultisigPixelProof::new(pixel, inner_keys, 2);

        let mut bytes = Vec::new();

        proof
            .consensus_encode(&mut bytes)
            .expect("failed to encode the proof");

        let decoded_proof = MultisigPixelProof::consensus_decode(&mut bytes.as_slice())
            .expect("failed to decode the proof");

        assert_eq!(
            proof, decoded_proof,
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_lightning_commitment_proof_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);

        let proof = LightningCommitmentProof {
            pixel,
            revocation_pubkey: *PUBKEY,
            to_self_delay: 100,
            local_delayed_pubkey: *PUBKEY,
        };

        let mut bytes = Vec::new();

        proof
            .consensus_encode(&mut bytes)
            .expect("failed to encode the proof");

        let decoded_proof = LightningCommitmentProof::consensus_decode(&mut bytes.as_slice())
            .expect("failed to decode the proof");

        assert_eq!(
            proof, decoded_proof,
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_lightning_htlc_proof_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);

        let proof = LightningHtlcProof::new(
            pixel,
            LightningHtlcData::new(
                *HASH,
                *PUBKEY,
                *PUBKEY,
                *HASH,
                htlc::HtlcScriptKind::Received { cltv_expiry: 100 },
            ),
        );

        let mut bytes = Vec::new();

        proof
            .consensus_encode(&mut bytes)
            .expect("failed to encode the proof");

        let decoded_proof = LightningHtlcProof::consensus_decode(&mut bytes.as_slice())
            .expect("failed to decode the proof");

        assert_eq!(
            proof, decoded_proof,
            "Converting back and forth should work"
        );
    }

    #[test]
    #[cfg(feature = "bulletproof")]
    fn test_bulletproof_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);

        let (range_proof, point) = bulletproof::generate(100, BLINDING);

        let proof = Bulletproof::new(pixel, *PUBKEY, *PUBKEY, point, range_proof, *SIG, *SIG);

        let mut bytes = Vec::new();

        proof
            .consensus_encode(&mut bytes)
            .expect("failed to encode the proof");

        let decoded_proof = Bulletproof::consensus_decode(&mut bytes.as_slice())
            .expect("failed to decode the proof");

        assert_eq!(
            proof, decoded_proof,
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_pixel_proofs_consensus_encode() {
        let chroma = Chroma::new(*X_ONLY_PUBKEY);
        let pixel = Pixel::new(100, chroma);

        #[cfg(feature = "bulletproof")]
        let (range_proof, point) = bulletproof::generate(100, BLINDING);

        let proofs: Vec<PixelProof> = vec![
            PixelProof::Sig(SigPixelProof::new(pixel, *PUBKEY)),
            PixelProof::Multisig(MultisigPixelProof::new(
                pixel,
                vec![*PUBKEY, *PUBKEY, *PUBKEY],
                2,
            )),
            PixelProof::Lightning(LightningCommitmentProof {
                pixel,
                revocation_pubkey: *PUBKEY,
                to_self_delay: 100,
                local_delayed_pubkey: *PUBKEY,
            }),
            PixelProof::LightningHtlc(LightningHtlcProof::new(
                pixel,
                LightningHtlcData::new(
                    *HASH,
                    *PUBKEY,
                    *PUBKEY,
                    *HASH,
                    htlc::HtlcScriptKind::Received { cltv_expiry: 100 },
                ),
            )),
            #[cfg(feature = "bulletproof")]
            PixelProof::Bulletproof(Box::new(Bulletproof::new(
                pixel,
                *PUBKEY,
                *PUBKEY,
                point,
                range_proof,
                *SIG,
                *SIG,
            ))),
        ];

        for proof in &proofs {
            let mut bytes = Vec::new();

            proof
                .consensus_encode(&mut bytes)
                .expect("failed to encode the proof");

            let decoded_proof = PixelProof::consensus_decode(&mut bytes.as_slice())
                .expect("failed to decode the proof");

            assert_eq!(
                proof, &decoded_proof,
                "Converting back and forth should work"
            );
        }
    }

    #[test]
    fn test_pixel_consensus_parsing() {
        let pixel = Pixel::new(100, *X_ONLY_PUBKEY);

        let mut bytes = Vec::new();

        pixel
            .consensus_encode(&mut bytes)
            .expect("failed to encode pixel");

        let decoded_pixel =
            Pixel::consensus_decode(&mut bytes.as_slice()).expect("failed to decode pixel");

        assert_eq!(
            pixel, decoded_pixel,
            "Converting back and forth should work"
        );
    }
}
