//! A pure-Rust implementation of Bulletproofs using Secp256k1.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

// TODO: get rid of k256. Only secp256k1 and own scalar implementation can be used.
pub use k256;
use k256::{elliptic_curve::ff::PrimeField, FieldBytes, ProjectivePoint, PublicKey, Scalar};

pub use range_proof::RangeProof;

mod constants;
mod range_proof;
pub mod util;
mod vec_ops;
mod wip;

/// Generate a range proof for a value with a blinding factor.
pub fn generate(value: u128, blinding: [u8; 32]) -> (RangeProof, ProjectivePoint) {
    let blinding =
        Scalar::from_repr(*FieldBytes::from_slice(&blinding)).expect("blinding is a valid scalar");

    range_proof::generate(value, blinding)
}

/// Verify a range proof with a commitment.
pub fn verify(commit: ProjectivePoint, proof: RangeProof) -> bool {
    range_proof::verify(commit, proof)
}

/// Commit to a value with a blinding factor.
///
/// v * G + r * H
pub fn commit(value: u128, blinding: [u8; 32]) -> ProjectivePoint {
    let v = Scalar::from(value);
    let r =
        Scalar::from_repr(*FieldBytes::from_slice(&blinding)).expect("blinding is a valid scalar");

    range_proof::commit(v, r)
}

/// Verify that the sum of the commitments is equal to the verifier.
pub fn verify_commits(commitments: Vec<ProjectivePoint>, verifier: PublicKey) -> bool {
    if commitments.is_empty() {
        return false;
    }

    let mut sum = *commitments.first().expect("commitments is not empty");
    for commitment in &commitments[1..] {
        sum -= commitment;
    }

    let origin = PublicKey::from_affine(sum.to_affine()).expect("sum is a valid point");

    origin == verifier
}

#[cfg(test)]
mod tests {
    use super::range_proof::RangeProof;

    #[test]
    fn test_verification() {
        let value = 100;
        let binding = [
            91, 142, 202, 76, 120, 107, 124, 118, 58, 31, 122, 166, 94, 187, 20, 158, 221, 153, 23,
            84, 31, 168, 120, 136, 12, 190, 32, 249, 110, 174, 65, 2,
        ];

        let (proof, commit) = super::generate(value, binding);

        assert!(super::verify(commit, proof.clone()));

        let se_proof = proof.to_bytes();

        let de_proof = RangeProof::from_bytes(&se_proof).expect("proof is a valid range proof");

        assert_eq!(proof, de_proof);

        let wrong_commit = super::commit(101, binding);

        assert!(!super::verify(wrong_commit, proof));
    }
}
