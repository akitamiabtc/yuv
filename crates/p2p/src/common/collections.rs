use bitcoin_hashes::siphash24::Hash;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

/// Hasher using `siphash24`.
#[derive(Default)]
pub struct Hasher {
    data: Vec<u8>,
    key1: u64,
    key2: u64,
}

impl Hasher {
    fn new(key1: u64, key2: u64) -> Self {
        Self {
            data: vec![],
            key1,
            key2,
        }
    }
}

impl std::hash::Hasher for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes)
    }

    fn finish(&self) -> u64 {
        Hash::hash_with_keys(self.key1, self.key2, &self.data).as_u64()
    }
}

/// Random hasher state.
#[derive(Default, Clone)]
pub struct RandomState {
    key1: u64,
    key2: u64,
}

impl RandomState {
    fn new(mut rng: fastrand::Rng) -> Self {
        Self {
            key1: rng.u64(..),
            key2: rng.u64(..),
        }
    }
}

impl std::hash::BuildHasher for RandomState {
    type Hasher = Hasher;

    fn build_hasher(&self) -> Self::Hasher {
        Hasher::new(self.key1, self.key2)
    }
}

impl From<fastrand::Rng> for RandomState {
    fn from(rng: fastrand::Rng) -> Self {
        Self::new(rng)
    }
}

/// A map with the ability to randomly select values.
#[derive(Debug)]
pub struct AddressBook<K, V> {
    inner: HashMap<K, V>,
}

impl<K, V> AddressBook<K, V> {
    /// Create a new address book.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl<K, V> Deref for AddressBook<K, V> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K, V> DerefMut for AddressBook<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
