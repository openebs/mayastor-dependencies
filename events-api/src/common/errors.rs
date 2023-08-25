use snafu::Snafu;

/// All errors that can be returned from mbus
#[derive(Debug, Snafu)]
#[snafu(visibility(pub), context(suffix(false)))]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    /// Failed to publish message to the mbus.
    #[snafu(display(
        "Jetstream Publish Error. Retried '{}' times. Error: {}. Message : {}",
        retries,
        source,
        payload
    ))]
    PublishError {
        retries: u32,
        payload: String,
        source: async_nats::jetstream::context::PublishError,
    },
    /// Failed to get consumer messages.
    #[snafu(display(
        "Jetstream Error while getting consumer messages from consumer '{}': {}",
        consumer,
        error
    ))]
    ConsumerError { consumer: String, error: String },
    /// Failed to get/create stream.
    #[snafu(display(
        "Jetstream Error while getting/creating stream '{}': {}",
        stream,
        source
    ))]
    StreamError {
        stream: String,
        source: async_nats::jetstream::context::CreateStreamError,
    },
    /// Invalid event message id.
    #[snafu(display("Error while generating subject: {}", error_msg))]
    InvalidMessageId { error_msg: String },
    /// Failed to serialise value.
    #[snafu(display("Failed to serialise value. Error {}", source))]
    SerdeSerializeError { source: serde_json::Error },
}

impl From<serde_json::Error> for Error {
    fn from(source: serde_json::Error) -> Self {
        Self::SerdeSerializeError { source }
    }
}
