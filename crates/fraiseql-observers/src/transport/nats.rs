//! NATS `JetStream` transport implementation.
//!
//! This transport uses NATS `JetStream` for distributed event delivery with:
//! - Durable consumers for crash recovery
//! - At-least-once delivery guarantees
//! - Automatic reconnection with exponential backoff
//! - Subject-based routing: `entity.change.{entity_type}.{operation}`

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[cfg(feature = "nats")]
use async_nats::jetstream;
use async_trait::async_trait;
use futures::stream::StreamExt;

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
    transport::{
        EventFilter, EventStream, EventTransport, HealthStatus, TransportHealth, TransportType,
    },
};

#[cfg(feature = "nats")]
use crate::ssrf::validate_nats_url;

/// Configuration for NATS `JetStream` transport.
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL (e.g., "<nats://localhost:4222>")
    pub url: String,

    /// `JetStream` stream name (e.g., `"fraiseql-entity-changes"`)
    ///
    /// NATS stream names must not contain `.` or `_` (NATS rejects them).
    /// Use `-` as a separator.
    pub stream_name: String,

    /// Durable consumer name for this observer instance
    pub consumer_name: String,

    /// Subject prefix for entity changes (e.g., "entity.change")
    pub subject_prefix: String,

    /// Maximum reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: usize,

    /// Base delay for reconnection backoff (milliseconds)
    pub reconnect_delay_ms: u64,

    /// Message acknowledgment timeout (seconds)
    pub ack_wait_secs: u64,

    /// `JetStream` retention policy: max messages
    pub retention_max_messages: i64,

    /// `JetStream` retention policy: max bytes
    pub retention_max_bytes: i64,

    /// Subject for dead-letter queue messages.
    ///
    /// When set, messages that cannot be deserialized are published to this NATS subject
    /// before being ACK-ed. Operators can subscribe to this subject to inspect or replay
    /// malformed payloads. If `None`, undecodable messages are only counted and logged.
    ///
    /// Example: `"entity.change.dead-letter"`.
    pub dead_letter_subject: Option<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url:                    "nats://localhost:4222".to_string(),
            stream_name:            "fraiseql-entity-changes".to_string(),
            consumer_name:          "observer-default".to_string(),
            subject_prefix:         "entity.change".to_string(),
            max_reconnect_attempts: 5,
            reconnect_delay_ms:     1000,
            ack_wait_secs:          30,
            retention_max_messages: 1_000_000,
            retention_max_bytes:    1_073_741_824, // 1 GB
            dead_letter_subject:    None,
        }
    }
}

/// NATS `JetStream` transport implementation.
///
/// # Features
///
/// - **Durable consumers**: Crash recovery with stable consumer names
/// - **At-least-once delivery**: Manual acknowledgment with timeout
/// - **Automatic reconnection**: Exponential backoff on connection failures
/// - **Subject routing**: `entity.change.{entity_type}.{operation}`
///
/// # Example
///
/// ```no_run
/// use fraiseql_observers::transport::{EventFilter, EventTransport, NatsTransport, NatsConfig};
/// use futures::StreamExt;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = NatsConfig {
///         url: "nats://localhost:4222".to_string(),
///         consumer_name: "observer-1".to_string(),
///         ..Default::default()
///     };
///
///     let transport = NatsTransport::new(config).await?;
///     let mut stream = transport.subscribe(EventFilter::default()).await?;
///
///     while let Some(event_result) = stream.next().await {
///         match event_result {
///             Ok(event) => println!("Received event: {:?}", event),
///             Err(e) => eprintln!("Error: {}", e),
///         }
///     }
///
///     Ok(())
/// }
/// ```
#[cfg(feature = "nats")]
pub struct NatsTransport {
    client:    Arc<async_nats::Client>,
    jetstream: Arc<jetstream::Context>,
    config:    NatsConfig,
    /// Count of messages that could not be deserialized.
    ///
    /// Undecodable messages are ACKed (preventing infinite redelivery) and
    /// counted here so operators can monitor message format drift or schema
    /// version mismatches without a dead-letter queue infrastructure.
    pub undecodable_count: Arc<AtomicU64>,
}

#[cfg(feature = "nats")]
impl NatsTransport {
    /// Create a new NATS transport with the given configuration.
    ///
    /// This will:
    /// 1. Connect to the NATS server
    /// 2. Create a `JetStream` context
    /// 3. Ensure the stream exists (create if necessary)
    ///
    /// # Errors
    ///
    /// Returns `TransportConnectionError` if connection fails.
    pub async fn new(config: NatsConfig) -> Result<Self> {
        // Reject private/loopback NATS URLs before attempting a network connection.
        validate_nats_url(&config.url)?;

        // Connect to NATS server
        let client = async_nats::connect(&config.url).await.map_err(|e| {
            ObserverError::TransportConnectionFailed {
                reason: format!("Failed to connect to NATS server: {e}"),
            }
        })?;

        // Create JetStream context
        let jetstream = jetstream::new(client.clone());

        // Ensure stream exists
        Self::ensure_stream(&jetstream, &config).await?;

        Ok(Self {
            client:           Arc::new(client),
            jetstream:        Arc::new(jetstream),
            config,
            undecodable_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Ensure the `JetStream` stream exists, create if necessary.
    ///
    /// # Stream Configuration
    ///
    /// - **Subjects**: `{subject_prefix}.>`
    /// - **Retention**: Limits-based (max messages or max bytes)
    /// - **Discard policy**: Old messages when limits reached
    async fn ensure_stream(jetstream: &jetstream::Context, config: &NatsConfig) -> Result<()> {
        let subjects = vec![format!("{}.>", config.subject_prefix)];

        // Try to get existing stream, create if it doesn't exist
        if jetstream.get_stream(&config.stream_name).await.is_err() {
            // Stream doesn't exist, create it
            jetstream
                .create_stream(jetstream::stream::Config {
                    name: config.stream_name.clone(),
                    subjects,
                    max_messages: config.retention_max_messages,
                    max_bytes: config.retention_max_bytes,
                    ..Default::default()
                })
                .await
                .map_err(|e| ObserverError::TransportConnectionFailed {
                    reason: format!("Failed to create JetStream stream: {e}"),
                })?;
        }

        Ok(())
    }

    /// Parse a NATS message into an `EntityEvent`.
    ///
    /// # Message Format
    ///
    /// Messages are expected to be JSON-encoded `EntityEvent` objects.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::DeserializationError` with the raw bytes preserved so
    /// the caller can route the unparseable payload to a dead-letter queue.
    fn parse_message(msg: &jetstream::Message) -> Result<EntityEvent> {
        serde_json::from_slice(&msg.payload).map_err(|e| ObserverError::DeserializationError {
            raw:    msg.payload.to_vec(),
            reason: format!("Failed to deserialize EntityEvent from NATS message: {e}"),
        })
    }

    /// Return the number of messages that could not be deserialized since startup.
    ///
    /// Useful for monitoring message format drift or schema version mismatches.
    #[must_use]
    pub fn undecodable_count(&self) -> u64 {
        self.undecodable_count.load(Ordering::Relaxed)
    }

    /// Build subject filter from `EventFilter`
    fn build_subject_filter(&self, filter: &EventFilter) -> String {
        if let Some(ref entity_type) = filter.entity_type {
            // Filter by entity type: entity.change.{entity_type}.>
            format!("{}.{}.>", self.config.subject_prefix, entity_type)
        } else {
            // All entity types: entity.change.>
            format!("{}.>", self.config.subject_prefix)
        }
    }
}

#[cfg(feature = "nats")]
// Reason: EventTransport is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl EventTransport for NatsTransport {
    async fn subscribe(&self, filter: EventFilter) -> Result<EventStream> {
        let subject_filter = self.build_subject_filter(&filter);
        let jetstream = Arc::clone(&self.jetstream);
        let config = self.config.clone();

        // Create or get durable consumer
        let consumer = jetstream
            .create_consumer_on_stream(
                jetstream::consumer::pull::Config {
                    durable_name: Some(config.consumer_name.clone()),
                    filter_subject: subject_filter.clone(),
                    deliver_policy: jetstream::consumer::DeliverPolicy::All,
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(config.ack_wait_secs),
                    ..Default::default()
                },
                &config.stream_name,
            )
            .await
            .map_err(|e| ObserverError::TransportSubscribeFailed {
                reason: format!("Failed to create consumer: {e}"),
            })?;

        // Get message stream from consumer
        let messages: jetstream::consumer::pull::Stream =
            consumer.messages().await.map_err(|e| ObserverError::TransportSubscribeFailed {
                reason: format!("Failed to get message stream: {e}"),
            })?;

        // Clone filter fields for use in async closure (wrapped in Arc for sharing)
        let filter_operation = Arc::new(filter.operation.clone());
        let filter_tenant_id = Arc::new(filter.tenant_id.clone());
        let undecodable_count = Arc::clone(&self.undecodable_count);
        let dead_letter_subject = Arc::new(self.config.dead_letter_subject.clone());
        let dlq_client = Arc::clone(&self.client);

        // Convert JetStream messages to Result<EntityEvent>
        let event_stream = messages.filter_map(move |msg_result| {
            let filter_op = Arc::clone(&filter_operation);
            let filter_tenant = Arc::clone(&filter_tenant_id);
            let dlq_subject = Arc::clone(&dead_letter_subject);
            let dlq_client = Arc::clone(&dlq_client);
            let undecodable_count = Arc::clone(&undecodable_count);

            async move {
                match msg_result {
                    Ok(msg) => {
                        // Parse message into EntityEvent
                        match Self::parse_message(&msg) {
                            Ok(event) => {
                                // Apply additional filters (operation, tenant_id)
                                if let Some(ref op) = filter_op.as_ref() {
                                    let event_op = match event.event_type {
                                        crate::event::EventKind::Created => "INSERT",
                                        crate::event::EventKind::Updated => "UPDATE",
                                        crate::event::EventKind::Deleted => "DELETE",
                                        crate::event::EventKind::Custom => "CUSTOM",
                                    };
                                    if event_op != op {
                                        // Skip if operation doesn't match
                                        if let Err(e) = msg.ack().await {
                                            tracing::error!(
                                                "Failed to ack filtered message: {}",
                                                e
                                            );
                                        }
                                        return None;
                                    }
                                }

                                // Filter by tenant_id if specified
                                if let Some(ref tenant) = filter_tenant.as_ref() {
                                    if event.tenant_id.as_ref() != Some(tenant) {
                                        tracing::trace!(
                                            event_id = %event.id,
                                            event_tenant = ?event.tenant_id,
                                            filter_tenant = %tenant,
                                            "Skipping event due to tenant_id mismatch"
                                        );
                                        if let Err(e) = msg.ack().await {
                                            tracing::error!(
                                                "Failed to acknowledge NATS message: {}",
                                                e
                                            );
                                        }
                                        return None;
                                    }
                                }

                                // Acknowledge message after successful parsing
                                if let Err(e) = msg.ack().await {
                                    tracing::error!("Failed to acknowledge NATS message: {}", e);
                                }

                                Some(Ok(event))
                            },
                            Err(e) => {
                                // Increment counter so operators can monitor decode failures.
                                let total = undecodable_count.fetch_add(1, Ordering::Relaxed) + 1;
                                tracing::error!(
                                    undecodable_total = total,
                                    "NATS message failed to decode — ACKing to prevent \
                                     redelivery; check undecodable_count metric for drift"
                                );

                                // Publish raw payload to dead-letter queue if configured.
                                if let Some(ref subject) = dlq_subject.as_ref() {
                                    if let Err(dlq_err) = dlq_client
                                        .publish(subject.clone(), msg.payload.clone())
                                        .await
                                    {
                                        tracing::error!(
                                            error = %dlq_err,
                                            dead_letter_subject = %subject,
                                            "Failed to publish undecodable message to dead-letter queue"
                                        );
                                    } else {
                                        tracing::warn!(
                                            dead_letter_subject = %subject,
                                            undecodable_total = total,
                                            "Undecodable NATS message forwarded to dead-letter queue"
                                        );
                                    }
                                }

                                // ACK to prevent infinite redelivery loop.
                                if let Err(ack_err) = msg.ack().await {
                                    tracing::error!(
                                        "Failed to acknowledge undecodable message: {}",
                                        ack_err
                                    );
                                }
                                Some(Err(e))
                            },
                        }
                    },
                    Err(e) => {
                        tracing::error!("Error receiving NATS message: {}", e);
                        Some(Err(ObserverError::TransportSubscribeFailed {
                            reason: format!("Failed to receive message: {e}"),
                        }))
                    },
                }
            }
        });

        Ok(Box::pin(event_stream))
    }

    async fn publish(&self, event: EntityEvent) -> Result<()> {
        // Build subject: entity.change.{entity_type}.{operation}
        let operation = match event.event_type {
            crate::event::EventKind::Created => "INSERT",
            crate::event::EventKind::Updated => "UPDATE",
            crate::event::EventKind::Deleted => "DELETE",
            crate::event::EventKind::Custom => "CUSTOM",
        };

        let subject = format!("{}.{}.{}", self.config.subject_prefix, event.entity_type, operation);

        // Serialize event to JSON
        let payload =
            serde_json::to_vec(&event).map_err(|e| ObserverError::TransportPublishFailed {
                reason: format!("Failed to serialize event: {e}"),
            })?;

        // Publish to NATS JetStream
        self.jetstream
            .publish(subject, payload.into())
            .await
            .map_err(|e| ObserverError::TransportPublishFailed {
                reason: format!("Failed to publish event: {e}"),
            })?
            .await
            .map_err(|e| ObserverError::TransportPublishFailed {
                reason: format!("Failed to confirm event publication: {e}"),
            })?;

        Ok(())
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Nats
    }

    async fn health_check(&self) -> Result<TransportHealth> {
        // Check NATS connection status
        match self.client.connection_state() {
            async_nats::connection::State::Connected => Ok(TransportHealth {
                status:  HealthStatus::Healthy,
                message: None,
            }),
            async_nats::connection::State::Disconnected => Ok(TransportHealth {
                status:  HealthStatus::Unhealthy,
                message: Some("NATS client disconnected".to_string()),
            }),
            _ => Ok(TransportHealth {
                status:  HealthStatus::Degraded,
                message: Some("NATS client in degraded state".to_string()),
            }),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "nats")]
mod tests {
    use super::*;
    use crate::ssrf::validate_nats_url;

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert_eq!(config.url, "nats://localhost:4222");
        assert_eq!(config.stream_name, "fraiseql-entity-changes");
        assert_eq!(config.consumer_name, "observer-default");
        assert_eq!(config.subject_prefix, "entity.change");
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.reconnect_delay_ms, 1000);
        assert_eq!(config.ack_wait_secs, 30);
        assert_eq!(config.retention_max_messages, 1_000_000);
        assert_eq!(config.retention_max_bytes, 1_073_741_824);
    }

    // Note: Integration tests with an embedded NATS server live in the tests/ directory.
    // Unit tests for NatsTransport require a running NATS server and are therefore
    // deferred to integration tests.

    #[test]
    fn validate_nats_url_rejects_loopback() {
        let result = validate_nats_url("nats://127.0.0.1:4222");
        assert!(result.is_err(), "loopback NATS URL must be rejected");
    }

    #[test]
    fn validate_nats_url_rejects_private_ip() {
        let result = validate_nats_url("nats://10.0.0.1:4222");
        assert!(result.is_err(), "private-IP NATS URL must be rejected");
    }

    #[test]
    fn validate_nats_url_rejects_wrong_scheme() {
        let result = validate_nats_url("http://nats.example.com:4222");
        assert!(result.is_err(), "non-nats:// scheme must be rejected");
    }
}
