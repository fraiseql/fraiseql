use async_trait::async_trait;
use serde::Serialize;

use super::{SubscriptionError, transport::TransportAdapter, types::SubscriptionEvent};

/// Kafka transport adapter configuration.
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Kafka broker addresses (comma-separated).
    pub brokers: String,

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
    #[must_use]
    pub fn new(brokers: impl Into<String>, default_topic: impl Into<String>) -> Self {
        Self {
            brokers:       brokers.into(),
            default_topic: default_topic.into(),
            client_id:     "fraiseql".to_string(),
            acks:          "all".to_string(),
            timeout_ms:    30_000,
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
/// - **With `kafka` feature**: Full rdkafka-based producer with actual Kafka delivery
/// - **Without `kafka` feature**: Stub that logs events (for development/testing)
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{KafkaAdapter, KafkaConfig};
///
/// let config = KafkaConfig::new("localhost:9092", "fraiseql-events")
///     .with_client_id("my-service")
///     .with_compression("lz4");
///
/// let adapter = KafkaAdapter::new(config)?;
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
#[cfg(feature = "kafka")]
pub struct KafkaAdapter {
    config:   KafkaConfig,
    producer: rdkafka::producer::FutureProducer,
}

#[cfg(feature = "kafka")]
impl std::fmt::Debug for KafkaAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaAdapter")
            .field("brokers", &self.config.brokers)
            .field("default_topic", &self.config.default_topic)
            .field("client_id", &self.config.client_id)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "kafka")]
impl KafkaAdapter {
    /// Create a new Kafka adapter with a producer connection.
    ///
    /// # Errors
    ///
    /// Returns error if the Kafka producer cannot be created (e.g., invalid config).
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        use rdkafka::{config::ClientConfig, producer::FutureProducer};

        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("client.id", &config.client_id)
            .set("acks", &config.acks)
            .set("message.timeout.ms", config.timeout_ms.to_string());

        if let Some(ref compression) = config.compression {
            client_config.set("compression.type", compression);
        }

        let producer: FutureProducer = client_config.create().map_err(|e| {
            SubscriptionError::Internal(format!("Failed to create Kafka producer: {e}"))
        })?;

        tracing::info!(
            brokers = %config.brokers,
            topic = %config.default_topic,
            client_id = %config.client_id,
            "KafkaAdapter created with rdkafka producer"
        );

        Ok(Self { config, producer })
    }

    /// Get the topic for a subscription (uses default if not specified).
    fn get_topic(&self, _subscription_name: &str) -> &str {
        // Could be extended to support per-subscription topic mapping
        &self.config.default_topic
    }

    /// Get reference to the underlying producer for direct Kafka operations.
    #[must_use = "the producer reference should be used for Kafka operations"]
    pub const fn producer(&self) -> &rdkafka::producer::FutureProducer {
        &self.producer
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
        use std::time::Duration;

        use rdkafka::producer::FutureRecord;

        let message = KafkaMessage::from_event(event, subscription_name);
        let topic = self.get_topic(subscription_name);

        let payload = serde_json::to_string(&message).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize message: {e}"))
        })?;

        let record = FutureRecord::to(topic).key(message.key()).payload(&payload);

        let timeout = Duration::from_millis(self.config.timeout_ms);

        match self.producer.send(record, timeout).await {
            Ok(delivery) => {
                tracing::debug!(
                    topic = topic,
                    partition = delivery.partition,
                    offset = delivery.offset,
                    key = message.key(),
                    event_id = %event.event_id,
                    "Kafka message delivered successfully"
                );
                Ok(())
            },
            Err((kafka_error, _)) => {
                tracing::error!(
                    topic = topic,
                    key = message.key(),
                    event_id = %event.event_id,
                    error = %kafka_error,
                    "Failed to deliver Kafka message"
                );
                Err(SubscriptionError::DeliveryFailed {
                    transport: "kafka".to_string(),
                    reason:    kafka_error.to_string(),
                })
            },
        }
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // Check if we can fetch cluster metadata as a health check
        use std::time::Duration;

        use rdkafka::producer::Producer;

        match self.producer.client().fetch_metadata(
            None, // All topics
            Duration::from_secs(5),
        ) {
            Ok(metadata) => {
                tracing::debug!(
                    broker_count = metadata.brokers().len(),
                    topic_count = metadata.topics().len(),
                    "Kafka health check passed"
                );
                true
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Kafka health check failed"
                );
                false
            },
        }
    }
}

// =============================================================================
// Kafka Adapter - Stub Implementation (without `kafka` feature)
// =============================================================================

/// Kafka transport adapter stub (without `kafka` feature).
///
/// This is a stub implementation for development and testing.
/// Enable the `kafka` feature for actual Kafka delivery.
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
    /// This is a stub implementation. Enable the `kafka` feature for actual delivery.
    ///
    /// # Errors
    ///
    /// This stub implementation never fails, but returns `Result` for API compatibility.
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        tracing::warn!(
            brokers = %config.brokers,
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
        let message = KafkaMessage::from_event(event, subscription_name);
        let topic = self.get_topic(subscription_name);

        let _payload = serde_json::to_string(&message).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize message: {e}"))
        })?;

        // Stub implementation - log the event
        tracing::info!(
            topic = topic,
            key = message.key(),
            event_id = %event.event_id,
            "Kafka delivery (STUB) - enable 'kafka' feature for actual delivery"
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // Stub always returns true
        tracing::debug!("Kafka health check (STUB) - always returns true");
        true
    }
}
