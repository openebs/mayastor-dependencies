use async_trait::async_trait;
use mbus_nats::{BusResult, BusSubscription};
use message::EventMessage;
use serde::{de::DeserializeOwned, Serialize};

mod common;
pub mod mbus_nats;
pub mod message;

#[async_trait]
pub trait Bus {
    /// Publish a message to message bus.
    async fn publish(&mut self, message: &EventMessage) -> BusResult<u64>;
    /// Create a subscription which can be
    /// polled for messages until the bus is closed.
    async fn subscribe<T: Serialize + DeserializeOwned>(&mut self)
        -> BusResult<BusSubscription<T>>;
}
