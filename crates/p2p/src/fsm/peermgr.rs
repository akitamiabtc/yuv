use std::collections::{HashMap, HashSet};
use std::{net, net::SocketAddr, sync::Arc};

use tracing::{debug, error};

use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts},
    TokioAsyncResolver,
};

use bitcoin::{
    network::address::Address, network::constants::ServiceFlags,
    network::message_network::VersionMessage,
};
use yuv_types::network::Network;

use crate::{
    common::peer::{AddressSource, Source},
    common::time::Clock,
    fsm::addrmgr::is_local,
    fsm::handler::{DisconnectReason, Whitelist},
    fsm::handler::{PeerId, Socket},
    net::{Disconnect as NetDisconnect, Link, LocalDuration, LocalTime},
};

use super::output::{Connect, Disconnect, SetTimer, Wire};

/// Time to wait for response during peer handshake before disconnecting the peer.
pub const HANDSHAKE_TIMEOUT: LocalDuration = LocalDuration::from_secs(12);
/// Time to wait for a new connection.
pub const CONNECTION_TIMEOUT: LocalDuration = LocalDuration::from_secs(6);
/// Time to wait until idle.
pub const IDLE_TIMEOUT: LocalDuration = LocalDuration::from_mins(1);
/// Target number of concurrent outbound peer connections.
pub const TARGET_OUTBOUND_PEERS: usize = 8;
/// Maximum number of inbound peer connections.
pub const MAX_INBOUND_PEERS: usize = 16;

/// A time offset, in seconds.
type TimeOffset = i64;

/// An event originating in the peer manager.
#[derive(Debug, Clone)]
pub enum Event {
    /// The `version` message was received from a peer.
    VersionReceived {
        /// The peer's id.
        addr: PeerId,
        /// The version message.
        msg: VersionMessage,
    },
    /// A peer has successfully negotiated (handshaked).
    Negotiated {
        /// The peer's id.
        addr: PeerId,
        /// Connection link.
        link: Link,
        /// Services offered by negotiated peer.
        services: ServiceFlags,
        /// Peer user agent.
        user_agent: String,
        /// Protocol version.
        version: u32,
    },
    /// Connecting to a peer found from the specified source.
    Connecting(PeerId, Source, ServiceFlags),
    /// Connection attempt failed.
    ConnectionFailed(PeerId, Arc<std::io::Error>),
    /// A new peer has connected and is ready to accept messages.
    /// This event is triggered *before* the peer handshake
    /// has successfully completed.
    Connected(PeerId, Link),
    /// A peer has been disconnected.
    Disconnected(PeerId, NetDisconnect<DisconnectReason>),
}

impl std::fmt::Display for Event {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionReceived { addr, msg } => write!(
                fmt,
                "Peer address = {}, version = {}, height = {}, agent = {}, services = {}, timestamp = {}, nonce = {}",
                addr, msg.version, msg.start_height, msg.user_agent, msg.services, msg.timestamp, msg.nonce,
            ),
            Self::Negotiated {
                addr,
                services,
                ..
            } => write!(
                fmt,
                "{}: Peer negotiated with services {}",
                addr, services
            ),
            Self::Connecting(addr, source, services) => {
                write!(
                    fmt,
                    "Connecting to peer {} from source `{}` with {}",
                    addr, source, services
                )
            }
            Self::Connected(addr, link) => write!(fmt, "{}: Peer connected ({:?})", &addr, link),
            Self::ConnectionFailed(addr, err) => {
                write!(fmt, "{}: Peer connection attempt failed: {}", &addr, err)
            }
            Self::Disconnected(addr, reason) => {
                write!(fmt, "Disconnected from {} ({})", &addr, reason)
            }
        }
    }
}

/// Peer manager configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Protocol version.
    pub protocol_version: u32,
    /// Peer whitelist.
    pub whitelist: Whitelist,
    /// Services offered by this implementation.
    pub services: ServiceFlags,
    /// Peer addresses to persist connections with.
    pub persistent: Vec<net::SocketAddr>,
    /// Services required by peers.
    pub required_services: ServiceFlags,
    /// Target number of outbound peer connections.
    pub target_outbound_peers: usize,
    /// Maximum number of inbound peer connections.
    pub max_inbound_peers: usize,
    /// Maximum time to wait between reconnection attempts.
    pub retry_max_wait: LocalDuration,
    /// Minimum time to wait between reconnection attempts.
    pub retry_min_wait: LocalDuration,
    /// Our user agent.
    pub user_agent: &'static str,
}

/// Peer negotiation (handshake) state.
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Ord, Eq)]
enum HandshakeState {
    /// Received "version" and waiting for "ytxidack" message from remote.
    Version { since: LocalTime },
    /// Received "ytxidack" and waiting for "verack" message from remote
    Ytxidack { since: LocalTime },
    /// Received "verack". Handshake is complete.
    Verack { since: LocalTime },
}

/// A peer connection. Peers that haven't yet sent their `version` message are stored as
/// connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    /// Remote peer socket.
    pub socket: Socket,
    /// Local peer address.
    pub local_addr: net::SocketAddr,
    /// Whether this is an inbound or outbound peer connection.
    pub link: Link,
    /// Connected since this time.
    pub since: LocalTime,
}

/// Peer state.
#[derive(Debug, Clone)]
pub enum Peer {
    /// A connection is being attempted.
    Connecting {
        /// Time the connection was attempted.
        time: LocalTime,
    },
    /// A connection is established.
    Connected {
        /// Connection.
        conn: Connection,
        /// Peer information, if a `version` message was received.
        peer: Option<PeerInfo>,
    },
}

/// A peer with protocol information.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The peer's services.
    pub services: ServiceFlags,
    /// Peer user agent string.
    pub user_agent: String,
    /// An offset in seconds, between this peer's clock and ours.
    /// A positive offset means the peer's clock is ahead of ours.
    pub time_offset: TimeOffset,
    /// Whether this peer relays transactions.
    pub relay: bool,
    /// The max protocol version supported by both the peer and yuv_p2p.
    pub version: u32,
    /// Whether this is a persistent peer.
    pub persistent: bool,
    /// Whether this peer supports BIP-339.
    pub wtxidrelay: bool,
    /// Whether this peer use YUV protocol
    pub ytxidrelay: bool,
    /// Peer nonce. Used to detect self-connections.
    nonce: u64,
    /// Peer handshake state.
    state: HandshakeState,
}

impl PeerInfo {
    /// Check whether the peer has finished negotiating and received our `version`.
    pub fn is_negotiated(&self) -> bool {
        matches!(self.state, HandshakeState::Verack { .. })
    }
}

/// Manages peer connections and handshake.
#[derive(Debug)]
pub struct PeerManager<U, C> {
    /// Peer manager configuration.
    pub config: Config,
    /// Last time we were idle.
    last_idle: Option<LocalTime>,
    /// Connection states.
    peers_storage: HashMap<SocketAddr, Peer>,
    /// Peers that have been disconnected and a retry attempt is scheduled.
    disconnected: HashMap<net::SocketAddr, (Option<LocalTime>, usize)>,
    /// Bitcoin network type
    network: Network,
    upstream: U,
    rng: fastrand::Rng,
    clock: C,
}

impl<U: Wire<Event> + SetTimer + Connect + Disconnect, C: Clock + Sync> PeerManager<U, C> {
    /// Create a new peer manager.
    pub fn new(
        config: Config,
        rng: fastrand::Rng,
        upstream: U,
        clock: C,
        network: Network,
    ) -> Self {
        let disconnected = HashMap::new();
        let peers_storage = HashMap::new();

        Self {
            config,
            last_idle: None,
            peers_storage,
            disconnected,
            upstream,
            rng,
            clock,
            network,
        }
    }

    /// Initialize the peer manager. Must be called once.
    pub async fn initialize<A: AddressSource>(&mut self, addrs: &mut A) {
        let peers = self.config.persistent.clone();

        for addr in peers {
            if !self.connect(&addr) {
                debug!("{}: unable to connect to persistent peer", addr);
            }
        }
        self.upstream.set_timer(IDLE_TIMEOUT);
        self.maintain_connections(addrs).await;
    }

    /// A persistent peer has been disconnected.
    fn persistent_disconnected(&mut self, addr: &net::SocketAddr, local_time: LocalTime) {
        let (retry_at, attempts) = self.disconnected.entry(*addr).or_default();
        let delay = LocalDuration::from_secs(2u64.saturating_pow(*attempts as u32))
            .clamp(self.config.retry_min_wait, self.config.retry_max_wait);

        *retry_at = Some(local_time + delay);
        *attempts += 1;

        self.upstream.set_timer(delay);
    }

    /// Maintain persistent peer connections.
    fn maintain_persistent(&mut self) {
        let local_time = self.clock.local_time();
        let mut reconnect = Vec::new();

        for (addr, (retry_at, _)) in &mut self.disconnected {
            if let Some(t) = retry_at {
                if *t <= local_time {
                    *retry_at = None;
                    reconnect.push(*addr);
                }
            }
        }

        for addr in reconnect {
            if !self.connect(&addr) {
                error!(target: "p2p", "Couldn't establish connection with {addr}");
            }
        }
    }

    /// Maintain peers that we accepted from other nodes
    pub fn maintain_newcome<A: AddressSource>(&mut self, addrs: &mut A) {
        while let Some(addr) = addrs.sample_with(|ka| {
            matches!(ka.source, Source::Peer(_))
                | matches!(ka.source, Source::Imported)
                | matches!(ka.source, Source::Dns)
        }) {
            if let Ok(socket_addr) = addr.0.socket_addr() {
                self.whitelist(socket_addr);
                self.connect(&socket_addr);
            }
        }
    }

    /// Called when a peer connected. Returns true if peer is already connected, false - connection is new.
    pub fn peer_connected(&mut self, addr: SocketAddr, local_addr: SocketAddr, link: Link) -> bool {
        let local_time = self.clock.local_time();

        #[cfg(debug_assertions)]
        if link.is_outbound() {
            debug_assert!(self.is_connecting(&addr), "{} is not connecting", addr)
        }
        debug_assert!(!self.is_connected(&addr), "{} is already connected", addr);

        self.peers_storage.insert(
            addr,
            Peer::Connected {
                conn: Connection {
                    socket: Socket::new(addr),
                    local_addr,
                    link,
                    since: local_time,
                },
                peer: None,
            },
        );
        self.disconnected.remove(&addr);

        match link {
            Link::Inbound => {
                if self.connected().filter(|c| c.link.is_inbound()).count()
                    >= self.config.max_inbound_peers
                {
                    // TODO: Test this branch.
                    // Don't allow inbound connections beyond the configured limit.
                    self._disconnect(addr, DisconnectReason::ConnectionLimit);
                } else {
                    // Wait for their version message..
                }
            }
            Link::Outbound => {
                let nonce = self.rng.u64(..);
                self.upstream
                    .version(addr, self.version(addr, local_addr, nonce));
            }
        }
        // Set a timeout for receiving the `version` message.
        self.upstream.set_timer(HANDSHAKE_TIMEOUT);
        self.upstream.event(Event::Connected(addr, link));
        false
    }

    /// Called when a peer disconnected.
    pub async fn peer_disconnected<A: AddressSource>(
        &mut self,
        addr: &SocketAddr,
        addrs: &mut A,
        reason: NetDisconnect<DisconnectReason>,
    ) {
        let local_time = self.clock.local_time();
        // debug_assert!(self.peers_storage.contains_key(addr));
        // debug_assert!(!self.is_disconnected(addr));

        if self.is_connected(addr) {
            self.upstream.event(Event::Disconnected(*addr, reason));
        } else if self.is_connecting(addr) {
            // If we haven't yet established a connection, the disconnect reason
            // should always be a `ConnectionError`.
            if let NetDisconnect::ConnectionError(err) = reason {
                self.upstream.event(Event::ConnectionFailed(*addr, err));
            }
        }
        self.peers_storage.remove(addr);

        if self.config.persistent.contains(addr) {
            self.persistent_disconnected(addr, local_time);
        } else {
            // If an outbound peer disconnected, we should make sure to maintain
            // our target outbound connection count.
            self.maintain_connections(addrs).await;
        }
    }

    /// Called when a `wtxidrelay` message was received.
    pub fn received_wtxidrelay(&mut self, addr: &PeerId) {
        if let Some(Peer::Connected {
            peer: Some(peer),
            conn: _,
        }) = self.peers_storage.get_mut(addr)
        {
            match peer.state {
                HandshakeState::Version { .. } => peer.wtxidrelay = true,
                _ => self.disconnect(
                    *addr,
                    DisconnectReason::PeerMisbehaving(
                        "`wtxidrelay` must be received before `verack`",
                    ),
                ),
            }
        }
    }

    /// Called when a `ytxidrelay` message was received.
    pub fn received_ytxidrelay(&mut self, addr: &PeerId) {
        if let Some(Peer::Connected {
            peer: Some(peer),
            conn: _,
        }) = self.peers_storage.get_mut(addr)
        {
            match peer.state {
                HandshakeState::Version { .. } => {
                    peer.ytxidrelay = true;
                    self.upstream.ytxidack(*addr);
                    self.upstream.verack(*addr);
                }
                _ => self.disconnect(
                    *addr,
                    DisconnectReason::PeerMisbehaving(
                        "`ytxidrelay` must be received before `verack`",
                    ),
                ),
            }
        }
    }

    /// Called when a `version` message was received.
    pub fn received_version<A: AddressSource>(
        &mut self,
        addr: &PeerId,
        msg: VersionMessage,
        addrs: &mut A,
    ) {
        if let Err(reason) = self.handle_version(addr, msg, addrs) {
            self._disconnect(*addr, reason);
        }
    }

    fn handle_version<A: AddressSource>(
        &mut self,
        addr: &PeerId,
        msg: VersionMessage,
        addrs: &mut A,
    ) -> Result<(), DisconnectReason> {
        let now = self.clock.local_time();

        if let Some(Peer::Connected { conn, .. }) = self.peers_storage.get(addr) {
            self.upstream.event(Event::VersionReceived {
                addr: *addr,
                msg: msg.clone(),
            });

            let VersionMessage {
                // Peer's local time.
                timestamp,
                // Highest protocol version understood by the peer.
                version,
                // Services offered by this peer.
                services,
                // User agent.
                user_agent,
                // Peer nonce.
                nonce,
                // Our address, as seen by the remote peer.
                receiver,
                // Relay node.
                relay,
                ..
            } = msg;

            let target = self.config.target_outbound_peers;
            let trusted =
                self.config.whitelist.contains(&addr.ip(), &user_agent) || is_local(&addr.ip());

            // Don't support peers with too old of a protocol version.
            if version < crate::fsm::handler::PROTOCOL_VERSION {
                return Err(DisconnectReason::PeerProtocolVersion(version));
            }

            // Peers that don't advertise the `NETWORK` service are not full nodes.
            // It's not so useful for us to connect to them, because they're likely
            // to be less secure.
            if conn.link.is_outbound() && !services.has(self.config.required_services) && !trusted {
                return Err(DisconnectReason::PeerServices(services));
            }

            // Check for self-connections. We only need to check one link direction,
            // since in the case of a self-connection, we will see both link directions.
            for (peer, conn) in self.peers() {
                if conn.link.is_outbound() && peer.nonce == nonce {
                    return Err(DisconnectReason::SelfConnection);
                }
            }

            // If this peer doesn't have the preferred services, and we already have enough peers,
            // disconnect this peer.
            if conn.link.is_outbound()
                // && !services.has(preferred)
                && self.negotiated(Link::Outbound).count() >= target
            {
                return Err(DisconnectReason::ConnectionLimit);
            }

            // Record the address this peer has of us.
            if let Ok(addr) = receiver.socket_addr() {
                addrs.record_local_address(addr);
            }

            match conn.link {
                Link::Inbound => {
                    self.upstream.version(
                        conn.socket.addr,
                        self.version(conn.socket.addr, conn.local_addr, nonce),
                    );
                    self.upstream
                        .wtxid_relay(conn.socket.addr)
                        .ytxid_relay(conn.socket.addr)
                        .set_timer(HANDSHAKE_TIMEOUT);
                }
                Link::Outbound => {
                    self.upstream
                        .wtxid_relay(conn.socket.addr)
                        .ytxid_relay(conn.socket.addr)
                        .set_timer(HANDSHAKE_TIMEOUT);
                }
            }
            let conn = conn.clone();
            let persistent = self.config.persistent.contains(&conn.socket.addr);

            self.peers_storage.insert(
                conn.socket.addr,
                Peer::Connected {
                    conn,
                    peer: Some(PeerInfo {
                        nonce,
                        time_offset: timestamp,
                        services,
                        persistent,
                        user_agent,
                        state: HandshakeState::Version { since: now },
                        relay,
                        version: u32::min(self.config.protocol_version, version),
                        wtxidrelay: false,
                        ytxidrelay: true, // for now we assume that every node supports YUV protocol by default
                    }),
                },
            );
        }

        Ok(())
    }

    pub fn received_ytxidack(&mut self, addr: &PeerId, local_time: LocalTime) {
        if let Some(Peer::Connected {
            peer: Some(peer),
            conn: _,
        }) = self.peers_storage.get_mut(addr)
        {
            if let HandshakeState::Version { .. } = peer.state {
                peer.state = HandshakeState::Ytxidack { since: local_time }
            } else {
                self._disconnect(
                    *addr,
                    DisconnectReason::PeerMisbehaving("unexpected `ytxidack` message received"),
                );
            }
        }
    }

    /// Called when a `verack` message was received.
    pub fn received_verack(
        &mut self,
        addr: &PeerId,
        local_time: LocalTime,
    ) -> Option<(PeerInfo, Connection)> {
        if let Some(Peer::Connected {
            peer: Some(peer),
            conn,
        }) = self.peers_storage.get_mut(addr)
        {
            if let HandshakeState::Ytxidack { .. } = peer.state {
                self.upstream.event(Event::Negotiated {
                    addr: *addr,
                    link: conn.link,
                    services: peer.services,
                    user_agent: peer.user_agent.clone(),
                    version: peer.version,
                });

                peer.state = HandshakeState::Verack { since: local_time };

                return Some((peer.clone(), conn.clone()));
            } else {
                self._disconnect(
                    *addr,
                    DisconnectReason::PeerMisbehaving("unexpected `verack` message received"),
                );
            }
        }
        None
    }

    /// Called when a tick was received.
    pub async fn received_wake<A: AddressSource>(&mut self, addrs: &mut A) {
        let mut timed_out = Vec::new();
        let local_time = self.clock.local_time();

        // Time out all peers that have been idle in a "connecting" state for too long.
        for addr in self.idle_peers(local_time).collect::<Vec<_>>() {
            timed_out.push((addr, "connection"));
        }
        // Time out peers that haven't sent a `verack` quickly enough.
        for (peer, conn) in self.peers() {
            match peer.state {
                HandshakeState::Version { since } => {
                    if local_time - since >= HANDSHAKE_TIMEOUT {
                        timed_out.push((conn.socket.addr, "handshake"));
                    }
                }
                HandshakeState::Ytxidack { .. } | HandshakeState::Verack { .. } => {}
            }
        }
        // Time out peers that haven't sent a `version` quickly enough.
        for connected in self.peers_storage.values().filter_map(|c| match c {
            Peer::Connected { conn, peer: None } => Some(conn),
            _ => None,
        }) {
            if local_time - connected.since >= HANDSHAKE_TIMEOUT {
                timed_out.push((connected.socket.addr, "handshake"));
            }
        }
        // Disconnect all timed out peers.
        for (addr, reason) in timed_out {
            self._disconnect(addr, DisconnectReason::PeerTimeout(reason));
        }

        // Disconnect peers that have been dropped from all other sub-protocols.
        // Since the job of the peer manager is simply to establish connections, if a peer is
        // dropped from all other sub-protocols and we are holding on to the last reference,
        // there is no use in keeping this peer around.
        let dropped = self
            .negotiated(Link::Outbound)
            .filter(|(_, c)| c.socket.refs() == 1)
            .map(|(_, c)| c.socket.addr)
            .collect::<Vec<_>>();
        for addr in dropped {
            self._disconnect(addr, DisconnectReason::PeerDropped);
        }

        if local_time - self.last_idle.unwrap_or_default() >= IDLE_TIMEOUT {
            self.maintain_connections(addrs).await;
            self.upstream.set_timer(IDLE_TIMEOUT);
            self.last_idle = Some(local_time);
        }

        self.maintain_persistent();
        self.maintain_newcome(addrs);
    }

    /// Whitelist a peer.
    pub fn whitelist(&mut self, addr: net::SocketAddr) -> bool {
        self.config.whitelist.addr.insert(addr.ip())
    }

    /// Create a `version` message for this peer.
    pub fn version(
        &self,
        addr: net::SocketAddr,
        local_addr: net::SocketAddr,
        nonce: u64,
    ) -> VersionMessage {
        VersionMessage {
            // Our max supported protocol version.
            version: self.config.protocol_version,
            // Local services.
            services: self.config.services,
            // Local time.
            timestamp: 0,
            // Receiver address and services, as perceived by us.
            receiver: Address::new(&addr, ServiceFlags::NONE),
            // Local address (unreliable) and local services (same as `services` field)
            sender: Address::new(&local_addr, self.config.services),
            // A nonce to detect connections to self.
            nonce,
            // Our user agent string.
            user_agent: self.config.user_agent.to_owned(),
            // Blockchain height (just assume it as 0 as we do not care about bc height)
            start_height: 0,
            // Whether we want to receive transaction `inv` messages.
            relay: false,
        }
    }
}

/// Connection management functions.
impl<U: Connect + Disconnect + SetTimer + Wire<Event>, C: Clock + Sync> PeerManager<U, C> {
    /// Called when a peer is being connected to.
    pub fn peer_attempted(&mut self, addr: &net::SocketAddr) {
        // Since all "attempts" are made from this module, we expect that when a peer is
        // attempted, we know about it already.
        //
        // It's possible that as we were attempting to connect to a peer, that peer in the
        // meantime connected to us. Hence we also account for an already-connected *inbound*
        // peer.
        debug_assert!(self.is_connecting(addr) || self.is_inbound(addr));
    }

    /// Check whether a peer is connected via an inbound link.
    pub fn is_inbound(&mut self, addr: &PeerId) -> bool {
        self.peers_storage.get(addr).map_or(
            false,
            |c| matches!(c, Peer::Connected { conn, .. } if conn.link.is_inbound()),
        )
    }

    /// Check whether a peer is connecting.
    pub fn is_connecting(&self, addr: &PeerId) -> bool {
        self.peers_storage
            .get(addr)
            .map_or(false, |c| matches!(c, Peer::Connecting { .. }))
    }

    /// Check whether a peer is connected.
    pub fn is_connected(&self, addr: &PeerId) -> bool {
        self.peers_storage
            .get(addr)
            .map_or(false, |c| matches!(c, Peer::Connected { .. }))
    }

    /// Check whether a peer is disconnected.
    pub fn is_disconnected(&self, addr: &PeerId) -> bool {
        !self.is_connected(addr) && !self.is_connecting(addr)
    }

    /// Iterator over peers that have at least sent their `version` message.
    pub fn peers(&self) -> impl Iterator<Item = (&PeerInfo, &Connection)> + Clone {
        self.peers_storage.values().filter_map(move |c| match c {
            Peer::Connected {
                conn,
                peer: Some(peer),
            } => Some((peer, conn)),
            _ => None,
        })
    }

    /// Returns connecting peers.
    pub fn connecting(&self) -> impl Iterator<Item = &PeerId> {
        self.peers_storage
            .iter()
            .filter(|(_, p)| matches!(p, Peer::Connecting { .. }))
            .map(|(addr, _)| addr)
    }

    /// Iterator over peers in a *connected* state..
    pub fn connected(&self) -> impl Iterator<Item = &Connection> + Clone {
        self.peers_storage.values().filter_map(|c| match c {
            Peer::Connected { conn, .. } => Some(conn),
            _ => None,
        })
    }

    /// Iterator over fully negotiated peers.
    pub fn negotiated(&self, link: Link) -> impl Iterator<Item = (&PeerInfo, &Connection)> + Clone {
        self.peers()
            .filter(move |(p, c)| p.is_negotiated() && c.link == link)
    }

    /// Connect to a peer.
    pub fn connect(&mut self, addr: &PeerId) -> bool {
        let time = self.clock.local_time();

        if self.is_connected(addr) || self.is_connecting(addr) {
            return true;
        }
        if !self.is_disconnected(addr) {
            return false;
        }

        self.peers_storage.insert(*addr, Peer::Connecting { time });
        self.upstream.connect(*addr, CONNECTION_TIMEOUT);

        true
    }

    /// Disconnect from a peer.
    pub fn disconnect(&mut self, addr: PeerId, reason: DisconnectReason) {
        if self.is_connected(&addr) {
            self._disconnect(addr, reason);
        }
    }

    /// Disconnect a peer (internal).
    fn _disconnect(&mut self, addr: PeerId, reason: DisconnectReason) {
        self.upstream.disconnect(addr, reason);
    }

    /// Given the current peer state and targets, calculate how many new connections we should
    /// make.
    fn delta(&self) -> usize {
        // Peers with our preferred services.
        let primary = self
            .negotiated(Link::Outbound)
            // .filter(|(p, _)| p.services.has(self.config.preferred_services))
            .count();
        // Peers only with required services, which we'd eventually want to drop in favor of peers
        // that have all services.
        let secondary = self.negotiated(Link::Outbound).count() - primary;
        // Connected peers that have not yet completed handshake.
        let connected = self.connected().count() - primary - secondary;
        // Connecting peers.
        let connecting = self.connecting().count();

        // We connect up to the target number of peers plus an extra margin equal to the number of
        // target divided by two. This ensures we have *some* connections to
        // primary peers, even if that means exceeding our target. When a secondary peer is
        // dropped, if we have our target number of primary peers connected, there is no need
        // to replace the connection.
        //
        // Above the target count, all peer connections without the preferred services are
        // automatically dropped. This ensures we never have more than the target of secondary
        // peers.
        let target = self.config.target_outbound_peers;
        let unknown = connecting + connected;
        let total = primary + secondary + unknown;
        let max = target + target / 2;

        // If we are somehow connected to more peers than the target or maximum,
        // don't attempt to connect to more. This can happen if the client has been
        // requesting connections to specific peers.
        if total > max || primary + unknown > target {
            return 0;
        }

        usize::min(max - total, target - (primary + unknown))
    }

    /// List of DNS seeds (for now it`s just Bitcoin hardcoded seeds)
    fn get_dns_seed(&self) -> &[&str] {
        // TODO: add YUV seednodes
        match self.network {
            Network::Bitcoin => &[],
            Network::Testnet => &[],
            Network::Regtest => &[],
            Network::Signet => &[],
            Network::Mutiny => &[],
        }
    }

    /// Bitcoin ports (for now we use them for DNS seeds)
    fn get_port(&self) -> u16 {
        match self.network {
            Network::Bitcoin => 8333,
            Network::Testnet => 18333,
            Network::Regtest => 18444,
            Network::Signet => 38333,
            Network::Mutiny => 38332,
        }
    }

    /// Attempt to maintain a certain number of outbound peers.
    async fn maintain_connections<A: AddressSource>(&mut self, addrs: &mut A) {
        // If we have persistent peers configured, we don't use this mechanism for maintaining
        // connections. Instead, we retry the configured peers.
        if !self.config.persistent.is_empty() {
            return;
        }

        let delta = self.delta();
        let negotiated = self.negotiated(Link::Outbound).count();
        let target = self.config.target_outbound_peers;

        // Keep track of new addresses we're connecting to, and loop until
        // we've connected to enough addresses.
        let mut connecting = HashSet::new();

        while connecting.len() < delta {
            if let Some((addr, source)) = if negotiated < target {
                addrs
                    .sample(self.config.required_services)
                    // If we can't find peers with any kind of useful services, then
                    // perhaps we should connect to peers that may know of such peers. This
                    // is especially important when doing an initial DNS sync, since DNS
                    // addresses don't come with service information. This will draw from
                    // that pool.
                    .or_else(|| addrs.sample(ServiceFlags::NONE))
            } else {
                None
            } {
                if let Ok(sockaddr) = addr.socket_addr() {
                    debug_assert!(!self.is_connected(&sockaddr));

                    if self.connect(&sockaddr) {
                        connecting.insert(sockaddr);
                        self.upstream
                            .event(Event::Connecting(sockaddr, source, addr.services));
                    }
                }
            } else {
                let resolver =
                    TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

                // For now we just use Bitcoin seeds.
                if self.get_dns_seed().is_empty() {
                    debug!("Tried to get more addresses from DNS seeds, however, there`s no DNS seeds provided");
                    break;
                }

                let seed_ind = self.rng.usize(0..self.get_dns_seed().len());
                let dns_seed = self
                    .get_dns_seed()
                    .get(seed_ind)
                    .expect("must return dns seed");

                // Get random dns seed
                match resolver.lookup_ip(dns_seed.to_string()).await {
                    Ok(response) => {
                        let ips: Vec<(u32, Address)> = response
                            .iter()
                            .map(|addr| {
                                (
                                    LocalTime::now().as_secs() as u32,
                                    Address::new(
                                        &SocketAddr::new(addr, self.get_port()),
                                        ServiceFlags::NONE,
                                    ),
                                )
                            })
                            .collect();

                        addrs.insert(ips, Source::Dns);
                    }
                    Err(e) => error!("Failed to get addresses from DNS seed {dns_seed}: {e}"),
                }
                break;
            }
        }
    }

    /// Peers that have been idle longer than [`CONNECTION_TIMEOUT`].
    fn idle_peers(&self, now: LocalTime) -> impl Iterator<Item = PeerId> + '_ {
        self.peers_storage.iter().filter_map(move |(addr, c)| {
            if let Peer::Connecting { time } = c {
                if now - *time >= CONNECTION_TIMEOUT {
                    return Some(*addr);
                }
            }
            None
        })
    }
}
