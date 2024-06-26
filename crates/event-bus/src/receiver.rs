use std::marker::PhantomData;

use channel::RecvError;
use flume as channel;

use crate::event_bus::BusEvent;

/// Wrapper for async channel receiver.
/// It encapsulates the logic of downcasting the event to the specified type.
#[derive(Clone)]
pub struct Receiver<E: Clone> {
    inner: channel::Receiver<Box<dyn BusEvent>>,
    __event: PhantomData<E>,
}

impl<E: Clone + 'static> Receiver<E> {
    /// Create new receiver from async channel receiver.
    pub fn new(inner: channel::Receiver<Box<dyn BusEvent>>) -> Self {
        Self {
            inner,
            __event: Default::default(),
        }
    }

    /// Receive event from channel.
    pub async fn recv(&self) -> Result<E, RecvError> {
        let event_raw = self.inner.recv_async().await?;

        let event_any = event_raw.as_any();

        match event_any.downcast_ref::<E>() {
            Some(inner) => Ok(inner.clone()),
            None => panic!("invalid event type"),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}
