//! Peer-to-peer networking core types.
#![allow(clippy::type_complexity)]
extern crate core;

use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    io, net,
    net::{SocketAddr, TcpListener},
    sync::Arc,
};

use async_trait::async_trait;
use flume as chan;
use flume::Receiver;
use popol::Event;
use tokio_util::sync::CancellationToken;

pub use reactor::{ReactorTcp, Waker};
pub use time::{LocalDuration, LocalTime};

pub mod error;
pub mod reactor;
mod socket;
pub mod time;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Source<Id: PeerId> {
    Peer(Id),
    Listener,
    Waker,
}

/// Link direction of the peer connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Inbound conneciton.
    Inbound,
    /// Outbound connection.
    Outbound,
}

impl Link {
    /// Check whether the link is outbound.
    pub fn is_outbound(&self) -> bool {
        *self == Link::Outbound
    }

    /// Check whether the link is inbound.
    pub fn is_inbound(&self) -> bool {
        *self == Link::Inbound
    }
}

/// Output of a state transition of the state machine.
#[derive(Debug)]
pub enum Io<M, E, D, Id: PeerId = net::SocketAddr> {
    /// There are some bytes ready to be sent to a peer.
    Write(Id, M),
    /// Connect to a peer.
    Connect(Id),
    /// Disconnect from a peer.
    Disconnect(Id, D),
    /// Ask for a wakeup in a specified amount of time.
    SetTimer(LocalDuration),
    /// Emit an event.
    Event(E),
}

/// Disconnection event which includes the reason.
#[derive(Debug, Clone)]
pub enum Disconnect<T> {
    /// Error while dialing the remote. This error occures before a connection is
    /// even established. Errors of this kind are usually not transient.
    DialError(Arc<std::io::Error>),
    /// Error with an underlying established connection. Sometimes, reconnecting
    /// after such an error is possible.
    ConnectionError(Arc<std::io::Error>),
    /// Peer was disconnected for another reason.
    StateMachine(T),
    /// Peer is banned due to the violation of protocol rules
    PeerBanned,
}

impl<T> Disconnect<T> {
    pub fn is_dial_err(&self) -> bool {
        matches!(self, Self::DialError(_))
    }

    pub fn is_banned(&self) -> bool {
        matches!(self, Self::PeerBanned)
    }

    pub fn is_connection_err(&self) -> bool {
        matches!(self, Self::ConnectionError(_))
    }
}

impl<T: fmt::Display> fmt::Display for Disconnect<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DialError(err) => write!(f, "{}", err),
            Self::ConnectionError(err) => write!(f, "{}", err),
            Self::StateMachine(reason) => write!(f, "{}", reason),
            Self::PeerBanned => write!(f, "peer was banned"),
        }
    }
}

/// Remote peer id, which must be convertible into a [`net::SocketAddr`]
pub trait PeerId: Eq + Ord + Clone + Hash + fmt::Debug + From<net::SocketAddr> {
    fn to_socket_addr(&self) -> net::SocketAddr;
}

impl<T> PeerId for T
where
    T: Eq + Ord + Clone + Hash + fmt::Debug,
    T: Into<net::SocketAddr>,
    T: From<net::SocketAddr>,
{
    fn to_socket_addr(&self) -> net::SocketAddr {
        self.clone().into()
    }
}

/// A network service.
///
/// Network protocols must implement this trait to be drivable by the reactor.
#[async_trait]
pub trait Service<Id: PeerId = net::SocketAddr>: StateMachine<Id, Message = [u8]> {
    /// Commands handled by the service. These commands should originate from an
    /// external "user" thread. They are passed through the reactor via a channel
    /// given to Reactor::run. The reactor calls [`Service::command_received`]
    /// on the service for each command received.
    type Command: Send + Sync;

    /// An external command has been received.
    async fn command_received(&mut self, cmd: Self::Command);
}

/// A service state-machine to implement a network protocol's logic.
///
/// This trait defines an API for connecting specific protocol domain logic to a
/// Reactor. It is parametrized by a peer id, which is shared between the reactor
/// and state machine.
///
/// The state machine emits [`Io`] instructions to the reactor via its [`Iterator`] trait.
#[async_trait]
pub trait StateMachine<Id: PeerId = net::SocketAddr>:
    Iterator<Item = Io<<Self::Message as ToOwned>::Owned, Self::Event, Self::DisconnectReason, Id>>
{
    /// Message type sent between peers.
    type Message: fmt::Debug + ToOwned + ?Sized;
    /// Events emitted by the state machine.
    /// These are forwarded by the reactor to the user thread.
    type Event: fmt::Debug + Send;
    /// Reason a peer was disconnected, in case the peer was disconnected by the internal
    /// state-machine logic.
    type DisconnectReason: fmt::Debug + fmt::Display + Into<Disconnect<Self::DisconnectReason>>;

    /// Initialize the state machine. Called once before any event is sent to the state machine.
    async fn initialize(&mut self, _time: LocalTime) {}
    /// Called by the reactor upon receiving a message from a remote peer.
    async fn message_received(&mut self, addr: &Id, message: Cow<'_, Self::Message>);
    /// Connection attempt underway.
    ///
    /// This is only encountered when an outgoing connection attempt is made,
    /// and is always called before [`StateMachine::connected`].
    ///
    /// For incoming connections, [`StateMachine::connected`] is called directly.
    fn attempted(&mut self, addr: &Id);
    /// New connection with a peer.
    fn connected(&mut self, addr: Id, local_addr: &net::SocketAddr, link: Link) -> bool;
    /// Called whenever a remote peer was disconnected, either because of a
    /// network-related event or due to a local instruction from this state machine,
    /// using [`Io::Disconnect`].
    async fn disconnected(&mut self, addr: &Id, reason: Disconnect<Self::DisconnectReason>);
    fn is_disconnected(&mut self, addr: SocketAddr) -> bool;
    /// Called by the reactor every time the event loop gets data from the network, or times out.
    /// Used to update the state machine's internal clock.
    ///
    /// "a regular short, sharp sound, especially that made by a clock or watch, typically
    /// every second."
    fn tick(&mut self, local_time: LocalTime);
    /// A timer set with [`Io::SetTimer`] has expired.
    async fn timer_expired(&mut self);
    fn is_connected(&mut self, addr: net::SocketAddr) -> bool;
    fn connecting_amount(&self) -> usize;
}

/// Used by certain types of reactors to wake the event loop, for example when a
/// [`Service::Command`] is ready to be processed by the service.
pub trait NetWaker: Send + Sync + Clone {
    /// Wake up! Call this after sending a command to make sure the command is processed
    /// in a timely fashion.
    fn wake(&self) -> io::Result<()>;
}

/// Any network reactor that can drive the light-client service.
#[async_trait]
pub trait NetReactor<Id: PeerId = net::SocketAddr> {
    /// The type of waker this reactor uses.
    type Waker: NetWaker;

    /// Create a new reactor, initializing it with a publisher for service events,
    /// a channel to receive commands, and a channel to shut it down.
    fn new(listening: chan::Sender<net::SocketAddr>) -> Result<Self, io::Error>
    where
        Self: Sized;

    /// Run the given service with the reactor.
    ///
    /// Takes:
    ///
    /// * The addresses to listen for connections on.
    /// * The [`Service`] to run.
    /// * The [`StateMachine::Event`] publisher to use when the service emits events.
    /// * The [`Service::Command`] channel on which commands will be received.
    async fn run<S>(
        &mut self,
        listen_addrs: &SocketAddr,
        service: S,
        commands: chan::Receiver<S::Command>,
        cancellation: CancellationToken,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync,
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>> + Send + Sync;

    async fn listen_connections(
        &mut self,
        listen_addrs: &SocketAddr,
    ) -> Result<Option<net::TcpListener>, error::Error>;

    async fn handle_new_source<S>(
        &mut self,
        n: usize,
        events: &mut Vec<Event<Source<Id>>>,
        service: &mut S,
        commands: &Receiver<S::Command>,
        listener: &Option<net::TcpListener>,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync,
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>> + Send + Sync;

    /// Handles Peer source event that came to the reactor. Returns true, if event is invalid
    async fn handle_peer_source_event(
        &mut self,
        addr: Id,
        event: &Event<Source<Id>>,
    ) -> Result<bool, error::Error>;

    /// Handles Listener source that came to the reactor
    async fn handle_listener_source<S>(
        &mut self,
        listener: &Option<TcpListener>,
        service: &mut S,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync;

    /// Handles Peer source event that came to the reactor. Returns true, if shutdown command was received
    async fn handle_waker_source<S>(
        &mut self,
        service: &mut S,
        commands: &Receiver<S::Command>,
        event: Event<Source<Id>>,
    ) where
        S: Service<Id> + Send + Sync;

    /// Return a new waker.
    ///
    /// The reactor can provide multiple wakers such that multiple user threads may wake
    /// the event loop.
    fn waker(&self) -> Self::Waker;

    async fn add_connection<S>(
        &mut self,
        service: &mut S,
        socket_addr: SocketAddr,
        conn: net::TcpStream,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync;

    /// Checks if provided peer is in storages. Used for testing
    fn is_peer_connected(&mut self, addr: SocketAddr) -> bool;
    fn is_peer_disconnected(&mut self, addr: SocketAddr) -> bool;
}
