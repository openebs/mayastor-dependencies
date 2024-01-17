use crate::event::{
    Component, EventDetails, EventMessage, EventMeta, EventSource, HostInitiatorEventDetails,
    NexusChildEventDetails, NvmePathEventDetails, ReactorEventDetails, RebuildEventDetails,
    RebuildStatus, ReplicaEventDetails, StateChangeEventDetails, SwitchOverEventDetails,
    SwitchOverStatus, Version,
};
use chrono::{DateTime, Utc};
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

    /// Add HA swtich over event specific data to event source.
    pub fn with_switch_over_data(
        self,
        status: SwitchOverStatus,
        start_time: DateTime<Utc>, // switch over start time
        existing_nqn: &str,        // failed nexus path of the volume
        new_path: Option<String>,  // new nexus path of the volume
        retry_count: u64,          // number of failed attempts in the current Stage
    ) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                switch_over_details: Some(SwitchOverEventDetails {
                    switch_over_status: status as i32,
                    start_time: {
                        match start_time.try_into() {
                            Ok(time) => Some(time),
                            Err(_) => None,
                        }
                    },
                    existing_nqn: existing_nqn.to_string(),
                    new_path,
                    retry_count,
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add nexus child event specific data to event source.
    pub fn with_nexus_child_data(self, uri: &str) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                nexus_child_details: Some(NexusChildEventDetails {
                    uri: uri.to_string(),
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add nvme path event specific data to event source.
    pub fn with_nvme_path_data(self, nqn: &str, path: &str) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                nvme_path_details: Some(NvmePathEventDetails {
                    nqn: nqn.to_string(),
                    path: path.to_string(),
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add subsystem event specific data to host event source.
    pub fn with_subsystem_data(self, subsystem_nqn: &str) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                host_initiator_details: Some(HostInitiatorEventDetails {
                    subsystem_nqn: subsystem_nqn.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add target(nexus/replica) details to host event meta data.
    pub fn with_target_data(mut self, target: &str, uuid: &str) -> Self {
        if let Some(mut event_details) = self.event_details {
            if let Some(mut host_initiator_details) = event_details.host_initiator_details {
                host_initiator_details.target = target.to_string();
                host_initiator_details.uuid = uuid.to_string();
                event_details.host_initiator_details = Some(host_initiator_details);
            }
            self.event_details = Some(event_details);
        }
        self
    }

    /// Add host event specific data to event source.
    pub fn with_host_initiator_data(mut self, host_nqn: &str) -> Self {
        if let Some(mut event_details) = self.event_details {
            if let Some(mut host_initiator_details) = event_details.host_initiator_details {
                host_initiator_details.host_nqn = host_nqn.to_string();
                event_details.host_initiator_details = Some(host_initiator_details);
            }
            self.event_details = Some(event_details);
        }
        self
    }

    /// Add reactor event specific data to io-engine event source.
    pub fn with_reactor_details(self, lcore: u64, state: &str) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                reactor_details: Some(ReactorEventDetails {
                    lcore,
                    state: state.to_string(),
                }),
                ..Default::default()
            }),
            ..self
        }
    }

    /// Add state change event specific data to event source.
    pub fn with_state_change_data(self, previous: String, next: String) -> Self {
        EventSource {
            event_details: Some(EventDetails {
                state_change_details: Some(StateChangeEventDetails { previous, next }),
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
            "agent-ha-cluster" => Ok(Self::HaClusterAgent),
            "agent-ha-node" => Ok(Self::HaNodeAgent),
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
