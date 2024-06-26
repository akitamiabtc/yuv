use bitcoin::hashes::{sha256::Hash as Sha256Hash, Hash, HashEngine};
use bitcoin::secp256k1::Parity;
use bitcoin::ScriptBuf;
use bitcoin::{
    self,
    secp256k1::{self, Scalar, Secp256k1, Signing, Verification},
    PrivateKey, PublicKey,
};
use core::ops::Deref;

use crate::errors::PixelKeyError;
use crate::PixelHash;

/// Public key that can spend a pixel.
///
/// Defined as: `PXK = hash(PXH, Pk) * G + P_{B}`,
/// where `Pk` is owner's public key (coin inner key).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PixelKey(pub PublicKey);

impl Deref for PixelKey {
    type Target = PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PixelKey {
    pub fn new(
        pxh: impl Into<PixelHash>,
        inner_key: &secp256k1::PublicKey,
    ) -> Result<Self, PixelKeyError> {
        let ctx = Secp256k1::new();

        Self::new_with_ctx(pxh, inner_key, &ctx)
    }

    pub fn new_with_ctx<C>(
        pxh: impl Into<PixelHash>,
        inner_key: &secp256k1::PublicKey,
        ctx: &Secp256k1<C>,
    ) -> Result<Self, PixelKeyError>
    where
        C: Signing + Verification,
    {
        // If the public key is odd, change its parity to even.
        let mut inner_key = *inner_key;
        let (xonly, parity) = inner_key.x_only_public_key();
        if parity == Parity::Odd {
            inner_key = xonly.public_key(Parity::Even);
        };

        // hash(PXH, P_{B})
        let pxh_b = pixelhash_pubkey_hash(&pxh.into(), &inner_key);

        // Convert `hash(PXH, P_{B})` into scalar for next operation.
        let pxh_b = Scalar::from_be_bytes(*pxh_b.as_byte_array())?;

        // P_{B} + hash(PXH, P_{B}) * G (where G - generator point).
        //
        // `add_exp_tweak` multiplies by G the hash (scalar).
        let pxk = inner_key.add_exp_tweak(ctx, &pxh_b)?;

        Ok(Self(PublicKey::new(pxk)))
    }

    pub fn to_p2wpkh(&self) -> Option<ScriptBuf> {
        let pubkey_hash = self.0.wpubkey_hash()?;

        Some(ScriptBuf::new_v0_p2wpkh(&pubkey_hash))
    }
}

/// Calculates: `sha256(PXH || Pk)`
///
/// where `PXH` - hash of the pixel (see [`PixelHash`]),
///       `Pk` - public key of current owner.
fn pixelhash_pubkey_hash(pxh: &PixelHash, pubkey: &secp256k1::PublicKey) -> Sha256Hash {
    let mut hash_engine = Sha256Hash::engine();

    // By putting hash and key after each other into "hash engine",
    // the "engine" will hash the concatenation.
    hash_engine.input(pxh.as_byte_array());
    hash_engine.input(&pubkey.serialize());

    Sha256Hash::from_engine(hash_engine)
}

/// Private key that can spend a YUV UTXO.
///
/// Defined as: `Sk_{B} + hash(PXH || Pk)`, where `Sk_{B}` - is
/// a secret key of current owner of the coin, `PXH` is
/// [`PixelHash`], and `Pk` is derived from `Sk` public key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PixelPrivateKey(pub secp256k1::SecretKey);

impl PixelPrivateKey {
    pub fn new(
        pxh: impl Into<PixelHash>,
        inner_key: &secp256k1::SecretKey,
    ) -> Result<Self, PixelKeyError> {
        let ctx = Secp256k1::signing_only();

        Self::new_with_ctx(pxh, inner_key, &ctx)
    }

    /// Create [`PixelPrivateKey`] from [`PixelHash`] and secret key of the YUV UTXO owner.
    ///
    /// `ctx` is required if you want to be sure that operations are done
    /// only in secure parts of the memory. Otherwise use [`Self::new`].
    pub fn new_with_ctx<C>(
        pxh: impl Into<PixelHash>,
        inner_key: &secp256k1::SecretKey,
        ctx: &Secp256k1<C>,
    ) -> Result<Self, PixelKeyError>
    where
        C: Signing,
    {
        // If the public key is Odd, negate the secret key.
        let mut inner_key = *inner_key;
        let mut pubkey = inner_key.public_key(ctx);
        let (_, parity) = pubkey.x_only_public_key();
        if parity == Parity::Odd {
            inner_key = inner_key.negate();
            pubkey = inner_key.public_key(ctx);
        }

        // hash(PXH, P_{B})
        let pxh_b = pixelhash_pubkey_hash(&pxh.into(), &pubkey);

        // Convert `hash(PXH, P_{B})` into scalar for next operation.
        let pxh_b = Scalar::from_be_bytes(*pxh_b.as_byte_array())?;

        // (Sk_{B} + hash(PXH, P_{B})) mod P, where `P` curve order.
        //
        // `add_tweak` also does the `mod P` operation
        let spending_key = inner_key.add_tweak(&pxh_b)?;

        Ok(Self(spending_key))
    }
}

/// This traits adds ability to types from external libraries to always return
/// public key with even parity.
pub trait ToEvenPublicKey {
    fn even_public_key<C>(&self, ctx: &Secp256k1<C>) -> PublicKey
    where
        C: Signing;
}

impl ToEvenPublicKey for PrivateKey {
    fn even_public_key<C>(&self, ctx: &Secp256k1<C>) -> PublicKey
    where
        C: Signing,
    {
        let pubkey = self.public_key(ctx);

        pubkey.even_public_key(ctx)
    }
}

impl ToEvenPublicKey for PublicKey {
    fn even_public_key<C>(&self, _ctx: &Secp256k1<C>) -> PublicKey
    where
        C: Signing,
    {
        let (xonly, parity) = self.inner.x_only_public_key();

        match parity {
            Parity::Even => *self,
            Parity::Odd => PublicKey::new(secp256k1::PublicKey::from_x_only_public_key(
                xonly,
                Parity::Even,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use bitcoin::secp256k1::SecretKey;
    use bitcoin::{secp256k1::Secp256k1, PublicKey};
    use once_cell::sync::Lazy;

    use crate::{Pixel, PixelKey, PixelPrivateKey};

    static ISSUER: Lazy<PublicKey> = Lazy::new(|| {
        PublicKey::from_str("036a5e3a83f0b2bdfb2f874c6f4679dc02568deb8987d11314a36bceacb569ad8e")
            .expect("Should be valid public key")
    });

    static RECIPIENT_SECRET: Lazy<SecretKey> = Lazy::new(|| {
        SecretKey::from_str("f9e17ee5b837fece0695f9782253604586ab1daf42ecf2762573243c7a6979f4")
            .expect("Should be valid secret")
    });

    #[test]
    fn test_derived_public_key_eq_pxk() {
        let pixel = Pixel::new(100, *ISSUER);

        let ctx = Secp256k1::new();

        let pxk = PixelKey::new_with_ctx(pixel, &RECIPIENT_SECRET.public_key(&ctx), &ctx).unwrap();

        let pxsk = PixelPrivateKey::new_with_ctx(pixel, &RECIPIENT_SECRET, &ctx).unwrap();

        let derived = pxsk.0.public_key(&ctx);

        assert_eq!(
            derived, pxk.0.inner,
            "derived from private key, and public key got from hash MUST be equal"
        );
    }

    #[test]
    fn test_pixel_key() {
        let p = Pixel::new(100, *ISSUER);

        let pixel_key = PixelKey::new(p, &ISSUER.inner).unwrap();

        assert!(pixel_key.to_p2wpkh().is_some());
    }

    /// Provided uncompressed public key to pixel key
    #[test]
    fn test_pixel_key_uncompressed() {
        let p = Pixel::new(100, *ISSUER);

        let mut pixel_key = PixelKey::new(p, &ISSUER.inner).unwrap();
        pixel_key.0 = PublicKey::new_uncompressed(ISSUER.inner);

        assert!(pixel_key.to_p2wpkh().is_none());
    }
}
