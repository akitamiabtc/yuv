#[macro_use]
extern crate afl;

use bitcoin::hashes::sha256d::Hash;
use bitcoin::hashes::Hash as BitcoinHash;
use bitcoin::secp256k1::rand::rngs::StdRng;
use bitcoin::secp256k1::rand::SeedableRng;
use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey, ThirtyTwoByteHash};
use once_cell::sync::Lazy;
use std::str::FromStr;
use yuv_pixels::{Chroma, Pixel, PixelHash, PixelKey, PixelPrivateKey};

static ISSUER: Lazy<PublicKey> = Lazy::new(|| {
    PublicKey::from_str("02ef156c4ebfbf48fc4849915f65dc46a782ee837c7efd834e9d24d975d07784b8")
        .expect("Should be valid public key")
});

fn pad_to_seed_length(input: &[u8]) -> [u8; 32] {
    let mut result = [0; 32];

    let copy_length = input.len().min(32);
    result[..copy_length].copy_from_slice(&input[..copy_length]);

    result
}

pub fn generate_keypair(data: &[u8]) -> (SecretKey, PublicKey) {
    let seed = {
        let mut bytes: [u8; 32] = [0; 32];
        if data.len() > 32 {
            bytes.copy_from_slice(&Hash::hash(data).into_32());
        } else {
            bytes.copy_from_slice(&pad_to_seed_length(data));
        }
        bytes
    };

    let mut rng = StdRng::from_seed(seed);
    let secp = Secp256k1::new();

    secp.generate_keypair(&mut rng)
}

fn main() {
    let pixel = Pixel::new(100, &ISSUER.clone().into());
    let ctx = Secp256k1::new();

    fuzz!(|data: &[u8]| {
        let (recipient_priv_key, recipient_pub_key) = generate_keypair(data);

        let pxk = PixelKey::new_with_ctx(pixel, &recipient_pub_key, &ctx).unwrap();

        let pxsk = PixelPrivateKey::new_with_ctx(pixel, &recipient_priv_key, &ctx).unwrap();

        let derived = pxsk.0.public_key(&ctx);

        if !derived.eq(&pxk.0.inner) {
            panic!("public key derived from the private key MUST be equal to the public key got from the hash");
        };
    });

    fuzz!(|data: &[u8]| {
        let (_priv_key, pub_key) = generate_keypair(data);

        let (xonly, _parity) = pub_key.x_only_public_key();

        let pixel = Pixel::new(100, Chroma::from(xonly));

        if let Err(e) = PixelKey::new(PixelHash::from(pixel), &pub_key) {
            panic!("failed to create pixel key: {}", e);
        }
    });
}
