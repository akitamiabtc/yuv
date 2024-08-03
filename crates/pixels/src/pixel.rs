use core::fmt;
use core::{fmt::Display, mem::size_of, str::FromStr};

use bitcoin::address::{Payload, WitnessProgram, WitnessVersion};
use bitcoin::secp256k1::constants::SCHNORR_PUBLIC_KEY_SIZE;
use bitcoin::secp256k1::Parity;
use bitcoin::{key::XOnlyPublicKey, secp256k1, Address, Network, PublicKey};
use once_cell::sync::Lazy;

use crate::errors::{ChromaParseError, LumaParseError, PixelParseError};

/// The size of the [`Luma`] in bytes.
pub const LUMA_SIZE: usize = 32;

pub const AMOUNT_SIZE: usize = size_of::<u128>();

pub const BLINDING_FACTOR_SIZE: usize = LUMA_SIZE - AMOUNT_SIZE;

/// Size of serialized [`XOnlyPublicKey`] under the hood.
pub const CHROMA_SIZE: usize = 32;

/// Result size of serialized [`Pixel`].
pub const PIXEL_SIZE: usize = LUMA_SIZE + CHROMA_SIZE;

pub const ZERO_PUBKEY_BYTES: &[u8] = &[0x02; 33];

pub static ZERO_PUBLIC_KEY: Lazy<PublicKey> =
    Lazy::new(|| PublicKey::from_slice(ZERO_PUBKEY_BYTES).expect("Pubkey should be valid"));

/// Represents amount of tokens in the [`Pixel`].
///
/// The result size is 256 bits. The first 64 are for token amount, another 192
/// bits will be used as _blinding factor_ for future features.
#[derive(Clone, Debug, Copy, Hash, Default, PartialEq, Eq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Luma {
    pub amount: u128,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "is_default_blinding_factor")
    )]
    pub blinding_factor: [u8; BLINDING_FACTOR_SIZE],
}

#[cfg(feature = "serde")]
fn is_default_blinding_factor(blinding_factor: &[u8; BLINDING_FACTOR_SIZE]) -> bool {
    blinding_factor.iter().all(|&b| b == 0)
}

impl From<u128> for Luma {
    fn from(amount: u128) -> Self {
        Self {
            amount,
            ..Default::default()
        }
    }
}

impl From<[u8; LUMA_SIZE]> for Luma {
    fn from(bytes: [u8; LUMA_SIZE]) -> Self {
        Self::from_array(bytes)
    }
}

impl Luma {
    pub fn new(amount: u128, blinding_factor: [u8; BLINDING_FACTOR_SIZE]) -> Self {
        Self {
            amount,
            blinding_factor,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LumaParseError> {
        if bytes.len() < LUMA_SIZE {
            return Err(LumaParseError::InvalidSize(bytes.len()));
        }

        let bytes: [u8; LUMA_SIZE] = bytes[..LUMA_SIZE]
            .try_into()
            .expect("As we checked the bytes size, slice should always convert");

        Ok(Self::from_array(bytes))
    }

    pub fn from_array(bytes: [u8; LUMA_SIZE]) -> Self {
        // TODO: check if we want big-endian, or little-endian.
        let amount = u128::from_be_bytes(
            bytes[0..AMOUNT_SIZE]
                .try_into()
                .expect("Converting [u8; 32] to [u8; 16] should always success"),
        );

        let blinding_factor = bytes[AMOUNT_SIZE..]
            .try_into()
            .expect("Converting [u8; 32] to [u8; 16] should always success");

        Self {
            amount,
            blinding_factor,
        }
    }

    pub fn to_bytes(&self) -> [u8; LUMA_SIZE] {
        let mut buf: [u8; LUMA_SIZE] = [0u8; LUMA_SIZE];

        // TODO: check if want to use big-endian or little-endian.
        buf[..AMOUNT_SIZE].copy_from_slice(&self.amount.to_be_bytes());
        buf[AMOUNT_SIZE..].copy_from_slice(&self.blinding_factor);

        buf
    }
}

/// Represensts the asset type of the YUV token and is defined by X
/// coordinate of issuer's public key.
#[derive(Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Chroma(XOnlyPublicKey);

impl FromStr for Chroma {
    type Err = ChromaParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let xonly = XOnlyPublicKey::from_str(s)?;

        Ok(Self::new(xonly))
    }
}

impl Display for Chroma {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Chroma {
    pub fn new(xonly: XOnlyPublicKey) -> Self {
        Self(xonly)
    }

    pub fn xonly(&self) -> &XOnlyPublicKey {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; CHROMA_SIZE] {
        self.0.serialize()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ChromaParseError> {
        if bytes.len() < CHROMA_SIZE {
            return Err(ChromaParseError::InvalidSize(bytes.len()));
        }

        Ok(Self(XOnlyPublicKey::from_slice(bytes)?))
    }

    pub fn to_address(&self, network: Network) -> Address {
        let program = self.0.serialize();

        Address::new(
            network,
            Payload::WitnessProgram(
                WitnessProgram::new(WitnessVersion::V1, program.to_vec())
                    .expect("Should be valid program"),
            ),
        )
    }

    pub fn from_address(address: &str) -> Result<Self, ChromaParseError> {
        let address =
            Address::from_str(address).map_err(|_| ChromaParseError::InvalidAddressType)?;

        let (version, program) = match &address.payload {
            Payload::WitnessProgram(program) => (program.version(), program.program()),
            _ => return Err(ChromaParseError::InvalidAddressType),
        };

        if version != WitnessVersion::V1 {
            return Err(ChromaParseError::InvalidWitnessProgramVersion(version));
        }

        if program.len() != SCHNORR_PUBLIC_KEY_SIZE {
            return Err(ChromaParseError::InvalidWitnessProgramLength(program.len()));
        }

        let xonly = XOnlyPublicKey::from_slice(program.as_bytes())?;

        Ok(Self::new(xonly))
    }

    pub fn public_key(&self) -> PublicKey {
        // NOTE: We consider using only even parity as it's described so in
        // taproot BIP
        PublicKey::new(secp256k1::PublicKey::from_x_only_public_key(
            self.0,
            Parity::Even,
        ))
    }
}

impl From<PublicKey> for Chroma {
    fn from(public_key: PublicKey) -> Self {
        let (xonly, _parity) = public_key.inner.x_only_public_key();

        Self(xonly)
    }
}

impl From<XOnlyPublicKey> for Chroma {
    fn from(xonly: XOnlyPublicKey) -> Self {
        Self(xonly)
    }
}

impl From<&XOnlyPublicKey> for Chroma {
    fn from(xonly: &XOnlyPublicKey) -> Self {
        Self(*xonly)
    }
}

impl From<&Chroma> for XOnlyPublicKey {
    fn from(chroma: &Chroma) -> Self {
        chroma.0
    }
}

impl From<Chroma> for XOnlyPublicKey {
    fn from(chroma: Chroma) -> Self {
        chroma.0
    }
}

/// Pixel and it's data that participates in a transaction.
#[derive(Clone, Debug, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pixel {
    pub luma: Luma,
    pub chroma: Chroma,
}

impl Pixel {
    pub fn new(luma: impl Into<Luma>, chroma: impl Into<Chroma>) -> Self {
        Self {
            luma: luma.into(),
            chroma: chroma.into(),
        }
    }

    pub fn empty() -> Self {
        let zero_pubkey = PublicKey::from_slice(ZERO_PUBKEY_BYTES).expect("Pubkey should be valid");

        Self::new(0, zero_pubkey)
    }

    pub fn to_bytes(&self) -> [u8; PIXEL_SIZE] {
        let mut buf = [0u8; PIXEL_SIZE];

        buf[..LUMA_SIZE].copy_from_slice(&self.luma.to_bytes());
        buf[LUMA_SIZE..PIXEL_SIZE].copy_from_slice(&self.chroma.to_bytes());

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PixelParseError> {
        if bytes.len() < PIXEL_SIZE {
            return Err(PixelParseError::IncorrectSize(bytes.len()));
        }

        let luma = Luma::from_bytes(&bytes[0..LUMA_SIZE])?;
        let chroma = Chroma::from_bytes(&bytes[LUMA_SIZE..PIXEL_SIZE])?;

        Ok(Self { luma, chroma })
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use once_cell::sync::Lazy;

    use super::*;

    static X_ONLY_PUBKEY: Lazy<XOnlyPublicKey> = Lazy::new(|| {
        XOnlyPublicKey::from_str("0677b5829356bb5e0c0808478ac150a500ceab4894d09854b0f75fbe7b4162f8")
            .expect("Should be valid address")
    });

    #[test]
    fn test_luma_parsing() {
        let luma = Luma::from(100);

        let luma_as_bytes = luma.to_bytes();

        assert_eq!(
            luma,
            Luma::from_bytes(&luma_as_bytes).unwrap(),
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_chroma_parsing_bytes() {
        let chroma = Chroma::from(*X_ONLY_PUBKEY);

        let chroma_as_bytes = chroma.to_bytes();

        assert_eq!(
            chroma,
            Chroma::from_bytes(&chroma_as_bytes).unwrap(),
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_chroma_parsing_address() {
        let chroma = Chroma::from(*X_ONLY_PUBKEY);

        let address = chroma.to_address(Network::Bitcoin);

        assert_eq!(
            chroma,
            Chroma::from_address(&address.to_string()).unwrap(),
            "Converting back and forth should work"
        );
    }

    #[test]
    fn test_pixel_parsing() {
        let pixel = Pixel::new(100, *X_ONLY_PUBKEY);

        let pixel_as_bytes = pixel.to_bytes();

        assert_eq!(
            pixel,
            Pixel::from_bytes(&pixel_as_bytes).unwrap(),
            "Converting back and forth should work"
        );
    }
}
