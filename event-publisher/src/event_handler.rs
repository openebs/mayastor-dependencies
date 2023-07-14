use crate::{
    common::constants::MAX_BUFFER_MSGS, event_layer::EventLayer, publisher::MbusPublisher,
};
use events_api::{event::EventMessage, event_traits::initilize_source_component};

/// Event handle.
pub struct EventHandle {}

impl EventHandle {
    /// Initilize the buffer, starts the event publisher and return the layer for handling the event
    /// traces.
    pub fn init(mbus_url: String, service_name: &str) -> EventLayer {
        let (send, recv) = tokio::sync::mpsc::channel::<EventMessage>(MAX_BUFFER_MSGS);
        initilize_source_component(service_name);
        let layer = EventLayer::new(send);
        tokio::spawn(async move {
            MbusPublisher::run(&mbus_url, recv).await;
            // TODO handle the closed channel situation.
        });
        layer
    }
}
