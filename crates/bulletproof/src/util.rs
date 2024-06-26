use bitcoin::{
    hashes::{sha256, Hash, HashEngine},
    Network, PrivateKey, PublicKey,
};

use k256::{
    elliptic_curve::{group::GroupEncoding, sec1::FromEncodedPoint},
    ProjectivePoint,
};

use crate::RangeProof;

/// `HKDF_SALT` is salt used for HKDF, which is a simple key derivation function (KDF) based on the HMAC message authentication code.
const HKDF_SALT: &[u8] = b"43f905cb425b135f2ec3671bffd6643b8b8239fc8db5c529339f41c7d29bff5a";
const HKDF_INFO: &[u8] = b"ecdh key agreement";

pub fn ecdh(key: PrivateKey, pub_key: PublicKey, network: Network) -> eyre::Result<PrivateKey> {
    let key = k256::SecretKey::from_slice(&key.to_bytes())?;

    let encoded_pub_key = k256::EncodedPoint::from_bytes(pub_key.to_bytes())?;
    let pub_key =
        k256::PublicKey::from_encoded_point(&encoded_pub_key).expect("failed to create public key");

    let result_key = ecdh_inner(key, pub_key)?;

    Ok(PrivateKey::from_slice(&result_key.to_bytes(), network)
        .expect("Private key should be valid"))
}

fn ecdh_inner(key: k256::SecretKey, pub_key: k256::PublicKey) -> eyre::Result<k256::SecretKey> {
    let scalar = key.to_nonzero_scalar();

    let shared_secret = k256::ecdh::diffie_hellman(&scalar, pub_key.as_affine());

    let hkdf = shared_secret.extract::<sha2::Sha256>(Some(HKDF_SALT));

    let mut data = [0u8; 32];

    hkdf.expand(HKDF_INFO, &mut data)
        .expect("failed to expand hkdf");

    Ok(k256::SecretKey::from_slice(&data)?)
}

pub fn proof_hash(commitment: ProjectivePoint, proof: RangeProof) -> [u8; 32] {
    let mut hash_engine = sha256::Hash::engine();

    hash_engine.input(&commitment.to_bytes());
    hash_engine.input(&proof.to_bytes());

    *sha256::Hash::from_engine(hash_engine).as_byte_array()
}
