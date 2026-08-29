//! The Apache Kafka outbound sink (feature `cdc-kafka`).
//!
//! Publishes each change event to a rendered topic with an idempotent producer.
//! The endpoint parsing, transport guard and topic-charset validation this sink
//! depends on live in [`crate::sink`] and are always compiled — see the note
//! there. This module holds only what genuinely needs rdkafka.

use std::time::Duration;

use fraiseql_guard::kafka::{KafkaSecurityProtocol, guard_kafka_endpoint, resolve_kafka_sasl};
use rdkafka::{
    ClientConfig,
    error::KafkaError,
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    types::RDKafkaErrorCode,
};

use crate::{
    error::{CdcError, Result},
    event::ChangeEvent,
    sink::{
        CdcSink, CdcSinkConfig, PublishOutcome, SinkKind, entity_partition_key, render_kafka_topic,
    },
};

/// How long `send` may wait for space in the local producer queue.
///
/// Bounded so a full queue surfaces as a retryable failure the drain worker can
/// back off on, rather than parking the drain tick indefinitely.
const ENQUEUE_TIMEOUT: Duration = Duration::from_secs(5);

/// Bound on librdkafka's own end-to-end delivery attempt (`message.timeout.ms`).
///
/// Left well under the drain's retry cadence so a broker outage produces a
/// classified `Transient` outcome per attempt instead of an unbounded stall.
const MESSAGE_TIMEOUT_MS: &str = "30000";

/// A sink that publishes change events to Apache Kafka.
pub struct KafkaSink {
    config:   CdcSinkConfig,
    producer: FutureProducer,
}

impl KafkaSink {
    /// Build an idempotent Kafka producer for this sink.
    ///
    /// `endpoint` must carry an explicit scheme — `kafka+ssl://`,
    /// `kafka+sasl-ssl://`, or plaintext `kafka://` under the development opt-in.
    /// It is screened by [`guard_kafka_endpoint`] before any client is
    /// constructed; the payload here is the full row after-image of every
    /// mutation, so a scheme-less endpoint is refused rather than silently taken
    /// as `PLAINTEXT` the way librdkafka would take it.
    ///
    /// `enable.idempotence` is on: librdkafka then holds `acks=all`, bounded
    /// in-flight requests and producer-side retries, which together give
    /// no-duplicates-from-retry *and* per-partition ordering. Since the message
    /// key pins each entity to one partition, that is per-entity ordering.
    ///
    /// SASL credentials and any custom CA are supplied through the standard
    /// librdkafka environment (`extra` config is not yet exposed); the scheme
    /// only decides `security.protocol`.
    ///
    /// # Errors
    ///
    /// Returns [`CdcError::Config`] for an unsafe or malformed endpoint, or
    /// [`CdcError::Connection`] if the producer cannot be created.
    pub fn connect(endpoint: &str, config: CdcSinkConfig) -> Result<Self> {
        let endpoint = guard_kafka_endpoint(endpoint).map_err(CdcError::Config)?;

        let mut client = ClientConfig::new();
        client
            .set("bootstrap.servers", &endpoint.bootstrap_servers)
            .set("security.protocol", endpoint.security_protocol.as_str())
            .set("enable.idempotence", "true")
            .set("message.timeout.ms", MESSAGE_TIMEOUT_MS)
            .set("compression.type", "lz4");

        if endpoint.security_protocol == KafkaSecurityProtocol::SaslSsl {
            let sasl = resolve_kafka_sasl().map_err(CdcError::Config)?;
            client
                .set("sasl.mechanism", sasl.mechanism.as_str())
                .set("sasl.username", &sasl.username)
                .set("sasl.password", &sasl.password);
        }

        let producer: FutureProducer = client
            .create()
            .map_err(|e| CdcError::Connection(format!("kafka producer: {e}")))?;

        Ok(Self { config, producer })
    }
}

impl CdcSink for KafkaSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::Kafka
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        let topic = match render_kafka_topic(&self.config.subject_template, ev) {
            Ok(topic) => topic,
            Err(reason) => return PublishOutcome::Permanent(format!("topic render: {reason}")),
        };
        let payload = match serde_json::to_vec(ev) {
            Ok(payload) => payload,
            Err(error) => return PublishOutcome::Permanent(format!("encode: {error}")),
        };

        let key = entity_partition_key(ev);
        let msg_id = format!("{}:{}", ev.object_type, ev.seq);
        let headers = OwnedHeaders::new()
            .insert(Header {
                key:   "fraiseql-msg-id",
                value: Some(&msg_id),
            })
            .insert(Header {
                key:   "fraiseql-op",
                value: Some(ev.op.as_str()),
            });

        let record = FutureRecord::to(&topic).key(&key).payload(&payload).headers(headers);

        match self.producer.send(record, ENQUEUE_TIMEOUT).await {
            Ok(_) => PublishOutcome::Published,
            Err((error, _)) => classify(&error),
        }
    }
}

/// Classify a produce failure as retryable or dead-letter.
///
/// The default is [`PublishOutcome::Transient`], deliberately: the drain worker
/// dead-letters on its own `max_attempts` ceiling anyway, so a misclassified
/// transient error costs a delay, while a misclassified permanent one discards a
/// change event. Only failures that *cannot* succeed on redelivery of the same
/// bytes are permanent.
fn classify(error: &KafkaError) -> PublishOutcome {
    let permanent = matches!(
        error,
        KafkaError::MessageProduction(
            RDKafkaErrorCode::MessageSizeTooLarge
                | RDKafkaErrorCode::MessageBatchTooLarge
                | RDKafkaErrorCode::InvalidTopic
                | RDKafkaErrorCode::InvalidRecord
        )
    );
    if permanent {
        PublishOutcome::Permanent(format!("publish: {error}"))
    } else {
        PublishOutcome::Transient(format!("publish: {error}"))
    }
}

#[cfg(test)]
mod tests;
