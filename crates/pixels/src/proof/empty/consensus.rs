use bitcoin::{
    consensus::{Decodable, Encodable},
    key::constants::PUBLIC_KEY_SIZE,
    secp256k1,
};
use core2::io;

use crate::EmptyPixelProof;

impl Encodable for EmptyPixelProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        writer.write(&self.inner_key.serialize())
    }
}

impl Decodable for EmptyPixelProof {
    fn consensus_decode<R: io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let mut bytes = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut bytes)?;
        let inner_key = secp256k1::PublicKey::from_slice(&bytes).map_err(|_| {
            bitcoin::consensus::encode::Error::ParseFailed("Failed to parse the public key")
        })?;

        Ok(EmptyPixelProof::new(inner_key))
    }
}
