use crate::{tid, Receiver};

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::{Debug, Display, Formatter},
};

use flume as channel;

pub type EventBusResult<T> = Result<T, Error>;

type InnerSender = channel::Sender<Box<dyn BusEvent>>;
type InnerReceiver = channel::Receiver<Box<dyn BusEvent>>;

/// Trait for events that can be sent through event bus. Use [`event_bus_macros::Event`] derive
/// macro to implement it.
///
/// Declares `as_any` method that returns reference to `dyn Any`. It is used to downcast event
/// to concrete type during the [`EventBus::subscribe()`] method call.
pub trait BusEvent: Send {
    fn as_any(&self) -> &dyn Any;
}

/// Event bus that provides a simple interface for managing event channels between different parts
/// of application. It is based on [`flume`].
///
/// Use [`EventBus::extract`] method to extract subset of channels from existing event bus.
///
/// # Examples
/// ```
/// use event_bus::{EventBus, BusEvent, Receiver, typeid};
/// use std::any::TypeId;
/// use event_bus_macros::Event;
///
/// #[derive(Clone, Event)]
/// struct MyEvent {
///    id: u32,
/// }
///
/// # tokio_test::block_on(async {
/// // Creating event bus.
/// let mut full_event_bus = EventBus::default();
/// let mut full_event_bus = EventBus::default();
///
/// // Registering channel for MyEvent event.
/// full_event_bus.register::<MyEvent>(None);
///
/// // Extracting channel for MyEvent event. You aren't obligated
/// // to extract channels, so you can use full_event_bus instead.
/// let mut event_bus = full_event_bus.extract(&typeid![MyEvent], &typeid![MyEvent]).unwrap();
///
/// // Sending event to channel.
/// event_bus.send(MyEvent { id: 1 }).await;
///
/// // Subscribing to channel.
/// let mut receiver: Receiver<MyEvent> = event_bus.subscribe();
///
/// // Receiving event from subscribed channel.
/// let event = receiver.recv().await.unwrap();
/// # });
/// ```
#[derive(Clone, Default, Debug)]
pub struct EventBus {
    /// Map of event type id to channel sender.
    txs: HashMap<TypeId, InnerSender>,

    /// Map of event type id to channel receiver.
    rxs: HashMap<TypeId, InnerReceiver>,
}

impl EventBus {
    /// Register channel by creating sender and receiver for specified event type. it will be
    /// unbounded. If channel is already registered, method will return true otherwise false.
    ///
    /// It is possible to specify channel size as optional parameter. If channel size is not specified
    pub fn register<E: BusEvent + Clone + 'static>(&mut self, channel_size: Option<usize>) -> bool {
        if self.txs.contains_key(&tid::<E>()) {
            return true;
        }

        let (tx, rx) = match channel_size {
            Some(size) => channel::bounded::<Box<dyn BusEvent>>(size),
            None => channel::unbounded::<Box<dyn BusEvent>>(),
        };

        self.txs.insert(tid::<E>(), tx);
        self.rxs.insert(tid::<E>(), rx);

        false
    }

    /// Extract subset of channels from existing event bus. If channel for specified event type
    /// doesn't exist, method will return [`Error::ChannelForTypeIdDoesntExist`].
    ///
    /// Use [`typeid`](`crate::typeid`) macros for vec of event type ids.
    pub fn extract(&self, tx_ids: &[TypeId], rx_ids: &[TypeId]) -> EventBusResult<Self> {
        Ok(Self {
            txs: new_hashmap_with::<InnerSender>(&self.txs, tx_ids)?,
            rxs: new_hashmap_with::<InnerReceiver>(&self.rxs, rx_ids)?,
        })
    }

    /// Subscribe to channel by returning [`Receiver`] for specified event type. If channel for
    /// specified event type doesn't exist, method will panic. Use [`EventBus::try_subscribe`] to
    /// avoid panic.
    pub fn subscribe<E: BusEvent + Clone + 'static>(&self) -> Receiver<E> {
        let rx = self
            .rxs
            .get(&tid::<E>())
            .expect("channel for event must be presented")
            .clone();

        Receiver::new(rx)
    }

    /// Try subscribe to channel by returning [`Receiver`] for specified event type. If channel for
    /// specified event type doesn't exist, method will return [`Error::ChannelForTypeIdDoesntExist`].
    pub fn try_subscribe<E: BusEvent + Clone + 'static>(&self) -> EventBusResult<Receiver<E>> {
        let rx = self
            .rxs
            .get(&tid::<E>())
            .ok_or(Error::ChannelForTypeIdDoesntExist)?
            .clone();

        Ok(Receiver::new(rx))
    }

    /// Send event to channel. If channels for specified event isn't registered
    /// ([`EventBus::register`]), method will panic. Use [`EventBus::try_send`] to avoid panic.
    ///
    /// If channel size is specified and channel is full, the method will block until there is a
    /// space in channel.
    pub async fn send<E: BusEvent + 'static>(&self, event: E) {
        let channel = self
            .txs
            .get(&tid::<E>())
            .expect("channel for event must be presented");

        channel
            .send_async(Box::new(event))
            .await
            .expect("async channel already closed");
    }

    /// Try send event to channel. If channels for specified event isn't registered method will
    /// return [`Error::ChannelForTypeIdDoesntExist`].
    pub async fn try_send<E: BusEvent + 'static>(&self, event: E) -> EventBusResult<()> {
        let channel = self
            .txs
            .get(&tid::<E>())
            .ok_or(Error::ChannelForTypeIdDoesntExist)?;

        channel
            .send_async(Box::new(event))
            .await
            .map_err(Error::ChannelSend)?;

        Ok(())
    }
}

fn new_hashmap_with<Channel: Clone>(
    src: &HashMap<TypeId, Channel>,
    event_ids: &[TypeId],
) -> EventBusResult<HashMap<TypeId, Channel>> {
    let mut extracted_channels: HashMap<TypeId, Channel> = Default::default();

    for event_id in event_ids {
        extracted_channels.insert(
            *event_id,
            src.get(event_id)
                .ok_or(Error::ChannelForTypeIdDoesntExist)?
                .clone(),
        );
    }

    Ok(extracted_channels)
}

#[derive(Debug)]
pub enum Error {
    ChannelSend(channel::SendError<Box<dyn BusEvent>>),
    ChannelForTypeIdDoesntExist,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelSend(inner) => {
                write!(f, "failed to send message to channel: {inner}")
            }
            Self::ChannelForTypeIdDoesntExist => {
                write!(f, "channel for event id doesn't exist")
            }
        }
    }
}

impl std::error::Error for Error {}
