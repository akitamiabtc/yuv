use alloc::vec::Vec;
use bitcoin::{
    consensus::{Decodable, Encodable},
    key::constants::PUBLIC_KEY_SIZE,
    secp256k1,
};
use core2::io;

use crate::{MultisigPixelProof, Pixel};

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
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let pixel: Pixel = Decodable::consensus_decode(reader)?;

        let number_of_keys: u32 = Decodable::consensus_decode(reader)?;

        let inner_keys: Vec<secp256k1::PublicKey> = (0..number_of_keys)
            .map(|_i| {
                let mut bytes = [0u8; PUBLIC_KEY_SIZE];
                reader.read_exact(&mut bytes).map_err(|_| {
                    bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the public key")
                })?;
                secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
                    bitcoin::consensus::encode::Error::ParseFailed(
                        "Failed to create public key from bytes",
                    )
                })
            })
            .collect::<Result<_, _>>()?;

        let m: u8 = Decodable::consensus_decode(reader)?;

        Ok(MultisigPixelProof::new(pixel, inner_keys, m))
    }
}
