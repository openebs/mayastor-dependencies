use events_api::{event::EventMessage, mbus_nats::message_bus_init, Bus};

/// Message bus publisher.
#[derive(Clone)]
pub(crate) struct MbusPublisher {}

impl MbusPublisher {
    /// Initializes mbus and runs the publisher.
    pub async fn run(mbus_url: &str, recv: tokio::sync::mpsc::Receiver<EventMessage>) {
        let mbus = message_bus_init(mbus_url, None).await;
        Self::publish_events(mbus, recv).await;
    }

    /// Receives even messages from buffer through receiver handler of the buffer and publishes the
    /// messages to the mbus.
    async fn publish_events(
        mut mbus: impl Bus,
        mut recv: tokio::sync::mpsc::Receiver<EventMessage>,
    ) {
        while let Some(event_msg) = recv.recv().await {
            if let Err(err) = mbus.publish(&event_msg).await {
                tracing::debug!("Error publishing event message to mbus: {:?}", err);
                // TODO retry the event publish when there is publish error if the buffer if
                // not half full.
            }
        }
        // Channel has been closed and there are no remaining messages in the channel's buffer.
    }
}
