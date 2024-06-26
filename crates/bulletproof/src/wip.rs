extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use k256::elliptic_curve::{ff::Field, ff::PrimeField, group::GroupEncoding};
use merlin::Transcript;
use rand::rngs::OsRng;

use super::{
    constants::{hash_to_point, to_point, to_scalar, G, G_BOLD1, H, H_BOLD1, RANGE_PROOF_SIZE},
    vec_ops::VecOps,
};

/// Weighted Inner Product statment
#[derive(Clone)]
pub struct WipStmt {
    a_hat: k256::ProjectivePoint,
    y: Vec<k256::Scalar>,
}

/// Indexed scalars used to simplify calculations
#[derive(Clone)]
struct IndexedScalars {
    inner: Vec<k256::Scalar>,
    positions: Vec<Vec<usize>>,
}

impl IndexedScalars {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: vec![k256::Scalar::ONE; capacity + (capacity % 2)],
            positions: (0..capacity).map(|i| vec![i]).collect(),
        }
    }

    fn next(&mut self, section: usize, scalar: k256::Scalar) {
        for i in (section * self.positions.len() / 2)..((section + 1) * self.positions.len() / 2) {
            for pos in &self.positions[i] {
                self.inner[*pos] *= scalar;
            }
        }
    }

    fn merge(&mut self) {
        let j = self.positions.len() / 2;
        for i in 1..j + 1 {
            let mut pos = self
                .positions
                .pop()
                .expect("positions should have at least one element");

            self.positions[j - i].append(&mut pos);
        }
    }
}

/// Weighted Inner Product proof
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WipProof {
    l: Vec<k256::ProjectivePoint>,
    r: Vec<k256::ProjectivePoint>,
    a: k256::ProjectivePoint,
    b: k256::ProjectivePoint,
    r_rev: k256::Scalar,
    s_rev: k256::Scalar,
    delta_rev: k256::Scalar,
}

impl WipProof {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![];

        bytes.extend(self.a.to_bytes());
        bytes.extend(self.b.to_bytes());
        bytes.extend(self.r_rev.to_repr());
        bytes.extend(self.s_rev.to_repr());
        bytes.extend(self.delta_rev.to_repr());

        for (l, r) in self.l.iter().zip(self.r.iter()) {
            bytes.extend(l.to_bytes());
            bytes.extend(r.to_bytes());
        }

        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let a = to_point(&data[..33])?;
        let b = to_point(&data[33..66])?;

        let r_answer = to_scalar(&data[66..98])?;
        let s_answer = to_scalar(&data[98..130])?;
        let delta_answer = to_scalar(&data[130..162])?;

        let mut l = vec![];
        let mut r = vec![];
        let mut offset = 162;
        while offset < data.len() {
            l.push(to_point(&data[offset..offset + 33])?);
            r.push(to_point(&data[offset + 33..offset + 66])?);

            offset += 66;
        }

        Some(Self {
            l,
            r,
            a,
            b,
            r_rev: r_answer,
            s_rev: s_answer,
            delta_rev: delta_answer,
        })
    }
}

impl WipStmt {
    pub(crate) fn new(a_hat: k256::ProjectivePoint, y: k256::Scalar) -> Self {
        let mut y_vec = Vec::<k256::Scalar>::with_cap(RANGE_PROOF_SIZE);

        y_vec[0] = y;
        for i in 1..y_vec.len() {
            y_vec[i] = y_vec[i - 1] * y;
        }

        Self { a_hat, y: y_vec }
    }

    /// iteratively calculate the next generation of points
    #[allow(clippy::too_many_arguments)]
    fn next_gens(
        g_bold1: Vec<k256::ProjectivePoint>,
        g_bold2: Vec<k256::ProjectivePoint>,
        h_bold1: Vec<k256::ProjectivePoint>,
        h_bold2: Vec<k256::ProjectivePoint>,
        l: k256::ProjectivePoint,
        r: k256::ProjectivePoint,
        y_n_hat_inv: k256::Scalar,
        transcript: &mut Transcript,
    ) -> (
        k256::Scalar,
        Vec<k256::ProjectivePoint>,
        Vec<k256::ProjectivePoint>,
    ) {
        let p = sync_points(l, r, transcript);
        let p_inv = p.invert().expect("p should be invertible");

        let mut g_bolds = vec![];
        for g_bold in g_bold1.into_iter().zip(g_bold2.into_iter()) {
            g_bolds.push(multiexp::multiexp_vartime(&[
                (p_inv, g_bold.0),
                (p * y_n_hat_inv, g_bold.1),
            ]));
        }

        let mut h_bolds = vec![];
        for h_bold in h_bold1.into_iter().zip(h_bold2.into_iter()) {
            h_bolds.push(multiexp::multiexp_vartime(&[
                (p, h_bold.0),
                (p_inv, h_bold.1),
            ]));
        }

        (p, g_bolds, h_bolds)
    }

    /// iteratively calculate the next pairs of points
    fn next_pairs(
        g_bold: &mut IndexedScalars,
        h_bold: &mut IndexedScalars,
        p_terms: &mut Vec<(k256::Scalar, k256::ProjectivePoint)>,
        l: k256::ProjectivePoint,
        r: k256::ProjectivePoint,
        y_n_hat_inv: k256::Scalar,
        transcript: &mut Transcript,
    ) {
        let p = sync_points(l, r, transcript);
        let p_inv = p.invert().expect("p should be invertible");

        g_bold.next(0, p_inv);
        g_bold.next(1, p * y_n_hat_inv);
        h_bold.next(0, p);
        h_bold.next(1, p_inv);

        g_bold.merge();
        h_bold.merge();

        let e_square = p.square();
        let inv_e_square = p_inv.square();
        p_terms.push((e_square, l));
        p_terms.push((inv_e_square, r));
    }

    /// Generate a weighted inner product proof
    pub fn generate(
        self,
        a_l: Vec<k256::Scalar>,
        a_r: Vec<k256::Scalar>,
        alpha: k256::Scalar,
        transcript: &mut Transcript,
    ) -> WipProof {
        let mut y = self.y;
        let h = *H;
        let g = *G;

        let mut h_bold = H_BOLD1.clone();
        let mut g_bold = G_BOLD1.clone();

        let mut a_l = a_l;
        let mut a_r = a_r;
        let mut alpha = alpha;

        let mut l_vec = vec![];
        let mut r_vec = vec![];
        while h_bold.len() > 1 {
            let (a_1, a_2) = a_l.clone().split();
            let (b_1, b_2) = a_r.clone().split();
            let (h_bold1, h_bold2) = split_points(h_bold);
            let (g_bold1, g_bold2) = split_points(g_bold);

            let n_hat = h_bold1.len();

            let y_n_hat = y[n_hat - 1];
            y.shrink_to(n_hat);

            let d_l = k256::Scalar::random(&mut OsRng);
            let d_r = k256::Scalar::random(&mut OsRng);

            let c_l = a_1.wip(&b_2, &y);
            let c_r = a_2.mul(&y_n_hat).wip(&b_1, &y);

            let y_n_hat_inv = y_n_hat.invert().expect("y_n_hat should be invertible");

            let mut l_terms = a_1
                .mul(&y_n_hat_inv)
                .drain(..)
                .zip(h_bold2.iter().copied())
                .chain(b_2.iter().copied().zip(g_bold1.iter().copied()))
                .collect::<Vec<_>>();
            l_terms.push((c_l, h));
            l_terms.push((d_l, g));
            let l = multiexp::multiexp(&l_terms);
            l_vec.push(l);

            let mut r_terms = a_2
                .mul(&y_n_hat)
                .drain(..)
                .zip(h_bold1.iter().copied())
                .chain(b_1.iter().copied().zip(g_bold2.iter().copied()))
                .collect::<Vec<_>>();
            r_terms.push((c_r, h));
            r_terms.push((d_r, g));
            let r = multiexp::multiexp(&r_terms);
            r_vec.push(r);

            let p;
            (p, h_bold, g_bold) = Self::next_gens(
                h_bold1,
                h_bold2,
                g_bold1,
                g_bold2,
                l,
                r,
                y_n_hat_inv,
                transcript,
            );
            let p_inv = p.invert().expect("p should be invertible");
            let p_square = p.square();
            let p_square_inv = p_inv.square();

            a_l = a_1.mul(&p).add_all(&a_2.mul(&(y_n_hat * p_inv)));
            a_r = b_1.mul(&p_inv).add_all(&b_2.mul(&p));
            alpha += (d_l * p_square) + (d_r * p_square_inv);
        }

        let r = k256::Scalar::random(&mut OsRng);
        let s = k256::Scalar::random(&mut OsRng);
        let delta = k256::Scalar::random(&mut OsRng);
        let n = k256::Scalar::random(&mut OsRng);

        let r_y = r * y[0];

        let a_l_terms = vec![
            (r, h_bold[0]),
            (s, g_bold[0]),
            ((r_y * a_r[0]) + (s * y[0] * a_l[0]), h),
            (delta, g),
        ];

        let a = multiexp::multiexp(&a_l_terms);

        let a_r_terms = vec![(r_y * s, h), (n, g)];

        let b = multiexp::multiexp(&a_r_terms);

        let p = sync_points(a, b, transcript);

        WipProof {
            l: l_vec,
            r: r_vec,
            a,
            b,
            r_rev: r + (a_l[0] * p),
            s_rev: s + (a_r[0] * p),
            delta_rev: n + (delta * p) + (alpha * p.square()),
        }
    }

    /// Verify a weighted inner product proof
    pub fn verify(
        self,
        proof: WipProof,
        verifier: &mut multiexp::BatchVerifier<(), k256::ProjectivePoint>,
        transcript: &mut Transcript,
    ) {
        let mut a_hat_terms = vec![(k256::Scalar::ONE, self.a_hat)];

        let mut indexed_g_bold = IndexedScalars::with_capacity(RANGE_PROOF_SIZE);
        let mut indexed_h_bold = IndexedScalars::with_capacity(RANGE_PROOF_SIZE);

        proof.l.iter().zip(proof.r.iter()).for_each(|(l, r)| {
            let n_hat = (indexed_g_bold.positions.len() + (indexed_g_bold.positions.len() % 2)) / 2;
            let y_n_hat = self.y[n_hat - 1];
            let y_n_hat_inv = y_n_hat.invert().expect("y_n_hat should be invertible");

            Self::next_pairs(
                &mut indexed_g_bold,
                &mut indexed_h_bold,
                &mut a_hat_terms,
                *l,
                *r,
                y_n_hat_inv,
                transcript,
            )
        });

        let a_b = sync_points(proof.a, proof.b, transcript);
        let mut multiexp = a_hat_terms;
        for (scalar, _) in multiexp.iter_mut() {
            *scalar *= -a_b.square();
        }

        for i in 0..RANGE_PROOF_SIZE {
            multiexp.push((indexed_g_bold.inner[i] * proof.r_rev * a_b, H_BOLD1[i]));
        }

        for i in 0..RANGE_PROOF_SIZE {
            multiexp.push((indexed_h_bold.inner[i] * proof.s_rev * a_b, G_BOLD1[i]));
        }

        multiexp.push((-a_b, proof.a));
        multiexp.push((proof.r_rev * self.y[0] * proof.s_rev, *H));
        multiexp.push((proof.delta_rev, *G));
        multiexp.push((-k256::Scalar::ONE, proof.b));

        verifier.queue(&mut OsRng, (), multiexp);
    }
}

pub fn split_points(
    mut points: Vec<k256::ProjectivePoint>,
) -> (Vec<k256::ProjectivePoint>, Vec<k256::ProjectivePoint>) {
    let mut right = points.split_off((points.len() / 2) + (points.len() % 2));
    while right.len() < points.len() {
        right.push(k256::ProjectivePoint::IDENTITY);
    }

    (points, right)
}

fn sync_points(
    a: k256::ProjectivePoint,
    b: k256::ProjectivePoint,
    transcript: &mut Transcript,
) -> k256::Scalar {
    transcript.append_message(b"a", a.to_bytes().as_ref());
    transcript.append_message(b"b", b.to_bytes().as_ref());

    let mut raw_e = [0u8; 128];
    transcript.challenge_bytes(b"e", &mut raw_e);

    hash_to_point(&raw_e)
}
