//! Provides general trait for all entities that can be tweaked by pixel.
//!
//! For example, [`PublicKey`] can be tweaked by [`PixelKey`], [`SecretKey`]
//! can be tweaked to [`PixelPrivateKey`].

use bitcoin::secp256k1::{PublicKey, SecretKey};

use crate::{PixelHash, PixelKey, PixelPrivateKey};

/// For entities that can be tweaked by pixel.
///
/// For example, [`PublicKey`] can be tweaked into [`PublicKey`]:
///
/// ```
/// use yuv_pixels::Tweakable;
/// use std::str::FromStr;
/// use yuv_pixels::PixelKey;
///
/// let pubkey = bitcoin::PublicKey::from_str(
///    "036a5e3a83f0b2bdfb2f874c6f4679dc02568deb8987d11314a36bceacb569ad8e",
/// ).expect("Should be valid public key");
///
/// let pixel = yuv_pixels::Pixel::new(100, pubkey);
///
/// let tweaked: bitcoin::secp256k1::PublicKey = pubkey.inner.tweak(pixel);
/// ```
///
/// The same for [`SecretKey`]:
///
/// ```
/// use std::str::FromStr;
/// use yuv_pixels::Tweakable;
/// use bitcoin::secp256k1::SecretKey;
/// use bitcoin::secp256k1::Secp256k1;
///
/// let ctx = Secp256k1::new();
///
/// let private_key = bitcoin::PrivateKey::from_str(
///     "cUrMc62nnFeQuzXb26KPizCJQPp7449fsPsqn5NCHTwahSvqqRkV"
/// ).expect("Should be valid private key");
///
/// let pubkey = private_key.public_key(&ctx);
///
/// let pixel = yuv_pixels::Pixel::new(100, pubkey);
///
/// let tweaked: SecretKey = private_key.inner.tweak(pixel);
/// ```
pub trait Tweakable<P: Into<PixelHash>> {
    fn tweak(self, pixel: P) -> Self
    where
        Self: Sized;

    fn maybe_tweak(self, optional_pixel: Option<P>) -> Self
    where
        Self: Sized,
    {
        if let Some(pixel) = optional_pixel {
            return self.tweak(pixel);
        }

        self
    }
}

const EXPECT_MSG: &str = "Error will encounter only in rear cases of memory corruption";

impl<P> Tweakable<P> for PublicKey
where
    P: Into<PixelHash>,
{
    fn tweak(self, pixel: P) -> PublicKey {
        let key: PixelKey = PixelKey::new(pixel, &self).expect(EXPECT_MSG);

        key.0.inner
    }
}

impl<P> Tweakable<P> for SecretKey
where
    P: Into<PixelHash>,
{
    fn tweak(self, pixel: P) -> SecretKey {
        let seckey = PixelPrivateKey::new(pixel, &self).expect(EXPECT_MSG);

        seckey.0
    }
}

impl<P> Tweakable<P> for bitcoin::PublicKey
where
    P: Into<PixelHash>,
{
    fn tweak(self, pixel: P) -> Self
    where
        Self: Sized,
    {
        let tweaked = self.inner.tweak(pixel);

        bitcoin::PublicKey::new(tweaked)
    }
}

impl<P> Tweakable<P> for bitcoin::PrivateKey
where
    P: Into<PixelHash>,
{
    fn tweak(self, pixel: P) -> Self
    where
        Self: Sized,
    {
        let tweaked = self.inner.tweak(pixel);

        bitcoin::PrivateKey::new(tweaked, self.network)
    }
}
