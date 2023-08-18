use crate::{
    common::{
        constants::*,
        errors::Error,
        retry::{backoff_with_options, BackoffOptions},
    },
    event::EventMessage,
    Bus,
};
use async_nats::{
    jetstream::{
        self,
        consumer::{
            push::{Config, Messages, MessagesErrorKind},
            DeliverPolicy,
        },
        context::PublishErrorKind,
        stream::Stream,
        Context,
    },
    Client,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, time::Duration};

/// Result wrapper for Jetstream requests.
pub type BusResult<T> = Result<T, Error>;

/// Initialise the Nats Message Bus.
pub async fn message_bus_init(server: &str, msg_replicas: Option<usize>) -> impl crate::Bus {
    let bus = NatsMessageBus::new(server).await;
    let _ = bus.get_or_create_stream(None, msg_replicas).await;
    bus
}

/// Nats implementation of the Bus.
#[derive(Clone)]
pub(crate) struct NatsMessageBus {
    client: Client,
    jetstream: Context,
}

impl NatsMessageBus {
    /// Connect to nats server.
    async fn connect(server: &str) -> Client {
        tracing::debug!("Connecting to the nats server {}...", server);
        // We retry in a loop until successful. Once connected the nats library will handle
        // reconnections for us.
        let options = BackoffOptions::new()
            .with_init_delay(Duration::from_secs(5))
            .with_cutoff(4)
            .with_delay_step(Duration::from_secs(2))
            .with_max_delay(Duration::from_secs(10));
        let mut tries = 0;
        let mut log_error = true;
        loop {
            match async_nats::ConnectOptions::new()
                .event_callback(|event| async move {
                    match event {
                        async_nats::Event::Disconnected => {
                            tracing::warn!("NATS connection has been lost")
                        }
                        async_nats::Event::Connected => {
                            tracing::debug!("NATS connection has been reestablished")
                        }
                        _ => (),
                    }
                })
                .connect(server)
                .await
            {
                Ok(client) => {
                    tracing::debug!("Connected to the nats server {}", server);
                    return client;
                }
                Err(error) => {
                    if log_error {
                        tracing::warn!(%error, "Nats connection error. Retrying...");
                        log_error = false;
                    }
                    backoff_with_options(&mut tries, &options).await;
                }
            }
        }
    }

    /// Creates a new nats message bus connection.
    pub async fn new(server: &str) -> Self {
        let client = Self::connect(server).await;
        Self {
            client: client.clone(),
            jetstream: {
                let mut js = jetstream::new(client);
                js.set_timeout(PUBLISH_TIMEOUT);
                js
            },
        }
    }

    /// Creates consumer and returns an iterator for the messages on the stream.
    async fn create_consumer_and_get_messages(&mut self, stream: Stream) -> BusResult<Messages> {
        tracing::debug!("Getting/creating consumer for stats '{}'", CONSUMER_NAME);
        let options = BackoffOptions::new();
        let mut tries = 0;
        let mut log_error = true;

        let consumer_config = Config {
            durable_name: Some(CONSUMER_NAME.to_string()),
            deliver_policy: DeliverPolicy::All,
            deliver_subject: self.client.new_inbox(),
            max_ack_pending: 1, /* NOTE: when this value is 1, we receive and ack each message
                                 * before moving on to the next ordered message */
            ..Default::default()
        };

        loop {
            let err = match stream
                .get_or_create_consumer(CONSUMER_NAME, consumer_config.clone())
                .await
            {
                Ok(consumer) => match consumer.messages().await {
                    Ok(messages) => {
                        tracing::debug!(
                            "Getting/creating consumer for stats '{}' successful",
                            CONSUMER_NAME
                        );
                        return Ok(messages);
                    }
                    Err(error) => Error::ConsumerError {
                        consumer: CONSUMER_NAME.to_string(),
                        error: error.to_string(),
                    },
                },
                Err(error) => Error::ConsumerError {
                    consumer: CONSUMER_NAME.to_string(),
                    error: error.to_string(),
                },
            };

            if tries == options.max_retries {
                return Err(err);
            }
            if log_error {
                tracing::warn!(%err,
                    "Nats error while getting consumer '{}' messages. Retrying...",
                    CONSUMER_NAME
                );
                log_error = false;
            }
            backoff_with_options(&mut tries, &options).await;
        }
    }

    /// Creates a stream if not exists on message bus. Returns a handle to the stream.
    pub async fn get_or_create_stream(
        &self,
        retry_options: Option<BackoffOptions>,
        msg_replicas: Option<usize>,
    ) -> BusResult<Stream> {
        tracing::debug!("Getting/creating stream '{}'", STREAM_NAME);
        let options = retry_options.unwrap_or_default();
        let mut tries = 0;
        let mut log_error = true;
        let stream_config = async_nats::jetstream::stream::Config {
            name: STREAM_NAME.to_string(),
            subjects: vec![SUBJECTS.into()],
            max_messages_per_subject: MAX_MSGS_PER_SUBJECT, /* When this value is 1 and subject for each message is unique, then msgs are published at most once. */
            max_bytes: STREAM_SIZE,
            storage: async_nats::jetstream::stream::StorageType::Memory, /* The type of storage
                                                                          * backend, `File`
                                                                          * (default) */
            num_replicas: msg_replicas.unwrap_or(NUM_STREAM_REPLICAS),
            ..async_nats::jetstream::stream::Config::default()
        };

        loop {
            let err = match self
                .jetstream
                .get_or_create_stream(stream_config.clone())
                .await
            {
                Ok(stream) => {
                    tracing::debug!("Getting/creating stream '{}' successful", STREAM_NAME);
                    return Ok(stream);
                }
                Err(error) => error,
            };

            if tries == options.max_retries {
                return Err(Error::StreamError {
                    stream: STREAM_NAME.to_string(),
                    source: err,
                });
            }
            if log_error {
                tracing::warn!(%err,
                    "Error while getting/creating stream '{}'. Retrying...",
                    STREAM_NAME
                );
                log_error = false;
            }
            backoff_with_options(&mut tries, &options).await;
        }
    }

    /// Returns subject for the message. Should be unique for each message.
    fn subject(msg: &EventMessage) -> BusResult<String> {
        let event_id = match &msg.metadata {
            Some(event_meta) => &event_meta.id,
            None => {
                return Err(Error::InvalidMessageId {
                    error_msg: "the message id must not be empty".to_string(),
                })
            }
        };
        // If category is volume and id is 'id', then the subject for the message is
        // 'events.volume.id'
        let subject = format!("events.{}.{}", msg.category, event_id);
        Ok(subject)
    }

    /// The payload for mbus publish from the message.
    fn payload(msg: &EventMessage) -> BusResult<bytes::Bytes> {
        Ok(Bytes::from(serde_json::to_vec(msg)?))
    }
}

#[async_trait]
impl Bus for NatsMessageBus {
    /// Publish messages to the message bus atmost once. A jetstream publish call requires just the
    /// subject. NATS will figure out the stream the message should be published to. The message is
    /// discarded if there is any connection error after some retries.
    async fn publish(&mut self, message: &EventMessage) -> BusResult<u64> {
        let options = BackoffOptions::publish_backoff_options();
        let mut tries = 0;
        let mut log_error = true;

        let subject = NatsMessageBus::subject(message)?;
        let payload = NatsMessageBus::payload(message)?;

        loop {
            let publish_request = self.jetstream.publish(subject.clone(), payload.clone());
            let err = match publish_request.await {
                Ok(publish_ack_future) => match publish_ack_future.await {
                    Ok(publish_ack) => {
                        return Ok(publish_ack.sequence);
                    }
                    Err(error) => error,
                },
                Err(error) => error,
            };

            if log_error {
                tracing::warn!(%err,
                    "Error publishing message to jetstream. Retrying..."
                );
                log_error = false;
            }

            if tries == options.max_retries {
                return Err(Error::PublishError {
                    retries: options.max_retries,
                    payload: format!("{message:?}"),
                    source: err,
                });
            }

            match err.kind() {
                PublishErrorKind::TimedOut => {
                    tries += 1;
                    continue;
                }
                PublishErrorKind::StreamNotFound => {
                    let _ = self.get_or_create_stream(None, None).await?;
                    tries += 1;
                    continue;
                }
                _ => (),
            }

            backoff_with_options(&mut tries, &options).await;
        }
    }

    /// Subscribes to the stream on messages though a consumer. Returns an ordered stream of
    /// messages published to the stream.
    async fn subscribe<T: Serialize + DeserializeOwned>(
        &mut self,
    ) -> BusResult<BusSubscription<T>> {
        tracing::trace!("Subscribing to Nats message bus");
        let stream = self.get_or_create_stream(None, None).await?;

        let messages = self.create_consumer_and_get_messages(stream).await?;
        tracing::trace!("Subscribed to Nats message bus successfully");

        return Ok(BusSubscription {
            messages,
            _phantom: Default::default(),
        });
    }
}

/// The messages from bus are deserialised into the type T. For stats module T is EventMessage since
/// all the messages are in the same format.
pub struct BusSubscription<T> {
    messages: Messages, /* Handle to the messages stream for the consumer. We can access the
                         * messages by calling next() method on this. */
    _phantom: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned> BusSubscription<T> {
    /// Access to the next message for the consumer.
    pub async fn next(&mut self) -> Option<T> {
        loop {
            if let Some(message) = self.messages.next().await {
                let message = match message {
                    Ok(message) => message,
                    Err(error) => {
                        match error.kind() {
                            // TODO create consumer again
                            MessagesErrorKind::ConsumerDeleted => (),
                            // TODO check if stream and consumer exists,
                            MessagesErrorKind::MissingHeartbeat => (),
                            _ => tracing::warn!(%error, "Error accessing jetstream message"),
                        };
                        continue;
                    }
                };
                let _ack = message
                    .ack()
                    .await
                    .map_err(|error| tracing::warn!(%error, "Error acking jetstream message"));
                let value: Result<T, _> = serde_json::from_slice(&message.payload);
                match value {
                    Ok(value) => return Some(value),
                    Err(_error) => {
                        tracing::warn!(
                            "Error parsing jetstream message: {:?}; message ignored",
                            message
                        );
                    }
                }
            } else {
                return None;
            }
        }
    }
}
