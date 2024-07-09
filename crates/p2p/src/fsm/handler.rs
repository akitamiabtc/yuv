//! Bitcoin protocol state machine.
use std::{borrow::Cow, collections::HashSet, fmt, net, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use bitcoin::network::Magic;
use bitcoin::{locktime::absolute::Height, network::constants::ServiceFlags, network::Address};
use flume as chan;
use tracing::{debug, trace};

use event_bus::{typeid, EventBus};
use yuv_types::messages::p2p::{Inventory, NetworkMessage, RawNetworkMessage};
use yuv_types::network::Network;
use yuv_types::{ControllerMessage, ControllerP2PMessage, YuvTransaction};

use crate::fsm::output::Outbox;
use crate::{
    common::peer,
    common::peer::AddressSource,
    common::time::AdjustedClock,
    fsm::addrmgr::AddressManager,
    fsm::event::Event,
    fsm::invmgr::InventoryManager,
    fsm::peermgr::PeerManager,
    fsm::pingmgr::PingManager,
    fsm::{output, peermgr, pingmgr},
    net::{Disconnect, Link, LocalDuration, LocalTime},
};

pub type PeerId = net::SocketAddr;

/// Peer-to-peer protocol version.
pub const PROTOCOL_VERSION: u32 = 100000;
/// User agent included in `version` messages.
pub const USER_AGENT: &str = concat!("/yuv:", env!("CARGO_PKG_VERSION"), "/");

/// Configured limits.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Target outbound peer connections.
    pub max_outbound_peers: usize,
    /// Maximum inbound peer connections.
    pub max_inbound_peers: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_outbound_peers: peermgr::TARGET_OUTBOUND_PEERS,
            max_inbound_peers: peermgr::MAX_INBOUND_PEERS,
        }
    }
}

/// Peer whitelist.
#[derive(Debug, Clone, Default)]
pub struct Whitelist {
    /// Trusted addresses.
    pub(crate) addr: HashSet<net::IpAddr>,
    /// Trusted user-agents.
    user_agent: HashSet<String>,
}

impl Whitelist {
    pub(crate) fn contains(&self, addr: &net::IpAddr, user_agent: &str) -> bool {
        self.addr.contains(addr) || self.user_agent.contains(user_agent)
    }
}

/// Reference counting virtual socket.
/// When there are no more references held, this peer can be dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Socket {
    /// Socket address.
    pub addr: net::SocketAddr,
    /// Reference counter.
    refs: Arc<()>,
}

impl Socket {
    /// Create a new virtual socket.
    pub fn new(addr: impl Into<net::SocketAddr>) -> Self {
        Self {
            addr: addr.into(),
            refs: Arc::new(()),
        }
    }

    /// Get the number of references to this virtual socket.
    pub fn refs(&self) -> usize {
        Arc::strong_count(&self.refs)
    }
}

impl From<net::SocketAddr> for Socket {
    fn from(addr: net::SocketAddr) -> Self {
        Self::new(addr)
    }
}

/// Disconnect reason.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// Peer is misbehaving.
    PeerMisbehaving(&'static str),
    /// Peer protocol version is too old or too recent.
    PeerProtocolVersion(u32),
    /// Peer doesn't have the required services.
    PeerServices(ServiceFlags),
    /// Peer magic is invalid.
    PeerMagic(Magic),
    /// Peer timed out.
    PeerTimeout(&'static str),
    /// Peer was dropped by all sub-protocols.
    PeerDropped,
    /// Connection to self was detected.
    SelfConnection,
    /// Inbound connection limit reached.
    ConnectionLimit,
    /// Error trying to decode incoming message.
    DecodeError,
    /// Peer was forced to disconnect by external command.
    Command,
    /// Peer already had a connection and was banned due to the violation of protocol rules
    PeerBanned,
    /// Peer was disconnected for another reason.
    Other(&'static str),
}

impl DisconnectReason {
    /// Check whether the disconnect reason is transient, ie. may no longer be applicable
    /// after some time.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::ConnectionLimit | Self::PeerTimeout(_))
    }
}

impl From<DisconnectReason> for crate::net::Disconnect<DisconnectReason> {
    fn from(reason: DisconnectReason) -> Self {
        Self::StateMachine(reason)
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerMisbehaving(reason) => write!(f, "peer misbehaving: {}", reason),
            Self::PeerProtocolVersion(_) => write!(f, "peer protocol version mismatch"),
            Self::PeerServices(_) => write!(f, "peer doesn't have the required services"),
            Self::PeerMagic(magic) => write!(f, "received message with invalid magic: {}", magic),
            Self::PeerTimeout(s) => write!(f, "peer timed out: {:?}", s),
            Self::PeerDropped => write!(f, "peer dropped"),
            Self::SelfConnection => write!(f, "detected self-connection"),
            Self::ConnectionLimit => write!(f, "inbound connection limit reached"),
            Self::DecodeError => write!(f, "message decode error"),
            Self::Command => write!(f, "received external command"),
            Self::PeerBanned => write!(f, "peer was banned due to violation of protocol rules"),
            Self::Other(reason) => write!(f, "{}", reason),
        }
    }
}

/// A remote peer.
#[derive(Debug, Clone)]
pub struct Peer {
    /// Peer address.
    pub addr: net::SocketAddr,
    /// Local peer address.
    pub local_addr: net::SocketAddr,
    /// Whether this is an inbound or outbound peer connection.
    pub link: Link,
    /// Connected since this time.
    pub since: LocalTime,
    /// The peer's services.
    pub services: ServiceFlags,
    /// Peer user agent string.
    pub user_agent: String,
    /// Whether this peer relays transactions.
    pub relay: bool,
}

impl Peer {
    /// Check if this is an outbound peer.
    pub fn is_outbound(&self) -> bool {
        self.link.is_outbound()
    }
}

impl From<(&peermgr::PeerInfo, &peermgr::Connection)> for Peer {
    fn from((peer, conn): (&peermgr::PeerInfo, &peermgr::Connection)) -> Self {
        Self {
            addr: conn.socket.addr,
            local_addr: conn.local_addr,
            link: conn.link,
            since: conn.since,
            services: peer.services,
            user_agent: peer.user_agent.clone(),
            relay: peer.relay,
        }
    }
}

/// An instance of the Bitcoin P2P network protocol. Parametrized over the
/// block-tree and compact filter store.
pub struct StateMachine<P, C> {
    /// Bitcoin network we're connecting to.
    pub network: Network,
    /// Peer address manager.
    addrmgr: AddressManager<P, Outbox, C>,
    /// Ping manager.
    pingmgr: PingManager<Outbox, C>,
    /// Peer manager.
    pub peermgr: PeerManager<Outbox, C>,
    /// Inventory manager.
    invmgr: InventoryManager<Outbox>,
    /// Network-adjusted clock.
    pub clock: C,
    /// Last time a "tick" was triggered.
    #[allow(dead_code)]
    last_tick: LocalTime,
    /// Random number generator.
    pub rng: fastrand::Rng,
    /// Outbound I/O. Used to communicate protocol events with a reactor.
    pub outbox: Outbox,
    event_bus: EventBus,
}

/// State machine configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Bitcoin network we are connected to.
    pub network: Network,
    /// Peers to connect to.
    pub connect: Vec<net::SocketAddr>,
    /// Services offered by our peer.
    pub services: ServiceFlags,
    /// Required peer services.
    pub required_services: ServiceFlags,
    /// Peer whitelist. Peers in this list are trusted by default.
    pub whitelist: Whitelist,
    /// Our user agent.
    pub user_agent: &'static str,
    /// Ping timeout, after which remotes are disconnected.
    pub ping_timeout: LocalDuration,
    /// Configured limits.
    pub limits: Limits,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Network::Bitcoin,
            connect: Vec::new(),
            services: ServiceFlags::NONE,
            required_services: ServiceFlags::NETWORK,
            whitelist: Whitelist::default(),
            ping_timeout: pingmgr::PING_TIMEOUT,
            user_agent: USER_AGENT,
            limits: Limits::default(),
        }
    }
}

impl<P, C> Iterator for StateMachine<P, C> {
    type Item = output::Io;

    fn next(&mut self) -> Option<output::Io> {
        self.outbox.next()
    }
}

/// A command or request that can be sent to the protocol.
#[derive(Clone)]
pub enum Command {
    /// Get connected peers.
    GetPeers(ServiceFlags, chan::Sender<Vec<Peer>>),
    /// Broadcast to peers matching the predicate.
    Broadcast(NetworkMessage, fn(Peer) -> bool, chan::Sender<Vec<PeerId>>),
    /// Send a message to a random peer.
    Query(NetworkMessage, chan::Sender<Option<SocketAddr>>),
    /// Connect to a peer.
    Connect(SocketAddr),
    /// Disconnect from a peer.
    Disconnect(SocketAddr),
    /// Import addresses into the address book.
    ImportAddresses(Vec<Address>),
    /// Send Inv message to the desired peer
    SendInv(Vec<Inventory>),
    /// Send GetData message to the desired peer
    SendGetData(Vec<Inventory>, SocketAddr),
    /// Send GetData message to the desired peer
    SendYuvTransactions(Vec<YuvTransaction>, SocketAddr),
    /// Forbid some peer to connect to us
    BanPeer(SocketAddr),
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetPeers(flags, _) => write!(f, "GetPeers({})", flags),
            Self::Broadcast(msg, _, _) => write!(f, "Broadcast({:?})", msg),
            Self::Query(msg, _) => write!(f, "Query({:?})", msg),
            Self::SendInv(msg) => write!(f, "SendInv({:?})", msg),
            Self::SendGetData(msg, addr) => write!(f, "SendGetData({:?}) to {:?}", msg, addr),
            Self::SendYuvTransactions(msg, addr) => {
                write!(f, "SendYuvTransactions({:?}) to {:?}", msg, addr)
            }
            Self::Connect(addr) => write!(f, "Connect({})", addr),
            Self::Disconnect(addr) => write!(f, "Disconnect({})", addr),
            Self::ImportAddresses(addrs) => write!(f, "ImportAddresses({:?})", addrs),
            Self::BanPeer(addr) => write!(f, "BanPeer({:?})", addr),
        }
    }
}

impl<P: peer::Store + Send, C: AdjustedClock<PeerId> + Sync + Send> StateMachine<P, C> {
    /// Process a user command.
    pub async fn command(&mut self, cmd: Command) {
        debug!(target: "p2p", "Received command: {:?}", cmd);

        match cmd {
            Command::GetPeers(services, reply) => {
                let peers = self
                    .peermgr
                    .peers()
                    .filter(|(p, _)| p.is_negotiated())
                    .filter(|(p, _)| p.services.has(services))
                    .map(Peer::from)
                    .collect::<Vec<Peer>>();

                reply.send_async(peers).await.ok();
            }
            Command::Connect(addr) => {
                self.peermgr.whitelist(addr);
                self.peermgr.connect(&addr);
            }
            Command::Disconnect(addr) => {
                self.disconnect(addr, DisconnectReason::Command);
            }
            Command::Query(msg, reply) => {
                reply.send_async(self.query(msg, |_| true)).await.ok();
            }
            Command::Broadcast(msg, predicate, reply) => {
                let peers = self.broadcast(msg, |p| predicate(p.clone()));
                reply.send_async(peers).await.ok();
            }
            Command::ImportAddresses(addrs) => {
                self.addrmgr.insert(
                    // Nb. For imported addresses, the time last active is not relevant.
                    addrs.into_iter().map(|a| (0u32, a)),
                    peer::Source::Imported,
                );
            }
            Command::SendInv(txids) => {
                self.broadcast(NetworkMessage::Inv(txids), |_| true);
            }
            Command::SendGetData(txids, addr) => {
                self.send(NetworkMessage::GetData(txids), addr);
            }
            Command::SendYuvTransactions(txs, addr) => {
                self.send(NetworkMessage::YuvTx(txs), addr);
            }
            Command::BanPeer(addr) => {
                self.addrmgr
                    .peer_disconnected(&addr, Disconnect::PeerBanned);
                self.peermgr.disconnect(addr, DisconnectReason::PeerBanned);
            }
        }
    }
}

#[async_trait]
impl<P: peer::Store + Send, C: AdjustedClock<PeerId> + Sync + Send> crate::net::StateMachine
    for StateMachine<P, C>
{
    type Message = RawNetworkMessage;
    type Event = Event;
    type DisconnectReason = DisconnectReason;

    async fn initialize(&mut self, time: LocalTime) {
        self.clock.set(time);
        self.outbox.event(Event::Initializing);
        self.addrmgr.initialize();
        self.peermgr.initialize(&mut self.addrmgr).await;

        self.outbox.event(Event::Ready {
            height: Height::from_consensus(0).unwrap(),
            filter_height: Height::from_consensus(0).unwrap(),
            time,
        });
    }

    async fn message_received(&mut self, addr: &SocketAddr, msg: Cow<'_, RawNetworkMessage>) {
        let now = self.clock.local_time();
        let addr = *addr;
        let msg = msg.into_owned();

        if msg.magic != self.network.magic() {
            self.disconnect(addr, DisconnectReason::PeerMagic(msg.magic))
        }

        if !self.peermgr.is_connected(&addr) {
            debug!(target: "p2p", "Received {:?} from unknown peer {}", msg, addr);
            return;
        }

        debug!(target: "p2p", "Received {:?} from {}", msg, addr);

        match msg.payload.clone() {
            NetworkMessage::Inv(inv) => {
                self.event_bus
                    .send(ControllerMessage::P2P(ControllerP2PMessage::Inv {
                        inv,
                        sender: addr,
                    }))
                    .await;
            }
            NetworkMessage::GetData(inv) => {
                self.event_bus
                    .send(ControllerMessage::P2P(ControllerP2PMessage::GetData {
                        inv,
                        sender: addr,
                    }))
                    .await;
            }
            NetworkMessage::YuvTx(txs) => {
                self.event_bus
                    .send(ControllerMessage::P2P(ControllerP2PMessage::YuvTx {
                        txs,
                        sender: addr,
                    }))
                    .await;
            }
            NetworkMessage::Ping(nonce) => {
                if self.pingmgr.received_ping(addr, nonce) {
                    self.addrmgr.peer_active(addr);
                }
            }
            NetworkMessage::Pong(nonce) => {
                if self.pingmgr.received_pong(addr, nonce, now) {
                    self.addrmgr.peer_active(addr);
                }
            }
            NetworkMessage::Verack => {
                if let Some((peer, conn)) = self.peermgr.received_verack(&addr, self.last_tick) {
                    self.clock.record_offset(conn.socket.addr, peer.time_offset);
                    self.addrmgr.peer_negotiated(&addr, peer.services);
                    self.pingmgr.peer_negotiated(conn.socket.addr);

                    self.invmgr.peer_negotiated(conn.socket);
                }
            }
            NetworkMessage::WtxidRelay => {
                self.peermgr.received_wtxidrelay(&addr);
            }
            NetworkMessage::YtxidRelay => {
                self.peermgr.received_ytxidrelay(&addr);
            }
            NetworkMessage::Ytxidack => {
                self.peermgr.received_ytxidack(&addr, self.last_tick);
            }
            NetworkMessage::Version(msg) => {
                self.peermgr.received_version(&addr, msg, &mut self.addrmgr);
            }
            NetworkMessage::GetAddr => {
                self.addrmgr.received_getaddr(&addr);
            }
            NetworkMessage::Addr(addresses) => {
                self.addrmgr.received_addr(addr, addresses);
            }
            _ => debug!("Received undefined message"),
        }
    }

    fn attempted(&mut self, addr: &net::SocketAddr) {
        self.addrmgr.peer_attempted(addr);
        self.peermgr.peer_attempted(addr);
    }

    fn connected(
        &mut self,
        addr: net::SocketAddr,
        local_addr: &net::SocketAddr,
        link: Link,
    ) -> bool {
        if self.addrmgr.is_banned(&addr) {
            debug!("Prevented an attempt of banned peer ({addr}) to connect to us");
            return false;
        }

        if self.peermgr.peer_connected(addr, *local_addr, link) {
            return false;
        }

        self.addrmgr.record_local_address(*local_addr);
        self.addrmgr.peer_connected(&addr);

        true
    }

    async fn disconnected(&mut self, addr: &SocketAddr, reason: Disconnect<DisconnectReason>) {
        self.addrmgr.peer_disconnected(addr, reason.clone());
        self.pingmgr.peer_disconnected(addr);
        self.peermgr
            .peer_disconnected(addr, &mut self.addrmgr, reason)
            .await;
        self.invmgr.peer_disconnected(addr);
    }

    fn is_disconnected(&mut self, addr: SocketAddr) -> bool {
        !self.addrmgr.is_connected(addr)
            || !self.pingmgr.is_connected(&addr)
            || self.peermgr.is_disconnected(&addr)
            || !self.invmgr.is_connected(&addr)
    }

    fn tick(&mut self, local_time: LocalTime) {
        trace!("Received tick");

        self.clock.set(local_time);
    }

    async fn timer_expired(&mut self) {
        trace!("Received wake");

        self.pingmgr.received_wake();
        self.addrmgr.received_wake();
        self.peermgr.received_wake(&mut self.addrmgr).await;

        let local_time = self.clock.local_time();

        if local_time - self.last_tick >= LocalDuration::from_secs(10) {
            let inbound = self.peermgr.negotiated(Link::Inbound).count();
            let connecting = self.peermgr.connecting().count();
            let target = self.peermgr.config.target_outbound_peers;
            let max_inbound = self.peermgr.config.max_inbound_peers;
            let addresses = self.addrmgr.len();

            let mut msg = Vec::new();

            msg.push(format!("inbound = {}/{}", inbound, max_inbound));
            msg.push(format!("connecting = {}/{}", connecting, target));
            msg.push(format!("addresses = {}", addresses));

            debug!(target: "p2p", "{}", msg.join(", "));

            self.last_tick = local_time;
        }
    }

    fn is_connected(&mut self, addr: SocketAddr) -> bool {
        !self.addrmgr.is_banned(&addr)
            || self.peermgr.is_connected(&addr)
            || self.addrmgr.is_connected(addr)
    }

    fn connecting_amount(&self) -> usize {
        self.peermgr.connecting().count()
    }
}

impl<P: peer::Store + Send, C: AdjustedClock<PeerId> + Sync + Send> StateMachine<P, C> {
    /// Construct a new protocol instance.
    pub fn new(
        peers: P,
        clock: C,
        rng: fastrand::Rng,
        config: Config,
        full_event_bus: &EventBus,
    ) -> Self {
        let event_bus = full_event_bus
            .extract(&typeid![ControllerMessage], &[])
            .expect("event channels must be presented");

        let Config {
            network,
            connect,
            services,
            whitelist,
            ping_timeout,
            user_agent,
            required_services,
            limits,
        } = config;

        let outbox = Outbox::new(network);
        let pingmgr = PingManager::new(ping_timeout, rng.clone(), outbox.clone(), clock.clone());
        let peermgr = PeerManager::new(
            peermgr::Config {
                protocol_version: PROTOCOL_VERSION,
                whitelist,
                persistent: connect,
                target_outbound_peers: limits.max_outbound_peers,
                max_inbound_peers: limits.max_inbound_peers,
                retry_max_wait: LocalDuration::from_mins(60),
                retry_min_wait: LocalDuration::from_secs(1),
                required_services,
                services,
                user_agent,
            },
            rng.clone(),
            outbox.clone(),
            clock.clone(),
            network,
        );
        let addrmgr = AddressManager::new(rng.clone(), peers, outbox.clone(), clock.clone());
        let invmgr = InventoryManager::new(outbox.clone());

        Self {
            network,
            clock,
            addrmgr,
            pingmgr,
            peermgr,
            invmgr,
            last_tick: LocalTime::default(),
            rng,
            outbox,
            event_bus,
        }
    }

    /// Disconnect a peer.
    pub fn disconnect(&mut self, addr: PeerId, reason: DisconnectReason) {
        // TODO: Trigger disconnection everywhere, as if peer disconnected. This
        // avoids being in a state where we know a peer is about to get disconnected,
        // but we still process messages from it as normal.

        self.peermgr.disconnect(addr, reason);
    }

    /// Send a message to all negotiated peers matching the predicate.
    fn broadcast<Q>(&mut self, msg: NetworkMessage, predicate: Q) -> Vec<PeerId>
    where
        Q: Fn(&Peer) -> bool,
    {
        let mut peers = Vec::new();

        for (peer_info, connection) in self.peermgr.peers() {
            let peer = Peer::from((peer_info, connection));
            if predicate(&peer) && peer_info.is_negotiated() {
                peers.push(peer.addr);
                self.outbox.message(peer.addr, msg.clone());
            }
        }

        peers
    }

    /// Send a message to the desired peer
    fn send(&mut self, msg: NetworkMessage, addr: PeerId) -> PeerId {
        self.outbox.message(addr, msg);
        addr
    }

    /// Send a message to a random outbound peer. Returns the peer id.
    fn query<Q>(&mut self, msg: NetworkMessage, f: Q) -> Option<PeerId>
    where
        Q: Fn(&Peer) -> bool,
    {
        let peers = self
            .peermgr
            .negotiated(Link::Outbound)
            .map(Peer::from)
            .filter(f)
            .collect::<Vec<_>>();

        match peers.len() {
            n if n > 0 => {
                let r = self.rng.usize(..n);
                let p = peers.get(r).unwrap();

                self.outbox.message(p.addr, msg);

                Some(p.addr)
            }
            _ => None,
        }
    }
}
