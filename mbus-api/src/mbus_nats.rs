use crate::{
    common::{
        constants::*,
        errors::Error,
        retry::{backoff_with_options, BackoffOptions},
    },
    message::EventMessage,
    Bus,
};
use async_nats::{
    jetstream::{
        self,
        consumer::{
            push::{Config, Messages},
            DeliverPolicy,
        },
        stream::Stream,
        Context,
    },
    Client,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use serde::{de::DeserializeOwned, Serialize};
use std::{io::ErrorKind, marker::PhantomData, time::Duration};

/// Result wrapper for Jetstream requests.
pub type BusResult<T> = Result<T, Error>;

/// Initialise the Nats Message Bus.
pub async fn message_bus_init(server: &str) -> impl crate::Bus {
    NatsMessageBus::new(server).await
}

/// Nats implementation of the Bus.
#[derive(Clone)]
pub(crate) struct NatsMessageBus {
    client: Client,
    jetstream: Context,
}

impl NatsMessageBus {
    /// Connect to nats server.
    pub async fn connect(server: &str) -> Client {
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

    /// Ensures that the stream is created on jetstream.
    pub async fn verify_stream_exists(&mut self) -> BusResult<()> {
        let options = BackoffOptions::new().with_max_retries(0);
        if let Err(err) = self.get_or_create_stream(Some(options)).await {
            tracing::warn!(%err,
                "Error while getting/creating stream '{}'",
                STREAM_NAME
            );
        }
        Ok(())
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
                    Err(error) => error,
                },
                Err(error) => error,
            };

            if tries == options.max_retries {
                return Err(Error::ConsumerError {
                    consumer: CONSUMER_NAME.to_string(),
                    error: err.to_string(),
                });
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
    ) -> BusResult<Stream> {
        tracing::debug!("Getting/creating stream '{}'", STREAM_NAME);
        let options = retry_options.unwrap_or(BackoffOptions::new());
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
            num_replicas: NUM_STREAM_REPLICAS,
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
                    error: err.to_string(),
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
    fn subject(msg: &EventMessage) -> String {
        format!("events.{}.{}", msg.category.to_string(), msg.metadata.id) // If category is volume
                                                                           // and id is
                                                                           // 'id', then the subject
                                                                           // for the
                                                                           // message is
                                                                           // 'events.volume.id'
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

        let subject = NatsMessageBus::subject(message);
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
                let _stream = self.verify_stream_exists().await; // Check and create a stream if necessary. Useful when the stream is deleted.
                log_error = false;
            }

            if tries == options.max_retries {
                return Err(Error::PublishError {
                    retries: options.max_retries,
                    payload: format!("{message:?}"),
                    error: err.to_string(),
                });
            }
            if let Some(error) = err.downcast_ref::<std::io::Error>() {
                if error.kind() == ErrorKind::TimedOut {
                    tries += 1;
                    continue;
                }
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
        let stream = self.get_or_create_stream(None).await?;

        let messages = self.create_consumer_and_get_messages(stream).await?;
        tracing::trace!("Subscribed to Nats message bus successfully");

        return Ok(BusSubscription {
            messages,
            _phantom: PhantomData::default(),
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
                        tracing::warn!(%error, "Error accessing jetstream message");
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
