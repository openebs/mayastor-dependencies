use serde::{Deserialize, Serialize};

/// General structure of message bus event.
#[derive(Serialize, Deserialize, Debug)]
pub struct EventMessage {
    /// Event Category.
    pub category: Category,
    /// Event Action.
    pub action: Action,
    /// Target id for the category against which action is performed.
    pub target: String,
    /// Event meta data.
    pub metadata: EventMeta,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventMeta {
    /// Something that uniquely identifies events.
    /// UUIDv4.
    /// GUID.
    pub id: String,
    pub source: EventSource,
    /// Event timestamp.
    pub event_timestamp: String,
    /// Version of the event message.
    pub version: String,
}

/// Event source.
#[derive(Serialize, Deserialize, Debug)]
pub struct EventSource {
    /// Io-engine or core-agent.
    pub component: String,
    /// Node name
    pub node: String,
}

/// Event action.
#[derive(Serialize, Deserialize, Debug)]
pub enum Action {
    CreateEvent,
    DeleteEvent,
    Unknown,
}

/// Event category.
#[derive(Serialize, Deserialize, Debug)]
pub enum Category {
    Pool,
    Volume,
    Unknown,
}

impl ToString for Category {
    fn to_string(&self) -> String {
        match self {
            Category::Pool => "pool".to_string(),
            Category::Volume => "volume".to_string(),
            Category::Unknown => "".to_string(),
        }
    }
}
