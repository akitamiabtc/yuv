extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use k256::elliptic_curve::{ff::Field, group::GroupEncoding};
use merlin::Transcript;
use rand::rngs::OsRng;

use crate::constants::{hash_to_point, to_point, G, G_BOLD1, H, H_BOLD1, RANGE_PROOF_SIZE};
use crate::vec_ops::VecOps;
use crate::wip::{WipProof, WipStmt};

/// Commit to a value with a blinding factor.
///
/// v * H + r * G
pub fn commit(v: k256::Scalar, r: k256::Scalar) -> k256::ProjectivePoint {
    (*H * v) + (*G * r)
}

/// Proof that a value is in the range [0, 2^128).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeProof {
    a: k256::ProjectivePoint,
    wip: WipProof,
}

impl RangeProof {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];
        bytes.extend_from_slice(self.a.to_bytes().as_ref());
        bytes.extend_from_slice(self.wip.to_bytes().as_ref());

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let a = to_point(&bytes[..33])?;
        let wip = WipProof::from_bytes(&bytes[33..])?;

        Some(Self { a, wip })
    }
}

impl core::hash::Hash for RangeProof {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write(self.a.to_bytes().as_ref());
        state.write(self.wip.to_bytes().as_ref());
    }
}

/// Calculate a point from a transcript and a.
fn calculate_point(
    a: k256::ProjectivePoint,
    label: &'static [u8],
    transcript: &mut Transcript,
) -> k256::Scalar {
    transcript.append_message(b"a", a.to_bytes().as_ref());

    let mut raw_point = [0u8; 128];
    transcript.challenge_bytes(label, &mut raw_point);

    hash_to_point(&raw_point)
}

/// Generate a range proof for a value with a blinding factor.
pub fn generate(value: u128, blinding: k256::Scalar) -> (RangeProof, k256::ProjectivePoint) {
    let mut transcript = Transcript::new(b"range_proof");

    // v * G + r * H
    let commit = commit(value.into(), blinding);

    // alpha = X: Ω -> R
    let alpha = k256::Scalar::random(&mut OsRng);

    let a_l = calculate_a_l(value);
    // a_r = { x - 1 | x ∈ a_l }
    let a_r = a_l.sub(&k256::Scalar::ONE);

    let a_terms = calculate_a_terms(alpha, &a_l, &a_r);

    // a = exp(a_terms)
    let a = multiexp::multiexp(&a_terms);

    // y = X: hash(a || v)
    let y = calculate_point(a, b"y", &mut transcript);
    // z = X: hash(a || v)
    let z = calculate_point(a, b"z", &mut transcript);

    let (two_descending_y, y_n_plus_one, a_hat) = calculate_a_hat(commit, a, y, z);

    // a_l = { x - z | x ∈ a_l}
    let a_l = a_l.sub(&z);
    let a_r = a_r.add_all(&two_descending_y.add(&z));
    // alpha = alpha + (r * y * n + 1)
    let alpha = alpha + (blinding * y_n_plus_one);

    let wip_stmt = WipStmt::new(a_hat, y);
    let proof = RangeProof {
        a,
        wip: wip_stmt.generate(a_l, a_r, alpha, &mut transcript),
    };

    (proof, commit)
}

/// Verify a range proof with a commitment.
pub(crate) fn verify(commit: k256::ProjectivePoint, proof: RangeProof) -> bool {
    let mut transcript = Transcript::new(b"range_proof");
    let mut verifier = multiexp::BatchVerifier::new(1);

    let v = commit;
    let y = calculate_point(proof.a, b"y", &mut transcript);
    let z = calculate_point(proof.a, b"z", &mut transcript);

    let (_, _, a_hat) = calculate_a_hat(v, proof.a, y, z);

    let stmt = WipStmt::new(a_hat, y);
    stmt.verify(proof.wip, &mut verifier, &mut transcript);

    verifier.verify_vartime()
}

/// A_l = {x| 0 <= x < 128, x = v >> i & 1}
fn calculate_a_l(value: u128) -> Vec<k256::Scalar> {
    let mut output = vec![];
    for i in 0..128 {
        output.push(k256::Scalar::from((value >> i) & 1));
    }

    output
}

/// Form a terms from parametes
fn calculate_a_terms(
    alpha: k256::Scalar,
    a_l: &[k256::Scalar],
    a_r: &[k256::Scalar],
) -> Vec<(k256::Scalar, k256::ProjectivePoint)> {
    let mut output = vec![];
    for (i, a_l) in a_l.iter().enumerate() {
        output.push((*a_l, H_BOLD1[i]));
    }
    for (i, a_r) in a_r.iter().enumerate() {
        output.push((*a_r, G_BOLD1[i]));
    }

    output.push((alpha, *G));

    output
}

/// Calculate a_hat seed parameters
fn calculate_a_hat(
    v: k256::ProjectivePoint,
    a: k256::ProjectivePoint,
    y: k256::Scalar,
    z: k256::Scalar,
) -> (Vec<k256::Scalar>, k256::Scalar, k256::ProjectivePoint) {
    // powers = [2, 2^2, 2^3, ..., 2^128]
    let powers = Vec::<k256::Scalar>::new_power(k256::Scalar::from(2u128), RANGE_PROOF_SIZE);

    // Y = [y, y*y, y*y*y, ..., y*y*...*y]
    let mut y_vec = vec![y];
    for i in 1..RANGE_PROOF_SIZE {
        y_vec.push(y_vec[i - 1] * y);
    }

    // -Y
    let y_vec_inv = y_vec.iter().rev().copied().collect::<Vec<_>>();

    // -Y * y
    let y_inv_mul = y_vec_inv[0] * y;

    // y = y + y*y + y*y*y + ... + y*y*...*y
    let y_sum = y_vec.iter().sum::<k256::Scalar>();

    let powers_y_inv = powers.mul_all(&y_vec_inv);
    let mut a_terms = Vec::with_capacity((RANGE_PROOF_SIZE * 2) + 2);
    for (i, scalar) in powers_y_inv.add(&z).drain(..).enumerate() {
        a_terms.push((-z, H_BOLD1[i]));
        a_terms.push((scalar, G_BOLD1[i]));
    }

    a_terms.push((y_inv_mul, v));

    // a_terms = y * z - (powers * -Y * y * z) - (y * z * 2)
    let last_term =
        (y_sum * z) - (powers.iter().sum::<k256::Scalar>() * y_inv_mul * z) - (y_sum * z.square());

    a_terms.push((last_term, *H));

    (
        powers_y_inv,
        y_inv_mul,
        a + multiexp::multiexp_vartime(&a_terms),
    )
}
