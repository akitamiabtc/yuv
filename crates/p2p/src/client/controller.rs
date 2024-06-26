use std::net::SocketAddr;
use std::time::SystemTime;
use std::{net, time};

use async_trait::async_trait;
use flume as chan;

use bitcoin;
use bitcoin::network::{constants::ServiceFlags, Address};

use event_bus::{typeid, EventBus};
use tokio_util::sync::CancellationToken;
use yuv_types::network::Network;
use yuv_types::{
    messages::p2p::{Inventory, NetworkMessage},
    ControllerMessage, YuvTransaction,
};

use crate::{
    client::error::Error,
    client::handle,
    client::peer::Cache,
    client::service::Service,
    common::peer::{KnownAddress, Source, Store},
    common::time::{AdjustedTime, RefClock},
    fsm::handler,
    fsm::handler::PeerId,
    fsm::handler::{Command, Limits, Peer},
    net::{NetReactor, NetWaker},
};

use super::boot_nodes::insert_boot_nodes;

/// P2P client configuration.
#[derive(Debug, Clone)]
pub struct P2PConfig {
    /// Bitcoin network.
    pub network: Network,
    /// Peers to connect
    pub connect: Vec<SocketAddr>,
    /// Client listen address.
    pub listen: SocketAddr,
    /// User agent string.
    pub user_agent: &'static str,
    /// Configured limits (inbound/outbound connections).
    pub limits: Limits,
}

impl P2PConfig {
    /// Create a new configuration for the given network.
    pub fn new(
        network: Network,
        listen: SocketAddr,
        connect: Vec<net::SocketAddr>,
        max_inb: usize,
        max_outb: usize,
    ) -> Self {
        Self {
            network,
            limits: Limits {
                max_outbound_peers: max_outb,
                max_inbound_peers: max_inb,
            },
            listen,
            connect,
            ..Self::default()
        }
    }
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            network: Network::Bitcoin,
            connect: Vec::new(),
            listen: ([0, 0, 0, 0], 0).into(),
            user_agent: handler::USER_AGENT,
            limits: Limits::default(),
        }
    }
}

/// Runs a pre-loaded client.
pub struct P2PClient<R: NetReactor> {
    handle: Handle<R::Waker>,
    service: Service<Cache, RefClock<AdjustedTime<SocketAddr>>>,
    listen: SocketAddr,
    commands: chan::Receiver<Command>,
    reactor: R,
}

impl<R: NetReactor> P2PClient<R> {
    /// Create a new client.
    pub fn new(config: P2PConfig, full_event_bus: &EventBus) -> Result<Self, Error> {
        let (commands_tx, commands_rx) = chan::unbounded::<Command>();

        let (listening_send, listening) = chan::bounded(1);
        let reactor = <R as NetReactor>::new(listening_send)?;

        let event_bus = full_event_bus
            .extract(&typeid![ControllerMessage], &typeid![])
            .expect("event channels must be presented");

        let local_time = SystemTime::now().into();
        let clock = AdjustedTime::<SocketAddr>::new(local_time);
        let rng = fastrand::Rng::new();

        let mut peers = Cache::new();

        insert_boot_nodes(&mut peers, config.network);

        for addr in &config.connect {
            peers.insert(
                addr,
                KnownAddress::new(
                    Address::new(addr, ServiceFlags::NONE),
                    Source::Imported,
                    None,
                ),
            );
        }

        let p2p_service = Service::new(
            peers,
            RefClock::from(clock),
            rng,
            config.clone(),
            &event_bus,
        );

        let listen = config.listen;

        let handle = Handle {
            commands: commands_tx,
            waker: reactor.waker(),
            timeout: time::Duration::from_secs(60),
            listening,
        };

        Ok(P2PClient {
            handle,
            listen,
            commands: commands_rx,
            reactor,
            service: p2p_service,
        })
    }

    /// Run a pre-loaded p2p client.
    pub async fn run(mut self, cancellation: CancellationToken) {
        let result = self
            .reactor
            .run(&self.listen, self.service, self.commands, cancellation)
            .await;

        if let Err(e) = result {
            tracing::error!("P2P is down. P2P client run error: {}", e);
        }
    }

    /// Create a new handle to communicate with the client.
    pub fn handle(&self) -> Handle<R::Waker> {
        self.handle.clone()
    }
}

#[derive(Clone)]
pub struct Handle<W: NetWaker> {
    pub commands: chan::Sender<Command>,
    pub waker: W,
    pub timeout: time::Duration,
    pub listening: chan::Receiver<net::SocketAddr>,
}

impl<W: NetWaker> Handle<W> {
    /// Send a command to the command channel, and wake up the event loop.
    async fn _command(&self, cmd: Command) -> Result<(), handle::Error> {
        if self.commands.send_async(cmd).await.is_err() {
            return Err(handle::Error::Command);
        }
        self.waker.wake()?;

        Ok(())
    }
}

#[async_trait]
impl<W: NetWaker> handle::Handle for Handle<W> {
    async fn command(&self, cmd: Command) -> Result<(), handle::Error> {
        self._command(cmd).await
    }

    async fn broadcast(
        &self,
        msg: NetworkMessage,
        predicate: fn(Peer) -> bool,
    ) -> Result<Vec<net::SocketAddr>, handle::Error> {
        let (transmit, receive) = chan::bounded(1);
        self.command(Command::Broadcast(msg, predicate, transmit))
            .await?;

        match receive.recv_async().await {
            Ok(addr) => Ok(addr),
            Err(_) => Err(handle::Error::Timeout),
        }
    }

    async fn query(&self, msg: NetworkMessage) -> Result<Option<net::SocketAddr>, handle::Error> {
        let (transmit, receive) = chan::bounded::<Option<SocketAddr>>(1);
        self.command(Command::Query(msg, transmit)).await?;

        match receive.recv_async().await {
            Ok(addr) => Ok(addr),
            Err(_) => Err(handle::Error::Timeout),
        }
    }

    async fn send_inv(&self, inv: Vec<Inventory>) -> Result<(), handle::Error> {
        self.command(Command::SendInv(inv)).await?;

        Ok(())
    }

    async fn send_get_data(&self, inv: Vec<Inventory>, addr: PeerId) -> Result<(), handle::Error> {
        self.command(Command::SendGetData(inv, addr)).await?;

        Ok(())
    }

    async fn send_yuv_txs(
        &self,
        txs: Vec<YuvTransaction>,
        addr: PeerId,
    ) -> Result<(), handle::Error> {
        self.command(Command::SendYuvTransactions(txs, addr))
            .await?;

        Ok(())
    }

    async fn ban_peer(&self, addr: SocketAddr) -> Result<(), handle::Error> {
        self.command(Command::BanPeer(addr)).await
    }
}
