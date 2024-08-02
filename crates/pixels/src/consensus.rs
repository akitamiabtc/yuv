use alloc::boxed::Box;
use bitcoin::consensus::{encode::Error as EncodeError, Decodable, Encodable};
use core2::io;

#[cfg(feature = "bulletproof")]
use crate::proof::bulletproof::Bulletproof;
use crate::{
    proof::{p2wpkh::P2WPKHProof, p2wsh::P2WSHProof, PixelProof},
    EmptyPixelProof, LightningCommitmentProof, LightningHtlcProof, MultisigPixelProof, Pixel,
    PIXEL_SIZE,
};

/// Pixel proof flags
const P2WPKH_FLAG: u8 = 0u8;
const MULTISIG_FLAG: u8 = 1u8;
const LIGHTNING_FLAG: u8 = 2u8;
const LIGHTNING_HTLC_FLAG: u8 = 3u8;
#[cfg(feature = "bulletproof")]
const BULLETPROOF_FLAG: u8 = 4u8;
const EMPTY_PIXEL_FLAG: u8 = 5u8;
const P2WSH_FLAG: u8 = 6u8;

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
            PixelProof::Sig(proof) => {
                len += P2WPKH_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
            PixelProof::P2WSH(proof) => {
                len += P2WSH_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
            #[cfg(feature = "bulletproof")]
            PixelProof::Bulletproof(bulletproof) => {
                len += BULLETPROOF_FLAG.consensus_encode(writer)?;
                len += bulletproof.consensus_encode(writer)?;
            }
            PixelProof::EmptyPixel(proof) => {
                len += EMPTY_PIXEL_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
            PixelProof::Multisig(proof) => {
                len += MULTISIG_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
            PixelProof::Lightning(proof) => {
                len += LIGHTNING_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
            PixelProof::LightningHtlc(proof) => {
                len += LIGHTNING_HTLC_FLAG.consensus_encode(writer)?;
                len += proof.consensus_encode(writer)?;
            }
        }

        Ok(len)
    }
}

impl Decodable for PixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, EncodeError> {
        let flag: u8 = Decodable::consensus_decode(reader)?;

        match flag {
            P2WPKH_FLAG => {
                let proof: P2WPKHProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::Sig(proof))
            }
            P2WSH_FLAG => {
                let proof: P2WSHProof = Decodable::consensus_decode(reader)?;
                Ok(PixelProof::P2WSH(Box::new(proof)))
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
            _ => Err(EncodeError::ParseFailed("Unknown pixel proof")),
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

    #[cfg(feature = "bulletproof")]
    use crate::Bulletproof;
    use crate::LightningCommitmentProof;
    use crate::MultisigPixelProof;
    use crate::Pixel;
    use crate::PixelProof;
    use crate::SigPixelProof;
    use crate::{
        proof::common::lightning::{commitment::script::ToLocalScript, htlc},
        LightningHtlcData,
    };
    use crate::{Chroma, LightningHtlcProof};
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
            data: ToLocalScript::new(*PUBKEY, 100, *PUBKEY),
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
                data: ToLocalScript::new(*PUBKEY, 100, *PUBKEY),
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
