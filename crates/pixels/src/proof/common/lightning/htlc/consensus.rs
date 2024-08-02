use bitcoin::{
    consensus::{Decodable, Encodable},
    hashes::{hash160, Hash},
    key::constants::PUBLIC_KEY_SIZE,
    secp256k1,
};
use core2::io;

use crate::{HtlcScriptKind, LightningHtlcData, LightningHtlcProof, Pixel};

// hash160 size in bytes
const HASH_SIZE: usize = 20;

// Htlc script flags
const OFFERED_CONSENSUS_FLAG: u8 = 0u8;
const RECEIVED_CONSENSUS_FLAG: u8 = 1u8;

impl Encodable for LightningHtlcProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;
        len += self.data.consensus_encode(writer)?;

        Ok(len)
    }
}

impl Decodable for LightningHtlcProof {
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
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
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let mut bytes = [0u8; HASH_SIZE];
        reader.read_exact(&mut bytes)?;
        let revocation_key_hash = hash160::Hash::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the hash")
        })?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let remote_htlc_key = secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the public key")
        })?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let local_htlc_key = secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the hash")
        })?;

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
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let b: u8 = Decodable::consensus_decode(reader)?;

        match b {
            OFFERED_CONSENSUS_FLAG => Ok(HtlcScriptKind::Offered),
            RECEIVED_CONSENSUS_FLAG => {
                let cltv_expiry: u32 = Decodable::consensus_decode(reader)?;
                Ok(HtlcScriptKind::Received { cltv_expiry })
            }
            _ => Err(bitcoin::consensus::encode::Error::ParseFailed(
                "Unknown HTLC script kind",
            )),
        }
    }
}
