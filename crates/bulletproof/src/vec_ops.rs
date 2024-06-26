extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

/// Advanced vector operations
pub trait VecOps
where
    Self: Sized,
{
    type Value;

    /// Create a vector with a given capacity.
    fn with_cap(len: usize) -> Self;
    /// Create a vector of powers of a value.
    fn new_power(v: Self::Value, len: usize) -> Self;

    /// Add two vectors together.
    fn add_all(&self, vec: &Self) -> Self;
    /// Multiply two vectors together.
    fn mul_all(&self, vec: &Self) -> Self;

    /// Add a value to each element of a vector.
    fn add(&self, v: &Self::Value) -> Self;
    /// Subtract a value from each element of a vector.
    fn sub(&self, v: &Self::Value) -> Self;
    /// Multiply each element of a vector by a value.
    fn mul(&self, v: &Self::Value) -> Self;

    /// Calculate the weighted inner product of three vectors.
    fn wip(&self, vec1: &Self, vec2: &Self) -> Self::Value;
    /// Calculate the inner product of two vectors.
    fn inner_product(&self, vec: &Self) -> Self::Value;
    /// Split a vector into two vectors.
    fn split(&mut self) -> (Self, Self);
}

impl VecOps for Vec<k256::Scalar> {
    type Value = k256::Scalar;

    fn with_cap(len: usize) -> Self {
        vec![k256::Scalar::ZERO; len]
    }

    fn new_power(v: Self::Value, len: usize) -> Self {
        let mut output = vec![k256::Scalar::ONE, v];

        for i in 2..len {
            output.push(output[i - 1] * v);
        }

        output.shrink_to(len);

        output
    }

    fn add_all(&self, vec: &Self) -> Self {
        self.iter()
            .zip(vec)
            .map(|(v1, v2)| v1 + v2)
            .collect::<Vec<_>>()
    }

    fn mul_all(&self, vec: &Self) -> Self {
        self.iter()
            .zip(vec)
            .map(|(v1, v2)| v1 * v2)
            .collect::<Vec<_>>()
    }

    fn add(&self, v: &Self::Value) -> Self {
        self.iter().map(|v_i| v_i + v).collect::<Vec<_>>()
    }

    fn sub(&self, v: &Self::Value) -> Self {
        self.iter().map(|v_i| v_i - v).collect::<Vec<_>>()
    }

    fn mul(&self, v: &Self::Value) -> Self {
        self.iter().map(|v_i| v_i * v).collect::<Vec<_>>()
    }

    fn wip(&self, vec1: &Self, vec2: &Self) -> Self::Value {
        self.iter()
            .zip(vec1)
            .zip(vec2)
            .map(|((v1, v2), v3)| v1 * v2 * v3)
            .sum()
    }

    fn inner_product(&self, vec: &Self) -> Self::Value {
        self.iter().zip(vec).map(|(v1, v2)| v1 * v2).sum()
    }

    fn split(&mut self) -> (Self, Self) {
        let mut r = self.split_off((self.len() / 2) + (self.len() % 2));

        while r.len() < self.len() {
            r.push(k256::Scalar::ZERO);
        }

        (self.clone(), r)
    }
}
