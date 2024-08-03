use alloc::vec::Vec;

use bitcoin::{
    hashes::{sha256, Hash},
    secp256k1::{self, schnorr::Signature, All, Secp256k1},
    PrivateKey, PublicKey,
};
use bulletproof::{k256::ProjectivePoint, util::ecdh, RangeProof};
use hashbrown::{hash_map::Entry, HashMap};

use crate::{Bulletproof, BulletproofError, Chroma};

pub struct CommitmentResult {
    pub proof: RangeProof,
    pub commitment: ProjectivePoint,
    pub proof_hash: [u8; 32],
}

/// Tweak the general signing key and chroma signing keys with the `ecdh` secret keys
/// derived using both inputs and outputs.
pub fn tweak_signing_keys(
    private_key: PrivateKey,
    bulletproof: &Bulletproof,
    network: bitcoin::Network,
    signing_key: &mut Option<secp256k1::SecretKey>,
    chroma_signing_keys: &mut HashMap<Chroma, secp256k1::SecretKey>,
    recipients: Vec<(PublicKey, u128)>,
    commitments: &mut Vec<(Chroma, CommitmentResult)>,
) -> Result<(), BulletproofError> {
    let input_dh_key = ecdh(private_key, PublicKey::new(bulletproof.sender_key), network)
        .map_err(|_e| BulletproofError::InvalidRangeProof)?;

    // For the inputs, we combine the secret keys.
    tweak(
        &secp256k1::Scalar::from_be_bytes(input_dh_key.inner.secret_bytes())
            .map_err(|_e| BulletproofError::InvalidRangeProof)?,
        signing_key,
        chroma_signing_keys,
        bulletproof.pixel.chroma,
        input_dh_key,
    )?;

    // Tweak the signing keys with the output keys.
    for (recipient, amount) in recipients {
        let (
            dh_key,
            CommitmentResult {
                proof,
                commitment,
                proof_hash,
            },
        ) = get_commitment(private_key, recipient, network, amount)?;

        // For the outputs, we negate the secret keys.
        tweak(
            &secp256k1::Scalar::from_be_bytes(dh_key.inner.negate().secret_bytes())
                .map_err(|_e| BulletproofError::InvalidRangeProof)?,
            signing_key,
            chroma_signing_keys,
            bulletproof.pixel.chroma,
            input_dh_key,
        )?;

        // Push the result to `commitments` that will be used to construct bulletproof builder outputs.
        commitments.push((
            bulletproof.pixel.chroma,
            CommitmentResult {
                proof,
                commitment,
                proof_hash,
            },
        ));
    }

    Ok(())
}

/// Generate a ECDH point using the given `PrivateKey` and `PublicKey` and generate a commitment for the specidied amount.
pub fn get_commitment(
    private_key: PrivateKey,
    public_key: PublicKey,
    network: bitcoin::Network,
    amount: u128,
) -> Result<(PrivateKey, CommitmentResult), BulletproofError> {
    let dh_key =
        ecdh(private_key, public_key, network).map_err(|_e| BulletproofError::InvalidRangeProof)?;
    let raw_dh_key: [u8; 32] = dh_key
        .to_bytes()
        .as_slice()
        .try_into()
        .map_err(|_e| BulletproofError::InvalidRangeProof)?;

    let (proof, commitment) = bulletproof::generate(amount, raw_dh_key);
    let proof_hash = bulletproof::util::proof_hash(commitment, proof.clone());

    Ok((
        dh_key,
        CommitmentResult {
            proof,
            commitment,
            proof_hash,
        },
    ))
}

/// Generate the general signature and chroma signatures.
pub fn create_signatures(
    ctx: &Secp256k1<All>,
    signing_key: secp256k1::SecretKey,
    chroma_signing_keys: &HashMap<Chroma, secp256k1::SecretKey>,
    engine: sha256::HashEngine,
    chroma_engines: &HashMap<Chroma, sha256::HashEngine>,
) -> Result<(Signature, HashMap<Chroma, Signature>), BulletproofError> {
    // Construct a general Schnorr signature for all the proofs.
    let signature = ctx.sign_schnorr(
        &secp256k1::Message::from_hashed_data::<sha256::Hash>(
            sha256::Hash::from_engine(engine).as_byte_array(),
        ),
        &secp256k1::KeyPair::from_secret_key(ctx, &signing_key),
    );

    // Construct Schnorr signatures for each Chroma.
    let mut chroma_signatures: HashMap<Chroma, Signature> = HashMap::new();
    for (chroma, signing_key) in chroma_signing_keys {
        let engine = chroma_engines
            .get(chroma)
            .ok_or(BulletproofError::InvalidRangeProof)?;

        let chroma_signature = ctx.sign_schnorr(
            &secp256k1::Message::from_hashed_data::<sha256::Hash>(
                sha256::Hash::from_engine(engine.clone()).as_byte_array(),
            ),
            &secp256k1::KeyPair::from_secret_key(ctx, signing_key),
        );

        chroma_signatures.insert(*chroma, chroma_signature);
    }

    Ok((signature, chroma_signatures))
}

fn tweak(
    tweak: &secp256k1::Scalar,
    signing_key: &mut Option<secp256k1::SecretKey>,
    chroma_signing_keys: &mut HashMap<Chroma, secp256k1::SecretKey>,
    chroma: Chroma,
    dh_key: PrivateKey,
) -> Result<(), BulletproofError> {
    *signing_key = if let Some(sk) = signing_key {
        Some(
            sk.add_tweak(tweak)
                .map_err(|_e| BulletproofError::InvalidRangeProof)?,
        )
    } else {
        Some(dh_key.inner)
    };
    match chroma_signing_keys.entry(chroma) {
        Entry::Occupied(mut entry) => {
            entry.insert(
                entry
                    .get()
                    .add_tweak(tweak)
                    .map_err(|_e| BulletproofError::InvalidRangeProof)?,
            );
        }
        Entry::Vacant(entry) => {
            entry.insert(dh_key.inner);
        }
    }
    Ok(())
}
