use bitcoin::{
    consensus::{encode::Error, Decodable, Encodable},
    secp256k1::{constants::PUBLIC_KEY_SIZE, PublicKey},
};
use core2::io;

use super::P2WSHProof;

impl Encodable for P2WSHProof {
    fn consensus_encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut len = self.pixel.consensus_encode(writer)?;

        len += writer.write(&self.inner_key.serialize())?;

        len += self.script.consensus_encode(writer)?;

        Ok(len)
    }
}

impl Decodable for P2WSHProof {
    fn consensus_decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let pixel = Decodable::consensus_decode(reader)?;

        let mut buf = [0u8; PUBLIC_KEY_SIZE];
        reader.read_exact(&mut buf)?;
        let inner_key = PublicKey::from_slice(&buf)
            .map_err(|_err| Error::ParseFailed("Failed to parse public key bytes"))?;

        let script = Decodable::consensus_decode(reader)?;

        Ok(P2WSHProof {
            pixel,
            inner_key,
            script,
        })
    }
}
