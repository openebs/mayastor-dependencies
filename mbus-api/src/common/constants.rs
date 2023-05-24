use std::time::Duration;

/// Each event message is nearly of size 0.3 KB. So the stream of this size(3 MB) can hold nearly
/// 10K messages.
pub const STREAM_SIZE: i64 = 3 * 1024 * 1024;

/// Stream name for the events.
pub const STREAM_NAME: &str = "events-stream";

/// Stats consumer name for message bus.
pub const CONSUMER_NAME: &str = "stats-events-consumer";

/// Subjects for events stream.
pub const SUBJECTS: &str = "events.>";

/// Timeout for jetstream publish.
pub const PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);

/// Replica count for messages. Maximum 5.
pub const NUM_STREAM_REPLICAS: usize = 3;

/// Max msgs per subject.
pub const MAX_MSGS_PER_SUBJECT: i64 = 1;
