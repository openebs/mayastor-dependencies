use crate::event::{
    Component, EventDetails, EventMessage, EventMeta, EventSource, RebuildEventDetails,
    RebuildStatus, ReplicaEventDetails, Version,
};
use chrono::Utc;
use once_cell::sync::OnceCell;
use std::str::FromStr;

/// Once cell static variable to store the component field of the event source.
static COMPONENT: OnceCell<Component> = OnceCell::new();

/// Initialize the event source component with the service name.
pub fn initialize_source_component(comp: &str) {
    COMPONENT.get_or_init(|| Component::from_str(comp).unwrap_or_default());
}

impl EventMeta {
    /// New event metadata with default values.
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: Some(EventSource::new("".to_string())),
            timestamp: Some(Utc::now().into()),
            version: Version::V1.into(),
        }
    }

    /// Event metadata with given source.
    pub fn from_source(source: EventSource) -> Self {
        Self {
            source: Some(source),
            ..Self::new()
        }
    }
}

impl EventSource {
    /// New event source with default values.
    pub fn new(node: String) -> Self {
        let component = COMPONENT.get().cloned().unwrap_or_default().into();
        Self {
            component,
            node,
            ..Default::default()
        }
    }

    /// Add rebuild event specific data to event source.
    pub fn with_rebuild_data(
        self,
        status: RebuildStatus, // Rebuild status
        source: &str,          // Rebuild source replica uri
        destination: &str,     // Rebuild destination replica uri
        error: Option<String>, // Rebuild error for RebuildFail event
    ) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                rebuild_details: Some(RebuildEventDetails {
                    rebuild_status: status as i32,
                    source_replica: source.to_string(),
                    destination_replica: destination.to_string(),
                    error,
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add replica event specific data to event source.
    pub fn with_replica_data(self, pool_name: &str, pool_uuid: &str, replica_name: &str) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                replica_details: Some(ReplicaEventDetails {
                    pool_name: pool_name.to_string(),
                    pool_uuid: pool_uuid.to_string(),
                    replica_name: replica_name.to_string(),
                }),
                ..Default::default()
            }),
            ..self
        }
    }
}

impl EventMessage {
    /// Generate the event trace with event message.
    pub fn generate(&self) {
        let event_data = serde_json::to_string(&self).unwrap_or_default();
        tracing::event!(target: "mbus-events-target", tracing::Level::INFO, event_data);
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
    use crate::{event::*, event_traits::initialize_source_component};

    #[test]
    fn component_initialization_with_unknown_input() {
        initialize_source_component("component");
        let event_source = EventSource::new("".to_string());
        let event_meta = EventMeta::from_source(event_source);
        assert_eq!(
            event_meta.source.unwrap().component,
            Component::UnknownComponent as i32
        )
    }

    #[test]
    fn metadata_for_new_event() {
        initialize_source_component("component");
        let event_source = EventSource::new("".to_string());
        let event_meta = EventMeta::from_source(event_source);
        assert!(!event_meta.id.is_empty());
        assert!(event_meta.timestamp.is_some());
        assert_eq!(event_meta.version, Version::V1 as i32);
        assert_ne!(event_meta.source, None);
    }
}
