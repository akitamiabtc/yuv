//! Shared peer types.

use std::net;
use std::net::SocketAddr;

use bitcoin::network::{address::Address, constants::ServiceFlags};

use crate::net::time::LocalTime;

/// Peer store.
///
/// Used to store peer addresses and metadata.
pub trait Store {
    /// Get a known peer address.
    fn get(&self, addr: &SocketAddr) -> Option<&KnownAddress>;

    /// Get a known peer address mutably.
    fn get_mut(&mut self, addr: &SocketAddr) -> Option<&mut KnownAddress>;

    /// Insert a *new* address into the store. Returns `true` if the address was inserted,
    /// or `false` if it was already known.
    fn insert(&mut self, addr: &SocketAddr, ka: KnownAddress) -> bool;

    /// Remove an address from the store.
    fn remove(&mut self, addr: &SocketAddr) -> Option<KnownAddress>;

    /// Return an iterator over the known addresses.
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&SocketAddr, &KnownAddress)> + 'a>;

    /// Returns the number of addresses.
    fn len(&self) -> usize;

    /// Returns true if there are no addresses.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears the store of all addresses.
    fn clear(&mut self);
}

/// Implementation of [`Store`] for [`std::collections::HashMap`].
impl Store for std::collections::HashMap<SocketAddr, KnownAddress> {
    fn get(&self, ip: &SocketAddr) -> Option<&KnownAddress> {
        self.get(ip)
    }

    fn get_mut(&mut self, ip: &SocketAddr) -> Option<&mut KnownAddress> {
        self.get_mut(ip)
    }

    fn insert(&mut self, addr: &SocketAddr, ka: KnownAddress) -> bool {
        use ::std::collections::hash_map::Entry;

        match self.entry(*addr) {
            Entry::Vacant(v) => {
                v.insert(ka);
            }
            Entry::Occupied(_) => return false,
        }
        true
    }

    fn remove(&mut self, addr: &SocketAddr) -> Option<KnownAddress> {
        self.remove(addr)
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&SocketAddr, &KnownAddress)> + 'a> {
        Box::new(self.iter())
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn clear(&mut self) {
        self.clear()
    }
}

/// Address source. Specifies where an address originated from.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Source {
    /// An address that was shared by another peer.
    Peer(net::SocketAddr),
    /// An address that came from a DNS seed.
    Dns,
    /// An address that came from some source external to the system, eg.
    /// specified by the user or added directly to the address manager.
    Imported,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer(addr) => write!(f, "{}", addr),
            Self::Dns => write!(f, "DNS"),
            Self::Imported => write!(f, "Imported"),
        }
    }
}

/// A known address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownAddress {
    /// Network address.
    pub addr: Address,
    /// Address of the peer who sent us this address.
    pub source: Source,
    /// Last time this address was used to successfully connect to a peer.
    pub last_success: Option<LocalTime>,
    /// Last time this address was sampled.
    pub last_sampled: Option<LocalTime>,
    /// Last time this address was tried.
    pub last_attempt: Option<LocalTime>,
    /// Last time this peer was seen alive.
    pub last_active: Option<LocalTime>,
}

impl KnownAddress {
    /// Create a new known address.
    pub fn new(addr: Address, source: Source, last_active: Option<LocalTime>) -> Self {
        Self {
            addr,
            source,
            last_success: None,
            last_attempt: None,
            last_sampled: None,
            last_active,
        }
    }
}

/// Source of peer addresses.
pub trait AddressSource {
    /// Sample a random peer address. Returns `None` if there are no addresses left.
    fn sample(&mut self, services: ServiceFlags) -> Option<(Address, Source)>;
    /// Sample peer with provided predicate.
    fn sample_with(
        &mut self,
        predicate: impl Fn(&KnownAddress) -> bool,
    ) -> Option<(Address, Source)>;
    /// Record an address of ours as seen by a remote peer.
    fn record_local_address(&mut self, addr: net::SocketAddr);
    fn is_connected(&mut self, addr: net::SocketAddr) -> bool;
    /// Return an iterator over random peer addresses.
    fn iter(&mut self, services: ServiceFlags) -> Box<dyn Iterator<Item = (Address, Source)> + '_>;
    fn insert(&mut self, addrs: impl IntoIterator<Item = (u32, Address)>, source: Source);
}
