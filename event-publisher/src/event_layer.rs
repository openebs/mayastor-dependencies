use events_api::event::EventMessage;
use std::fmt::Debug;
use tokio::sync::mpsc::{error::TrySendError, Sender};
use tracing::field::{Field, Visit};
use tracing_subscriber::{layer::Context, Layer};

/// Custom layer for recording the events for mbus.
pub struct EventLayer {
    sender: Sender<EventMessage>,
}

impl EventLayer {
    /// Creates the new EventLayer. The sender handle for the buffer is required to store the events
    /// in buffer as the events are recorded.
    pub fn new(sender: Sender<EventMessage>) -> Self {
        Self { sender }
    }
}

// Notifies the EventLayer that an event has occurred.
impl<S> Layer<S> for EventLayer
where
    S: tracing::Subscriber,
{
    // Records an event, gets the EventMessage from the event and sends it to the buffer.
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::default();

        event.record(&mut visitor);

        let output = visitor.0;

        if let Err(err) = self.sender.try_send(output) {
            match err {
                TrySendError::Full(_) => {
                    tracing::trace!("Buffer is full");
                }
                TrySendError::Closed(_) => {
                    tracing::warn!(%err, "Error sending message to buffer");
                    // TODO handle the closed channel situation.
                }
            }
        }
    }
}

// Custom visitor to visit all the fields on on the events being recorded.
#[derive(Default)]
struct EventVisitor(EventMessage);

impl Visit for EventVisitor {
    // Visit a string value. Deserializes the string to EventMessage.
    fn record_str(&mut self, _field: &Field, value: &str) {
        match serde_json::from_str::<EventMessage>(value) {
            Ok(value) => {
                self.0 = value;
            }
            Err(err) => {
                tracing::warn!("Error while getting event message: {:?}", err);
            }
        };
    }

    // Required method.
    fn record_debug(&mut self, _field: &Field, _value: &dyn Debug) {}
}
