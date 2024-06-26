//! Poll-based reactor. This is a single-threaded reactor using a `poll` loop.
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    io,
    io::prelude::*,
    net,
    net::SocketAddr,
    os::unix::io::AsRawFd,
    sync::Arc,
    time,
    time::SystemTime,
};

use async_trait::async_trait;
use flume as chan;
use flume::Receiver;
use popol::Event;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace};

use crate::net::socket::Socket;
use crate::net::time::TimeoutManager;
use crate::net::{
    error, error::Error, Disconnect, Io, Link, LocalDuration, LocalTime, NetReactor, NetWaker,
    PeerId, Service, Source,
};

/// Maximum time to wait when reading from a socket.
const READ_TIMEOUT: time::Duration = time::Duration::from_secs(6);
/// Maximum time to wait when writing to a socket.
const WRITE_TIMEOUT: time::Duration = time::Duration::from_secs(3);
/// Maximum amount of time to wait for i/o.
const WAIT_TIMEOUT: LocalDuration = LocalDuration::from_secs(5);
/// Socket read buffer size.
const READ_BUFFER_SIZE: usize = 1024 * 192;

pub type ReactorTcp = Reactor<net::TcpStream>;

#[derive(Clone)]
pub struct Waker(Arc<popol::Waker>);

impl Waker {
    fn new<Id: PeerId>(sources: &mut popol::Sources<Source<Id>>) -> io::Result<Self> {
        let waker = Arc::new(popol::Waker::new(sources, Source::Waker)?);

        Ok(Self(waker))
    }
}

impl NetWaker for Waker {
    fn wake(&self) -> io::Result<()> {
        self.0.wake()
    }
}

/// A single-threaded non-blocking reactor.
pub struct Reactor<R: Write + Read, Id: PeerId = net::SocketAddr> {
    peers: HashMap<Id, Socket<R>>,
    connecting: HashSet<Id>,
    pub sources: popol::Sources<Source<Id>>,
    waker: Waker,
    timeouts: TimeoutManager<()>,
    listening: chan::Sender<net::SocketAddr>,
}

/// The `R` parameter represents the underlying stream type, eg. `net::TcpStream`.
impl<R: Write + Read + AsRawFd, Id: PeerId> Reactor<R, Id> {
    /// Register a peer with the reactor.
    fn register_peer(&mut self, addr: Id, stream: R, link: Link) {
        let socket_addr = addr.to_socket_addr();
        self.sources
            .register(Source::Peer(addr.clone()), &stream, popol::interest::ALL);
        self.peers
            .insert(addr, Socket::from(stream, socket_addr, link));
    }

    /// Unregister a peer from the reactor.
    pub async fn unregister_peer<S>(
        &mut self,
        addr: Id,
        reason: Disconnect<S::DisconnectReason>,
        service: &mut S,
    ) where
        S: Service<Id>,
    {
        self.connecting.remove(&addr);
        self.peers.remove(&addr);
        self.sources.unregister(&Source::Peer(addr.clone()));

        service.disconnected(&addr, reason).await;
    }
}

#[async_trait]
impl<Id: PeerId + Send + Sync> NetReactor<Id> for Reactor<net::TcpStream, Id> {
    type Waker = Waker;

    /// Construct a new reactor, given a channel to send events on.
    fn new(listening: chan::Sender<net::SocketAddr>) -> Result<Self, io::Error> {
        let peers = HashMap::new();

        let mut sources = popol::Sources::new();
        let waker = Waker::new(&mut sources)?;
        let timeouts = TimeoutManager::new(LocalDuration::from_secs(1));
        let connecting = HashSet::new();

        Ok(Self {
            peers,
            connecting,
            sources,
            waker,
            timeouts,
            listening,
        })
    }

    /// Run the given service with the reactor.
    async fn run<S>(
        &mut self,
        listen_addrs: &SocketAddr,
        mut service: S,
        commands: Receiver<S::Command>,
        cancellation: CancellationToken,
    ) -> Result<(), Error>
    where
        S: Service<Id> + Send + Sync,
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>> + Send + Sync,
    {
        let listener = self.listen_connections(listen_addrs).await.unwrap();

        let local_time = SystemTime::now().into();
        service.initialize(local_time).await;

        self.process(&mut service, local_time).await;

        // I/O readiness events populated by `popol::Sources::wait_timeout`.
        let mut events = Vec::with_capacity(32);
        // Timeouts populated by `TimeoutManager::wake`.
        let mut timeouts: Vec<()> = Vec::with_capacity(32);

        loop {
            select! {
                _ = cancellation.cancelled() => {
                    trace!("Reactor cancelled");
                    break Ok(());
                }
                result = self.handle(&mut events, &mut service, &commands, &mut timeouts, &listener) => {
                    if let Err(e) = result {
                        break Err(e);
                    }
                }
            }
        }
    }

    async fn listen_connections(
        &mut self,
        listen_addr: &SocketAddr,
    ) -> Result<Option<net::TcpListener>, error::Error> {
        let listener = listen(listen_addr)?;
        let local_addr = listener.local_addr()?;

        self.sources
            .register(Source::Listener, &listener, popol::interest::READ);
        self.listening.send_async(local_addr).await.ok();

        debug!(target: "net", "Listening incoming connections on {}", local_addr);

        Ok(Some(listener))
    }

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
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>> + Send + Sync,
    {
        trace!("Woke up with {n} source(s) ready");

        for event in events.drain(..) {
            match &event.key {
                Source::Peer(addr) => {
                    if self
                        .handle_peer_source_event(addr.clone(), &event)
                        .await
                        .unwrap()
                    {
                        continue;
                    }

                    if event.is_writable() {
                        // Outbound message
                        self.handle_writable(addr.clone(), &event.key, service)
                            .await?;
                    }
                    if event.is_readable() {
                        // Inbound message
                        self.handle_readable(addr.clone(), service).await;
                    }
                }
                Source::Listener => self.handle_listener_source(listener, service).await?,
                Source::Waker => self.handle_waker_source(service, commands, event).await,
            }
        }

        Ok(())
    }

    async fn handle_peer_source_event(
        &mut self,
        addr: Id,
        event: &Event<Source<Id>>,
    ) -> Result<bool, error::Error> {
        let socket_addr = addr.to_socket_addr();

        if event.is_error() || event.is_hangup() {
            // Let the subsequent read fail.
            trace!("{}: Socket error triggered: {:?}", socket_addr, event);
        }
        if event.is_invalid() {
            // File descriptor was closed and is invalid.
            // Nb. This shouldn't happen. It means the source wasn't
            // properly unregistered, or there is a duplicate source.
            error!(target: "net", "{}: Socket is invalid, removing", socket_addr);

            self.sources.unregister(&event.key);
            return Ok(true);
        }

        return Ok(false);
    }

    async fn handle_listener_source<S>(
        &mut self,
        listener: &Option<net::TcpListener>,
        service: &mut S,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync,
    {
        while let Some(ref listener) = listener {
            let (conn, socket_addr) = match listener.accept() {
                Ok((conn, socket_addr)) => (conn, socket_addr),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    error!(target: "net", "Accept error: {}", e.to_string());
                    break;
                }
            };

            self.add_connection(service, socket_addr, conn).await?;
        }

        Ok(())
    }

    async fn handle_waker_source<S>(
        &mut self,
        service: &mut S,
        commands: &Receiver<S::Command>,
        event: Event<Source<Id>>,
    ) where
        S: Service<Id> + Send + Sync,
    {
        trace!("Woken up by waker ({} command(s))", commands.len());

        popol::Waker::reset(event.source).ok();

        if let Ok(cmd) = commands.recv_async().await {
            service.command_received(cmd).await;
        }
    }

    /// Return a new waker.
    ///
    /// Used to wake up the main event loop.
    fn waker(&self) -> Self::Waker {
        self.waker.clone()
    }

    async fn add_connection<S>(
        &mut self,
        service: &mut S,
        socket_addr: SocketAddr,
        conn: net::TcpStream,
    ) -> Result<(), error::Error>
    where
        S: Service<Id> + Send + Sync,
    {
        let addr = Id::from(socket_addr);
        trace!("{}: Accepting peer connection", socket_addr);

        conn.set_nonblocking(true)?;

        let local_addr = conn.local_addr()?;
        let link = Link::Inbound;

        self.register_peer(addr.clone(), conn, link);

        service.connected(addr, &local_addr, link);
        Ok(())
    }

    fn is_peer_connected(&mut self, addr: SocketAddr) -> bool {
        self.sources.get(&Source::Peer(addr.into())).is_some()
            || self.peers.get(&addr.into()).is_some()
    }

    fn is_peer_disconnected(&mut self, addr: SocketAddr) -> bool {
        !self.connecting.contains(&addr.into())
            || self.sources.get(&Source::Peer(addr.into())).is_none()
    }
}

impl<Id: PeerId + Send + Sync> Reactor<net::TcpStream, Id> {
    async fn handle<S>(
        &mut self,
        events: &mut Vec<Event<Source<Id>>>,
        service: &mut S,
        commands: &Receiver<S::Command>,
        timeouts: &mut Vec<()>,
        listener: &Option<net::TcpListener>,
    ) -> Result<(), Error>
    where
        S: Service<Id> + Send + Sync,
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>> + Send + Sync,
    {
        tokio::task::yield_now().await;

        trace!(
            "Polling {} source(s) and {} timeout(s), waking up in {:?}..",
            self.sources.len(),
            self.timeouts.len(),
            WAIT_TIMEOUT,
        );

        let result = self.sources.wait_timeout(events, WAIT_TIMEOUT.into()); // Blocking.
        let local_time = SystemTime::now().into();

        service.tick(local_time);

        match result {
            Ok(n) => {
                self.handle_new_source(n, events, service, commands, listener)
                    .await?;
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {
                // Nb. The way this is currently used basically ignores which keys have
                // timed out. So as long as *something* timed out, we wake the service.
                self.timeouts.wake(local_time, timeouts);

                if !timeouts.is_empty() {
                    timeouts.clear();
                    service.timer_expired().await;
                }
            }
            Err(err) => return Err(err.into()),
        }
        self.process(service, local_time).await;

        Ok(())
    }

    /// Process service state machine outputs.
    pub async fn process<S>(&mut self, service: &mut S, local_time: LocalTime)
    where
        S: Service<Id>,
        S::DisconnectReason: Into<Disconnect<S::DisconnectReason>>,
    {
        // Note that there may be messages destined for a peer that has since been
        // disconnected.
        while let Some(out) = service.next() {
            match out {
                Io::Write(addr, bytes) => {
                    if let Some((socket, source)) = self.peers.get_mut(&addr).and_then(|socket| {
                        self.sources
                            .get_mut(&Source::Peer(addr))
                            .map(|source| (socket, source))
                    }) {
                        socket.push(&bytes);
                        source.set(popol::interest::WRITE);
                    }
                }
                Io::Connect(addr) => self.handle_connect_process(addr, service).await,
                Io::Disconnect(addr, reason) => {
                    // Shutdown the connection, ignoring any potential errors.
                    // If the socket was already disconnected, this will yield
                    // an error that is safe to ignore (`ENOTCONN`). The other
                    // possible errors relate to an invalid file descriptor.
                    self.peers
                        .get(&addr)
                        .and_then(|peer| peer.disconnect().ok());
                    self.unregister_peer(addr, reason.into(), service).await;
                }
                Io::SetTimer(timeout) => {
                    self.timeouts.register((), local_time + timeout);
                }
                Io::Event(event) => {
                    trace!("Event: {:?}", event);
                }
            }
        }
    }

    async fn handle_connect_process<S: Service<Id>>(&mut self, addr: Id, service: &mut S) {
        let socket_addr = addr.to_socket_addr();

        match dial(&socket_addr) {
            Ok(stream) => {
                trace!("{:#?}", stream);

                self.register_peer(addr.clone(), stream, Link::Outbound);
                self.connecting.insert(addr.clone());

                service.attempted(&addr);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // Ignore. We are already establishing a connection through
                // this socket.
            }
            Err(err) => {
                error!(target: "net", "{}: Dial error: {}", socket_addr, err.to_string());

                service
                    .disconnected(&addr, Disconnect::DialError(Arc::new(err)))
                    .await;
            }
        }
    }

    async fn handle_readable<S>(&mut self, addr: Id, service: &mut S)
    where
        S: Service<Id>,
    {
        // Nb. If the socket was readable and writable at the same time, and it was disconnected
        // during an attempt to write, it will no longer be registered and hence available
        // for reads.
        if let Some(socket) = self.peers.get_mut(&addr) {
            let mut buffer = [0; READ_BUFFER_SIZE];

            let socket_addr = addr.to_socket_addr();
            trace!("{}: Socket is readable", socket_addr);

            // Nb. Since `poll`, which this reactor is based on, is *level-triggered*,
            // we will be notified again if there is still data to be read on the socket.
            // Hence, there is no use in putting this socket read in a loop, as the second
            // invocation would likely block.
            match socket.read(&mut buffer) {
                Ok(count) => {
                    if count > 0 {
                        service
                            .message_received(&addr, Cow::Borrowed(&buffer[..count]))
                            .await;
                    } else {
                        // If we get zero bytes read as a return value, it means the peer has
                        // performed an orderly shutdown.
                        socket.disconnect().ok();

                        self.unregister_peer(
                            addr,
                            Disconnect::ConnectionError(Arc::new(io::Error::from(
                                io::ErrorKind::ConnectionReset,
                            ))),
                            service,
                        )
                        .await;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    error!("would block");
                    // This shouldn't normally happen, since this function is only called
                    // when there's data on the socket. We leave it here in case external
                    // conditions change.
                }
                Err(err) => {
                    trace!("{}: Read error: {}", socket_addr, err.to_string());

                    socket.disconnect().ok();
                }
            }
        }
    }

    pub async fn handle_writable<S: Service<Id>>(
        &mut self,
        addr: Id,
        source: &Source<Id>,
        service: &mut S,
    ) -> io::Result<()> {
        let socket_addr = addr.to_socket_addr();
        trace!("{}: Socket is writable", socket_addr);

        let source = self.sources.get_mut(source).unwrap();
        let socket = self.peers.get_mut(&addr).unwrap();

        // "A file descriptor for a socket that is connecting asynchronously shall indicate
        // that it is ready for writing, once a connection has been established."
        //
        // Since we perform a non-blocking connect, we're only really connected once the socket
        // is writable.
        if self.connecting.remove(&addr) {
            let local_addr = socket.local_address()?;

            service.connected(addr.clone(), &local_addr, socket.link);
        }

        match socket.flush() {
            // In this case, we've written all the data, we
            // are no longer interested in writing to this
            // socket.
            Ok(()) => {
                source.unset(popol::interest::WRITE);
            }
            // In this case, the write couldn't complete. Set
            // our interest to `WRITE` to be notified when the
            // socket is ready to write again.
            Err(err)
                if [io::ErrorKind::WouldBlock, io::ErrorKind::WriteZero].contains(&err.kind()) =>
            {
                source.set(popol::interest::WRITE);
            }
            Err(err) => {
                error!(target: "net", "{}: Write error: {}", socket_addr, err.to_string());

                socket.disconnect().ok();
                self.unregister_peer(addr, Disconnect::ConnectionError(Arc::new(err)), service)
                    .await;
            }
        }
        Ok(())
    }
}

/// Connect to a peer given a remote address.
fn dial(addr: &SocketAddr) -> Result<net::TcpStream, io::Error> {
    use socket2::{Domain, Socket, Type};

    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let sock = Socket::new(domain, Type::STREAM, None)?;

    sock.set_read_timeout(Some(READ_TIMEOUT))?;
    sock.set_write_timeout(Some(WRITE_TIMEOUT))?;
    sock.set_nonblocking(true)?;

    match sock.connect(&(*addr).into()) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
        Err(e) if e.raw_os_error() == Some(libc::EALREADY) => {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists))
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) => return Err(e),
    }
    Ok(sock.into())
}

/// Listen for connections on the given address.
fn listen<A: net::ToSocketAddrs>(addr: A) -> Result<net::TcpListener, Error> {
    let sock = net::TcpListener::bind(addr)?;

    sock.set_nonblocking(true)?;

    Ok(sock)
}
