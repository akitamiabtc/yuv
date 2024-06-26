//! Client-related peer functionality.
use crate::common::peer::{KnownAddress, Store};
use std::{collections::HashMap, net::SocketAddr};

#[derive(Debug)]
pub struct Cache(HashMap<SocketAddr, KnownAddress>);

impl Cache {
    /// Create a new cache.
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for Cache {
    fn get(&self, ip: &SocketAddr) -> Option<&KnownAddress> {
        self.0.get(ip)
    }

    fn get_mut(&mut self, ip: &SocketAddr) -> Option<&mut KnownAddress> {
        self.0.get_mut(ip)
    }

    fn insert(&mut self, ip: &SocketAddr, known_address: KnownAddress) -> bool {
        <HashMap<_, _> as Store>::insert(&mut self.0, ip, known_address)
    }

    fn remove(&mut self, ip: &SocketAddr) -> Option<KnownAddress> {
        self.0.remove(ip)
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&SocketAddr, &KnownAddress)> + 'a> {
        Box::new(self.0.iter())
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn clear(&mut self) {
        self.0.clear()
    }
}
