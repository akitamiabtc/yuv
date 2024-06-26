#![doc = include_str!("../README.md")]
mod event_bus;

pub use crate::event_bus::{BusEvent, Error, EventBus};
use std::any::TypeId;

mod macros;
mod receiver;

pub use crate::receiver::Receiver;

pub use event_bus_macros::Event;

/// Wraps retrieving [`std::any::TypeId`] for type T.
///
/// Use [`typeid`] macros for vec
pub fn tid<T: 'static>() -> TypeId {
    TypeId::of::<T>()
}
