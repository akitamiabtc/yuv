//!
//! The peer-to-peer address manager.
//!
use std::collections::{HashMap, HashSet};
use std::net;
use std::net::SocketAddr;
use tracing::trace;

use crate::{
    common::peer::{AddressSource, KnownAddress, Source, Store},
    common::time::Clock,
    fsm,
    fsm::output::{Io, Outbox},
    net::{Disconnect, LocalDuration, LocalTime},
};

use super::output::{SetTimer, Wire};
use bitcoin::network::address::Address;
use bitcoin::network::constants::ServiceFlags;

/// Time to wait until a request times out.
pub const REQUEST_TIMEOUT: LocalDuration = LocalDuration::from_mins(1);

/// Idle timeout. Used to run periodic functions.
pub const IDLE_TIMEOUT: LocalDuration = LocalDuration::from_mins(1);

/// Sample timeout. How long before a sampled address can be returned again.
pub const SAMPLE_TIMEOUT: LocalDuration = LocalDuration::from_mins(3);

/// Maximum number of addresses expected in a `addr` message.
const MAX_ADDR_ADDRESSES: usize = 1000;
/// Maximum number of addresses we store for a given address range.
const MAX_RANGE_SIZE: usize = 256;

/// An event emitted by the address manager.
#[derive(Debug, Clone)]
pub enum Event {
    /// Peer addresses have been received.
    AddressesReceived {
        /// Number of addresses received.
        count: usize,
        /// Source of addresses received.
        source: Source,
    },
    /// Address book exhausted.
    AddressBookExhausted,
    /// An error was encountered.
    Error(String),
}

impl std::fmt::Display for Event {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::AddressesReceived { count, source } => {
                write!(
                    fmt,
                    "received {} addresse(s) from source `{}`",
                    count, source
                )
            }
            Event::AddressBookExhausted => {
                write!(
                    fmt,
                    "Address book exhausted.. fetching new addresses from peers"
                )
            }
            Event::Error(msg) => {
                write!(fmt, "error: {}", msg)
            }
        }
    }
}

/// Iterator over addresses.
pub struct Iter<F>(F);

impl<F> Iterator for Iter<F>
where
    F: FnMut() -> Option<(Address, Source)>,
{
    type Item = (Address, Source);

    fn next(&mut self) -> Option<Self::Item> {
        (self.0)()
    }
}

impl<P: Store, U, C> AddressManager<P, U, C> {
    /// Check whether we have unused addresses.
    pub fn is_exhausted(&self) -> bool {
        let time = self
            .last_idle
            .expect("AddressManager::is_exhausted: manager must be initialized");

        for (addr, ka) in self.peers.iter() {
            // Unsuccessful attempt to connect.
            if ka.last_attempt.is_some() && ka.last_success.is_none() {
                continue;
            }
            if time - ka.last_sampled.unwrap_or_default() < SAMPLE_TIMEOUT {
                continue;
            }
            if !self.connected.contains(addr) {
                return false;
            }
        }
        true
    }
}

/// Address manager configuration.
#[derive(Debug)]
pub struct Config {
    /// Services required from peers.
    pub required_services: ServiceFlags,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            required_services: ServiceFlags::NONE,
        }
    }
}

/// Manages peer network addresses.
#[derive(Debug)]
pub struct AddressManager<P, U, C> {
    /// Peer address store.
    peers: P,
    bans: HashSet<net::IpAddr>,
    address_ranges: HashMap<u8, HashSet<SocketAddr>>,
    connected: HashSet<SocketAddr>,
    sources: HashSet<net::SocketAddr>,
    local_addrs: HashSet<net::SocketAddr>,
    /// The last time we asked our peers for new addresses.
    last_request: Option<LocalTime>,
    /// The last time we idled.
    last_idle: Option<LocalTime>,
    outbox: Outbox,
    upstream: U,
    rng: fastrand::Rng,
    clock: C,
}

impl<P, U, C> Iterator for AddressManager<P, U, C> {
    type Item = Io;

    fn next(&mut self) -> Option<Self::Item> {
        self.outbox.next()
    }
}

impl<P: Store, U: Wire<Event> + SetTimer, C: Clock> AddressManager<P, U, C> {
    pub fn initialize(&mut self) {
        self.idle();
    }

    /// Return an iterator over randomly sampled addresses.
    pub fn iter(&mut self, services: ServiceFlags) -> impl Iterator<Item = (Address, Source)> + '_ {
        Iter(move || self.sample(services))
    }

    /// Get addresses from peers.
    pub fn get_addresses(&mut self) {
        for peer in &self.sources {
            self.upstream.get_addr(*peer);
        }
    }

    /// Called when we receive a `getaddr` message.
    pub fn received_getaddr(&mut self, from: &net::SocketAddr) {
        // TODO: We should only respond with peers who were last active within
        // the last 3 hours.
        let mut addrs = Vec::new();

        // Include one random address per address range.
        for range in self.address_ranges.values() {
            let ix = self.rng.usize(..range.len());
            let ip = range.iter().nth(ix).expect("index must be present");
            let ka = self.peers.get(ip).expect("address must exist");

            addrs.push((
                ka.last_active
                    .map(|t| t.as_secs() as u32)
                    .unwrap_or_default(),
                ka.addr.clone(),
            ));
        }
        self.upstream.addr(*from, addrs);
    }

    /// Called when a tick is received.
    pub fn received_wake(&mut self) {
        let local_time = self.clock.local_time();

        trace!("Received wake");

        // If we're already using all the addresses we have available, we should fetch more.
        if local_time - self.last_request.unwrap_or_default() >= REQUEST_TIMEOUT
            && self.is_exhausted()
        {
            self.outbox
                .event(fsm::event::Event::Address(Event::AddressBookExhausted));

            self.get_addresses();
            self.last_request = Some(local_time);
            self.outbox.set_timer(REQUEST_TIMEOUT);
        }

        if local_time - self.last_idle.unwrap_or_default() >= IDLE_TIMEOUT {
            self.idle();
        }
    }

    /// Called when a peer signaled activity.
    pub fn peer_active(&mut self, addr: net::SocketAddr) {
        let time = self.clock.local_time();
        if let Some(ka) = self.peers.get_mut(&addr) {
            ka.last_active = Some(time);
        }
    }

    /// Called when a peer connection is attempted.
    pub fn peer_attempted(&mut self, addr: &SocketAddr) {
        let time = self.clock.local_time();
        // We're only interested in connection attempts for addresses we keep track of.
        if let Some(ka) = self.peers.get_mut(addr) {
            ka.last_attempt = Some(time);
        }
    }

    /// Called when a peer has connected.
    pub fn peer_connected(&mut self, addr: &SocketAddr) {
        self.insert(
            vec![(
                LocalTime::now().as_secs() as u32,
                Address::new(addr, ServiceFlags::NONE),
            )],
            Source::Peer(*addr),
        );
        self.populate_address_ranges(addr);
        self.connected.insert(*addr);
    }

    /// Called when a peer has handshaked.
    pub fn peer_negotiated(&mut self, addr: &SocketAddr, services: ServiceFlags) {
        let time = self.clock.local_time();

        self.sources.insert(*addr);

        // We're only interested in peers we already know, eg. from DNS or peer
        // exchange. Peers should only be added to our address book if they are DNS seeds
        // or are discovered via a DNS seed.
        if let Some(ka) = self.peers.get_mut(addr) {
            // Only ask for addresses when connecting for the first time.
            if ka.last_success.is_none() {
                <Outbox as Wire<Event>>::get_addr(&mut self.outbox, *addr);
            }
            // Keep track of when the last successful handshake was.
            ka.last_success = Some(time);
            ka.last_active = Some(time);
            ka.addr.services = services;
        }
    }

    /// Called when a peer disconnected.
    pub fn peer_disconnected(
        &mut self,
        addr: &SocketAddr,
        reason: Disconnect<crate::fsm::handler::DisconnectReason>,
    ) {
        if self.connected.remove(addr) {
            // Disconnected peers cannot be used as a source for new addresses.
            self.sources.remove(addr);

            // If the reason for disconnecting the peer suggests that we shouldn't try to
            // connect to this peer again, then remove the peer from the address book.
            // Otherwise, we leave it in the address buckets so that it can be chosen
            // in the future.
            if let Disconnect::StateMachine(r) = reason {
                if !r.is_transient() {
                    self.ban(addr);
                }
            } else if reason.is_dial_err() || reason.is_banned() {
                self.ban(addr);
            }
        }
    }

    fn idle(&mut self) {
        // If it's been a while, save addresses to store.
        self.last_idle = Some(self.clock.local_time());
        self.outbox.set_timer(IDLE_TIMEOUT);
    }
}

impl<P: Store, U: Wire<Event>, C: Clock> AddressManager<P, U, C> {
    /// Create a new, empty address manager.
    pub fn new(rng: fastrand::Rng, peers: P, upstream: U, clock: C) -> Self {
        let addrs = peers.iter().map(|(addr, _)| *addr).collect::<Vec<_>>();
        let mut addrmgr = Self {
            peers,
            bans: HashSet::new(),
            address_ranges: HashMap::new(),
            connected: HashSet::new(),
            sources: HashSet::new(),
            local_addrs: HashSet::new(),
            last_request: None,
            last_idle: None,
            outbox: Outbox::default(),
            upstream,
            rng,
            clock,
        };

        for addr in addrs.iter() {
            addrmgr.populate_address_ranges(addr);
        }
        addrmgr
    }

    /// The number of peers known.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether there are any peers known to the address manager.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty() || self.address_ranges.is_empty()
    }

    /// Whether peen is banned
    pub fn is_banned(&self, addr: &SocketAddr) -> bool {
        self.bans.contains(&addr.ip())
    }

    /// Called when we received an `addr` message from a peer.
    pub fn received_addr(&mut self, peer: net::SocketAddr, addrs: Vec<(u32, Address)>) {
        if addrs.is_empty() || addrs.len() > MAX_ADDR_ADDRESSES {
            // Peer misbehaving, got empty message or too many addresses.
            return;
        }
        let source = Source::Peer(peer);

        self.upstream.event(Event::AddressesReceived {
            count: addrs.len(),
            source,
        });
        self.insert(addrs, source);
    }

    /// Add addresses to the address manager. The input matches that of the `addr` message
    /// sent by peers on the network.
    pub fn insert(&mut self, addrs: impl IntoIterator<Item = (u32, Address)>, source: Source) {
        let time = self
            .last_idle
            .expect("AddressManager::insert: manager must be initialized before inserting");
        for (last_active, addr) in addrs {
            // Ignore addresses that don't have a "last active" time.
            if last_active == 0 {
                continue;
            }

            // Ignore addresses that are too far into the future.
            if LocalTime::from_secs(last_active as u64) > time + LocalDuration::from_mins(60) {
                continue;
            }

            let Ok(socket_addr) = addr.socket_addr() else {
                continue;
            };

            // No banned addresses.
            if self.bans.contains(&socket_addr.ip()) {
                continue;
            }

            // No local addresses.
            if self.local_addrs.contains(&socket_addr) {
                continue;
            }

            // Record the address, and ignore addresses we already know.
            // Note that this should never overwrite an existing address.
            if !self.peers.insert(
                &addr.socket_addr().unwrap(),
                KnownAddress::new(addr.clone(), source, None),
            ) {
                continue;
            }

            self.populate_address_ranges(&socket_addr);
        }
    }

    /// Pick an address at random from the set of known addresses.
    ///
    /// This function tries to ensure a good geo-diversity of addresses, such that an adversary
    /// controlling a disproportionately large number of addresses in the same address range does
    /// not have an advantage over other peers.
    ///
    /// This works under the assumption that adversaries are *localized*.
    pub fn sample(&mut self, services: ServiceFlags) -> Option<(Address, Source)> {
        self.sample_with(|ka: &KnownAddress| {
            if !ka.addr.services.has(services) {
                match ka.source {
                    Source::Dns => {
                        // If we've negotiated with this peer and it hasn't signaled the
                        // required services, we know not to return it.
                        // DNS-sourced addresses don't include service information,
                        // so we won't be including these until we know the services.
                    }
                    Source::Imported => {
                        // We expect that imported addresses will always include the correct
                        // service information. Hence, if this one doesn't have the necessary
                        // services, it's safe to skip.
                    }
                    Source::Peer(_) => {
                        // Peer-sourced addresses come with service information. It's safe to
                        // skip this address if it doesn't have the required services.
                    }
                }
                return false;
            }
            true
        })
    }

    /// Sample an address using the provided predicate. Only returns addresses which are `true`
    /// according to the predicate.
    pub fn sample_with(
        &mut self,
        predicate: impl Fn(&KnownAddress) -> bool,
    ) -> Option<(Address, Source)> {
        if self.is_empty() {
            return None;
        }
        let time = self
            .last_idle
            .expect("AddressManager::sample: manager must be initialized before sampling");

        let mut ranges: Vec<_> = self.address_ranges.values().collect();
        self.rng.shuffle(&mut ranges);

        // First select a random address range.
        for range in ranges.drain(..) {
            assert!(!range.is_empty());

            let mut ips: Vec<_> = range.iter().collect();
            self.rng.shuffle(&mut ips);

            // Then select a random address in that range.
            for ip in ips.drain(..) {
                let ka = self.peers.get_mut(ip).expect("address must exist");

                // If the address was already attempted unsuccessfully, skip it.
                if ka.last_attempt.is_some() && ka.last_success.is_none() {
                    continue;
                }
                // If we recently sampled this address, don't return it again.
                if time - ka.last_sampled.unwrap_or_default() < SAMPLE_TIMEOUT {
                    continue;
                }
                // If we're already connected to this address, skip it.
                if self.connected.contains(ip) {
                    continue;
                }
                // If the provided filter doesn't pass, keep looking.
                if !predicate(ka) {
                    continue;
                }
                // Ok, we've found a worthy address!
                ka.last_sampled = Some(time);

                return Some((ka.addr.clone(), ka.source));
            }
        }

        None
    }

    ////////////////////////////////////////////////////////////////////////////

    /// Populate address ranges with an IP. This may remove an existing IP if
    /// its range is full. Returns the range key that was used.
    fn populate_address_ranges(&mut self, addr: &SocketAddr) -> u8 {
        let key = addr_key(&addr.ip());
        let range = self.address_ranges.entry(key).or_default();

        // If the address range is already full, remove a random address
        // before inserting this new one.
        if range.len() == MAX_RANGE_SIZE {
            let ix = self.rng.usize(..range.len());
            let addr = range
                .iter()
                .cloned()
                .nth(ix)
                .expect("the range is not empty");

            range.remove(&addr);
            self.peers.remove(&addr);
        }
        range.insert(*addr);
        key
    }

    /// Remove an address from the address book and prevent it from being sampled again.
    fn ban(&mut self, addr: &SocketAddr) -> bool {
        let key = addr_key(&addr.ip());

        if let Some(range) = self.address_ranges.get_mut(&key) {
            range.remove(addr);

            // TODO: Persist bans.
            self.peers.remove(addr);
            self.bans.insert(addr.ip());

            if range.is_empty() {
                self.address_ranges.remove(&key);
            }
            return true;
        }
        false
    }
}

impl<P: Store, U: Wire<Event> + SetTimer, C: Clock> AddressSource for AddressManager<P, U, C> {
    fn sample(&mut self, services: ServiceFlags) -> Option<(Address, Source)> {
        AddressManager::sample(self, services)
    }

    fn sample_with(
        &mut self,
        predicate: impl Fn(&KnownAddress) -> bool,
    ) -> Option<(Address, Source)> {
        AddressManager::sample_with(self, predicate)
    }

    fn record_local_address(&mut self, addr: net::SocketAddr) {
        self.local_addrs.insert(addr);
    }

    fn is_connected(&mut self, addr: net::SocketAddr) -> bool {
        return self.local_addrs.contains(&addr)
            || self.peers.get(&addr).is_some()
            || self.connected.contains(&addr)
            || self.address_ranges.contains_key(&addr_key(&addr.ip()));
    }

    fn iter(&mut self, services: ServiceFlags) -> Box<dyn Iterator<Item = (Address, Source)> + '_> {
        Box::new(AddressManager::iter(self, services))
    }

    fn insert(&mut self, addrs: impl IntoIterator<Item = (u32, Address)>, source: Source) {
        AddressManager::insert(self, addrs, source);
    }
}

/// Check whether an IP address is locally routable.
pub fn is_local(addr: &net::IpAddr) -> bool {
    match addr {
        net::IpAddr::V4(addr) => {
            addr.is_private() || addr.is_loopback() || addr.is_link_local() || addr.is_unspecified()
        }
        net::IpAddr::V6(_) => false,
    }
}

/// Get the 8-bit key of an IP address. This key is based on the IP address's
/// range, and is used as a key to group IP addresses by range.
pub fn addr_key(ip: &net::IpAddr) -> u8 {
    match ip {
        net::IpAddr::V4(ip) => {
            // Use the /16 range (first two components) of the IP address to key into the
            // range buckets.
            //
            // Eg. 124.99.123.1 and 124.54.123.1 would be placed in
            // different buckets, but 100.99.43.12 and 100.99.12.8
            // would be placed in the same bucket.
            let octets: [u8; 4] = ip.octets();
            let bits: u16 = (octets[0] as u16) << 8 | octets[1] as u16;

            (bits % u8::MAX as u16) as u8
        }
        net::IpAddr::V6(ip) => {
            // Use the first 32 bits of an IPv6 address to as a key.
            let segments: [u16; 8] = ip.segments();
            let bits: u32 = (segments[0] as u32) << 16 | segments[1] as u32;

            (bits % u8::MAX as u32) as u8
        }
    }
}
