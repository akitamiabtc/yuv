use async_trait::async_trait;
use bitcoin::consensus::Encodable;
use event_bus::EventBus;
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::net;
use tracing::{debug, error};

use crate::{
    client,
    client::P2PConfig,
    common::peer,
    common::time::AdjustedClock,
    fsm,
    net::LocalTime,
    net::StateMachine,
    net::{Disconnect, Io, Link},
};

/// Client service. Wraps a state machine and handles decoding and encoding of network messages.
pub struct Service<P, C> {
    inboxes: HashMap<net::SocketAddr, client::stream::Decoder>,
    machine: fsm::handler::StateMachine<P, C>,
}

impl<P: peer::Store + Send, C: AdjustedClock<net::SocketAddr> + Sync + Send> Service<P, C> {
    /// Create a new client service.
    pub fn new(
        peers: P,
        clock: C,
        rng: fastrand::Rng,
        config: P2PConfig,
        full_event_bus: &EventBus,
    ) -> Self {
        Self {
            inboxes: HashMap::new(),
            machine: fsm::handler::StateMachine::new(
                peers,
                clock,
                rng,
                fsm::handler::Config {
                    network: config.network,
                    connect: config.connect,
                    user_agent: config.user_agent,
                    limits: config.limits,

                    ..fsm::handler::Config::default()
                },
                full_event_bus,
            ),
        }
    }
}

#[async_trait]
impl<P, C> crate::net::Service for Service<P, C>
where
    P: peer::Store + Send,
    C: AdjustedClock<net::SocketAddr> + Sync + Send,
{
    type Command = fsm::handler::Command;

    async fn command_received(&mut self, cmd: Self::Command) {
        self.machine.command(cmd).await
    }
}

#[async_trait]
impl<P, C> StateMachine for Service<P, C>
where
    P: peer::Store + Send,
    C: AdjustedClock<net::SocketAddr> + Sync + Send,
{
    type Message = [u8];
    type Event = crate::fsm::event::Event;
    type DisconnectReason = fsm::handler::DisconnectReason;

    async fn initialize(&mut self, time: LocalTime) {
        self.machine.initialize(time).await;
    }

    async fn message_received(&mut self, addr: &net::SocketAddr, bytes: Cow<'_, [u8]>) {
        let Some(inbox) = self.inboxes.get_mut(addr) else {
            debug!("Received message from unknown peer {}", addr);
            return;
        };

        inbox.input(bytes.borrow());

        loop {
            match inbox.decode_next() {
                Ok(Some(msg)) => self.machine.message_received(addr, Cow::Owned(msg)).await,
                Ok(None) => break,
                Err(err) => {
                    error!("Invalid message received from {}. Error: {}", addr, err);
                    self.machine
                        .disconnect(*addr, fsm::handler::DisconnectReason::DecodeError);
                    return;
                }
            }
        }
    }

    fn attempted(&mut self, addr: &net::SocketAddr) {
        self.machine.attempted(addr)
    }

    fn connected(
        &mut self,
        addr: net::SocketAddr,
        local_addr: &net::SocketAddr,
        link: Link,
    ) -> bool {
        if !self.machine.connected(addr, local_addr, link) {
            return false;
        }
        self.inboxes
            .insert(addr, client::stream::Decoder::new(1024));
        true
    }

    async fn disconnected(
        &mut self,
        addr: &net::SocketAddr,
        reason: Disconnect<Self::DisconnectReason>,
    ) {
        self.inboxes.remove(addr);
        self.machine.disconnected(addr, reason).await
    }

    fn is_disconnected(&mut self, addr: net::SocketAddr) -> bool {
        return self.inboxes.get(&addr).is_none() || self.machine.is_disconnected(addr);
    }

    fn tick(&mut self, local_time: LocalTime) {
        self.machine.tick(local_time);
    }

    async fn timer_expired(&mut self) {
        self.machine.timer_expired().await;
    }

    fn is_connected(&mut self, addr: net::SocketAddr) -> bool {
        return self.inboxes.get(&addr).is_some() || self.machine.is_connected(addr);
    }

    fn connecting_amount(&self) -> usize {
        self.machine.peermgr.connecting().count()
    }
}

impl<P, C> Iterator for Service<P, C> {
    type Item = Io<Vec<u8>, crate::fsm::event::Event, fsm::handler::DisconnectReason>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.machine.next() {
            Some(Io::Write(addr, msg)) => {
                let mut buf = Vec::new();

                msg.consensus_encode(&mut buf)
                    .expect("writing to an in-memory buffer doesn't fail");
                Some(Io::Write(addr, buf))
            }
            Some(Io::Event(e)) => Some(Io::Event(e)),
            Some(Io::Connect(a)) => Some(Io::Connect(a)),
            Some(Io::Disconnect(a, r)) => Some(Io::Disconnect(a, r)),
            Some(Io::SetTimer(d)) => Some(Io::SetTimer(d)),

            None => None,
        }
    }
}
