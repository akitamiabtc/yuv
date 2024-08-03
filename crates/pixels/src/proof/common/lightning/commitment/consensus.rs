use bitcoin::{
    consensus::{Decodable, Encodable},
    key::constants::PUBLIC_KEY_SIZE,
    secp256k1,
};
use core2::io;

use crate::{LightningCommitmentProof, Pixel};

use super::script::ToLocalScript;

impl Encodable for LightningCommitmentProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;

        len += self.pixel.consensus_encode(writer)?;

        len += writer.write(&self.data.revocation_pubkey.serialize())?;

        len += self.data.to_self_delay.consensus_encode(writer)?;

        len += writer.write(&self.data.local_delayed_pubkey.serialize())?;

        Ok(len)
    }
}

impl Decodable for LightningCommitmentProof {
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let revocation_pubkey = secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the public key")
        })?;

        let to_self_delay: u16 = Decodable::consensus_decode(reader)?;

        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let local_delayed_pubkey = secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the public key")
        })?;

        Ok(LightningCommitmentProof {
            pixel,
            data: ToLocalScript::new(revocation_pubkey, to_self_delay, local_delayed_pubkey),
        })
    }
}
