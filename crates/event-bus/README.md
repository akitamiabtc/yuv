# `event-bus`

Utility crate that offers a simple interface for managing event channels between internal services. It provides the ability to create an EventBus instance which can be utilized to publish events and subscribe to them. One key feature is its dynamic functionality, eliminating the need to specify all events in an enum or struct and then update it whenever a new event arises. Instead, you simply call the register method with the event type as a generic parameter, enabling you to publish and subscribe to this event seamlessly.

Example of using an `EventBus`:

```rust
use event_bus::{EventBus, BusEvent, Receiver, typeid};
use std::any::TypeId;
use event_bus_macros::Event;

#[derive(Clone, Event)]
struct MyEvent {
    id: u32,
}

tokio_test::block_on(async {
    // Creating event bus.
    let mut full_event_bus = EventBus::default();
    let mut full_event_bus = EventBus::default();

    // Registering unbounded channel for MyEvent event.
    full_event_bus.register::<MyEvent>(None);

    // Extracting channel for MyEvent event. You aren't obligated
    // to extract channels, so you can use ful_event_bus instead.
    let mut event_bus = full_event_bus.extract(&typeid![MyEvent], &typeid![MyEvent]).unwrap();

    // Sending event to channel.
    event_bus.send(MyEvent { id: 1 }).await;

    // Subscribing to channel.
    let mut receiver: Receiver<MyEvent> = event_bus.subscribe();

    // Receiving event from subscribed channel.
    let event = receiver.recv().await.unwrap();
});
```
