use crate::event::{Component, EventMessage, EventMeta, EventSource, Version};
use chrono::Utc;
use once_cell::sync::OnceCell;
use std::str::FromStr;

/// Once cell static variable to store the component field of the event source.
static COMPONENT: OnceCell<Component> = OnceCell::new();

/// Initialize the event source component with the service name.
pub fn initilize_source_component(comp: &str) {
    COMPONENT.get_or_init(|| Component::from_str(comp).unwrap_or_default());
}

impl EventMeta {
    /// New event metadata with default values.
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: Some(EventSource::new("".to_string())),
            timestamp: Some(Utc::now().into()),
            version: Version::V1.into(),
        }
    }
}

impl EventSource {
    /// New event source with default values.
    pub fn new(node: String) -> Self {
        let component = COMPONENT.get().cloned().unwrap_or_default().into();
        Self { component, node }
    }
}

impl EventMessage {
    /// Generate the event trace with event message.
    pub fn generate(&self) {
        let event_data = serde_json::to_string(&self).unwrap_or_default();
        tracing::event!(target: "target-mbus", tracing::Level::INFO, event_data);
    }
}

// Get Component from service name.
impl FromStr for Component {
    type Err = String;
    fn from_str(c: &str) -> Result<Self, Self::Err> {
        match c {
            "agent-core" => Ok(Self::CoreAgent),
            "io-engine" => Ok(Self::IoEngine),
            _ => Err(format!("Received event from unknown component {c}")),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{event::*, event_traits::initilize_source_component};

    #[test]
    fn component_initialization_with_unknown_input() {
        initilize_source_component("component");
        let event_meta = EventMeta::new();
        assert_eq!(
            event_meta.source.unwrap().component,
            Component::UnknownComponent as i32
        )
    }

    #[test]
    fn metadata_for_new_event() {
        initilize_source_component("component");
        let event_meta = EventMeta::new();
        assert!(!event_meta.id.is_empty());
        assert!(!event_meta.timestamp.unwrap().to_string().is_empty());
        assert_eq!(event_meta.version, Version::V1 as i32);
        assert_ne!(event_meta.source, None);
    }
}
