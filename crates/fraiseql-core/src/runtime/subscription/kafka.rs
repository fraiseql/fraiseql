use async_trait::async_trait;
use serde::Serialize;

use super::{SubscriptionError, transport::TransportAdapter, types::SubscriptionEvent};

/// Kafka transport adapter configuration.
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// The broker endpoint, **scheme first**: `kafka+ssl://host:port`,
    /// `kafka+sasl-ssl://host:port`, or `kafka://host:port` for a development broker
    /// under `FRAISEQL_KAFKA_ALLOW_PLAINTEXT`.
    ///
    /// A bare `bootstrap.servers` list is refused rather than defaulted (#1102). This
    /// adapter ships entity after-images *and* pre-images, and librdkafka reads a
    /// scheme-less list as `PLAINTEXT` — which is what it did here, with no
    /// `security.protocol` set at all, until the endpoint was screened by
    /// [`fraiseql_guard::kafka`].
    pub endpoint: String,

    /// Default topic for events (can be overridden per subscription).
    pub default_topic: String,

    /// Client ID for Kafka producer.
    pub client_id: String,

    /// Message acknowledgment mode ("all", "1", "0").
    pub acks: String,

    /// Message timeout in milliseconds.
    pub timeout_ms: u64,

    /// Enable message compression.
    pub compression: Option<String>,
}

impl KafkaConfig {
    /// Create a new Kafka configuration.
    ///
    /// `endpoint` must carry a scheme — see [`KafkaConfig::endpoint`]. It is not
    /// validated here: the refusal happens at [`KafkaAdapter::new`], where it can be
    /// reported to whoever is mounting the transport.
    ///
    /// The default `timeout_ms` is 5 s rather than the CDC sink's 30 s. Subscription
    /// delivery is at-most-once with no outbox behind it, so a delivery attempt is a
    /// hot-path task with nothing to retry it — parking one for half a minute over a
    /// message nobody will resend is the wrong trade.
    #[must_use]
    pub fn new(endpoint: impl Into<String>, default_topic: impl Into<String>) -> Self {
        Self {
            endpoint:      endpoint.into(),
            default_topic: default_topic.into(),
            client_id:     "fraiseql".to_string(),
            acks:          "all".to_string(),
            timeout_ms:    5_000,
            compression:   None,
        }
    }

    /// Set the client ID.
    #[must_use]
    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    /// Set acknowledgment mode.
    #[must_use]
    pub fn with_acks(mut self, acks: impl Into<String>) -> Self {
        self.acks = acks.into();
        self
    }

    /// Set message timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Enable compression (e.g., "gzip", "snappy", "lz4").
    #[must_use]
    pub fn with_compression(mut self, compression: impl Into<String>) -> Self {
        self.compression = Some(compression.into());
        self
    }
}

/// Kafka message format for event delivery.
#[derive(Debug, Clone, Serialize)]
pub struct KafkaMessage {
    /// Unique event identifier.
    pub event_id: String,

    /// Subscription name.
    pub subscription_name: String,

    /// Entity type.
    pub entity_type: String,

    /// Entity primary key (used as message key).
    pub entity_id: String,

    /// Operation type.
    pub operation: String,

    /// Event data.
    pub data: serde_json::Value,

    /// Previous data (for UPDATE operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,

    /// Event timestamp.
    pub timestamp: String,

    /// Sequence number.
    pub sequence_number: u64,
}

impl KafkaMessage {
    /// Create a Kafka message from a subscription event.
    #[must_use]
    pub fn from_event(event: &SubscriptionEvent, subscription_name: &str) -> Self {
        Self {
            event_id:          event.event_id.clone(),
            subscription_name: subscription_name.to_string(),
            entity_type:       event.entity_type.clone(),
            entity_id:         event.entity_id.clone(),
            operation:         format!("{:?}", event.operation),
            data:              event.data.clone(),
            old_data:          event.old_data.clone(),
            timestamp:         event.timestamp.to_rfc3339(),
            sequence_number:   event.sequence_number,
        }
    }

    /// Get the message key (`entity_id` for partitioning).
    #[must_use]
    pub fn key(&self) -> &str {
        &self.entity_id
    }
}

// =============================================================================
// Kafka Adapter - Full Implementation (with `kafka` feature)
// =============================================================================

/// Kafka transport adapter for event streaming.
///
/// Delivers subscription events to Apache Kafka topics.
/// Uses the `entity_id` as the message key for consistent partitioning.
///
/// # Feature Flag
///
/// This adapter has two implementations:
/// - **With `kafka` feature**: delivery through [`fraiseql_kafka::KafkaEgress`], the one producer
///   this workspace builds
/// - **Without `kafka` feature**: a stub that fails loud on every delivery (#784)
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{KafkaAdapter, KafkaConfig};
///
/// let config = KafkaConfig::new("kafka+ssl://localhost:9093", "fraiseql-events")
///     .with_client_id("my-service")
///     .with_compression("lz4");
///
/// let adapter = KafkaAdapter::new(config)?;
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
#[cfg(feature = "kafka")]
pub struct KafkaAdapter {
    config: KafkaConfig,
    egress: fraiseql_kafka::KafkaEgress,
}

#[cfg(feature = "kafka")]
impl std::fmt::Debug for KafkaAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaAdapter")
            .field("endpoint", &self.config.endpoint)
            .field("default_topic", &self.config.default_topic)
            .field("client_id", &self.config.client_id)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "kafka")]
impl KafkaAdapter {
    /// Connect this adapter's shared Kafka egress.
    ///
    /// The endpoint is screened and `security.protocol` set from its scheme before any
    /// client exists — by [`fraiseql_kafka::KafkaEgress`], the one place in this
    /// workspace that builds a producer, so this transport and the CDC outbox sink
    /// cannot disagree about what is safe (#1102).
    ///
    /// # Errors
    ///
    /// [`SubscriptionError::Internal`] if the endpoint is refused (no scheme,
    /// unsupported scheme, unopted-in plaintext, blocked broker host, unresolvable SASL
    /// credentials) or if the producer cannot be created.
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        use std::time::Duration;

        // At-most-once, so both bounds are short: there is no outbox behind this and
        // nothing will retry, which makes a long timeout a parked task rather than a
        // second chance.
        let egress_config = fraiseql_kafka::KafkaEgressConfig::new(config.client_id.clone())
            .with_timeouts(
                Duration::from_millis(config.timeout_ms),
                Duration::from_millis(config.timeout_ms.min(1_000)),
            )
            .with_compression(config.compression.clone());

        let egress = fraiseql_kafka::KafkaEgress::connect(&config.endpoint, &egress_config)
            .map_err(|error| {
                SubscriptionError::Internal(format!("Failed to create Kafka producer: {error}"))
            })?;

        tracing::info!(
            endpoint = %config.endpoint,
            topic = %config.default_topic,
            client_id = %config.client_id,
            "KafkaAdapter connected through the shared Kafka egress"
        );

        Ok(Self { config, egress })
    }

    /// Get the topic for a subscription (uses default if not specified).
    fn get_topic(&self, _subscription_name: &str) -> &str {
        // Could be extended to support per-subscription topic mapping
        &self.config.default_topic
    }
}

#[cfg(feature = "kafka")]
// Reason: TransportAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl TransportAdapter for KafkaAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        use fraiseql_kafka::{EgressOutcome, EgressRecord};

        let message = KafkaMessage::from_event(event, subscription_name);
        let topic = self.get_topic(subscription_name);

        let payload = serde_json::to_string(&message).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize message: {e}"))
        })?;

        let outcome = self
            .egress
            .send(EgressRecord {
                topic,
                key: message.key(),
                payload: payload.as_bytes(),
                headers: &[("fraiseql-event-id", &message.event_id)],
            })
            .await;

        // At-most-once. Both failures are reported, neither is retried here: there is no
        // outbox behind this transport, so a retry loop would be a second durability
        // story to keep in step with the CDC sink's. The distinction survives in the
        // message because it is what an operator needs — "the broker was down" and "this
        // message can never be accepted" call for different responses.
        match outcome {
            EgressOutcome::Delivered => {
                tracing::debug!(
                    topic = topic,
                    key = message.key(),
                    event_id = %event.event_id,
                    "Kafka message delivered successfully"
                );
                Ok(())
            },
            EgressOutcome::Transient(reason) => {
                tracing::error!(
                    topic = topic,
                    key = message.key(),
                    event_id = %event.event_id,
                    %reason,
                    "Kafka delivery failed; at-most-once, so this event is not retried"
                );
                Err(SubscriptionError::DeliveryFailed {
                    transport: "kafka".to_string(),
                    reason,
                })
            },
            EgressOutcome::Permanent(reason) => {
                tracing::error!(
                    topic = topic,
                    key = message.key(),
                    event_id = %event.event_id,
                    %reason,
                    "Kafka refused this message permanently; redelivery could not succeed"
                );
                Err(SubscriptionError::DeliveryFailed {
                    transport: "kafka".to_string(),
                    reason,
                })
            },
        }
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // The egress owns the producer, so it owns the metadata call too — a transport
        // reaching for the raw client is the first step back towards a second producer.
        self.egress.is_reachable(std::time::Duration::from_secs(5))
    }
}

// =============================================================================
// Kafka Adapter - Stub Implementation (without `kafka` feature)
// =============================================================================

/// Kafka transport adapter stub (without `kafka` feature).
///
/// The stub fails loud: every `deliver` returns an error and `health_check`
/// reports unhealthy, so a deployment configured for Kafka on a binary built
/// without it cannot silently drop events. Enable the `kafka` feature for
/// actual Kafka delivery.
#[cfg(not(feature = "kafka"))]
#[derive(Debug)]
pub struct KafkaAdapter {
    config: KafkaConfig,
}

#[cfg(not(feature = "kafka"))]
impl KafkaAdapter {
    /// Create a new Kafka adapter stub.
    ///
    /// # Note
    ///
    /// This is a stub implementation: it can be constructed (so configuration
    /// plumbing keeps working), but every `deliver` fails and `health_check`
    /// reports unhealthy. Enable the `kafka` feature for actual delivery.
    ///
    /// # Errors
    ///
    /// Construction never fails; the `Result` mirrors the real adapter's API.
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        tracing::warn!(
            endpoint = %config.endpoint,
            topic = %config.default_topic,
            "KafkaAdapter created (STUB - enable 'kafka' feature for real Kafka support)"
        );
        Ok(Self { config })
    }

    /// Get the topic for a subscription (uses default if not specified).
    fn get_topic(&self, _subscription_name: &str) -> &str {
        &self.config.default_topic
    }
}

#[cfg(not(feature = "kafka"))]
// Reason: TransportAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl TransportAdapter for KafkaAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        // Fail loud (#784): reporting Ok here would drop the event while
        // signalling successful delivery — the fail-open shape the other
        // compiled-out runtime stubs (functions WASM/Deno, observers cache,
        // NATS) deliberately avoid.
        let topic = self.get_topic(subscription_name);
        Err(SubscriptionError::Internal(format!(
            "Kafka transport is not compiled into this binary (event {event_id} for topic \
             '{topic}' not delivered). Rebuild with `--features kafka`.",
            event_id = event.event_id,
        )))
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // The stub can deliver nothing, so it is never healthy.
        tracing::debug!("Kafka health check (STUB): kafka feature not compiled in — unhealthy");
        false
    }
}
