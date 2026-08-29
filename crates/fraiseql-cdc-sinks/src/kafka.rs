//! The Apache Kafka outbound sink (feature `cdc-kafka`).
//!
//! Publishes each change event to a rendered topic. What reaches the wire — the endpoint
//! guard, `security.protocol`, SASL and the produce call — is
//! [`fraiseql_kafka::KafkaEgress`], shared with the subscription transport so there is
//! one answer to "may this endpoint be plaintext?" rather than one per caller (#1102).
//!
//! What stays here is this sink's own: which topic, which partition key, what the bytes
//! are, and what a transient failure *means* — the drain worker's retry ceiling and
//! dead-letter state sit above `publish`, not inside it.

use fraiseql_kafka::{EgressError, EgressOutcome, EgressRecord, KafkaEgress, KafkaEgressConfig};

use crate::{
    error::{CdcError, Result},
    event::ChangeEvent,
    sink::{
        CdcSink, CdcSinkConfig, PublishOutcome, SinkKind, entity_partition_key, render_kafka_topic,
    },
};

/// A sink that publishes change events to Apache Kafka.
pub struct KafkaSink {
    config: CdcSinkConfig,
    egress: KafkaEgress,
}

impl KafkaSink {
    /// Build an idempotent Kafka producer for this sink.
    ///
    /// `endpoint` must carry an explicit scheme — `kafka+ssl://`, `kafka+sasl-ssl://`,
    /// or plaintext `kafka://` under the development opt-in. It is screened before any
    /// client is constructed; the payload here is the full row after-image of every
    /// mutation, so a scheme-less endpoint is refused rather than silently taken as
    /// `PLAINTEXT` the way librdkafka would take it.
    ///
    /// The egress defaults are this caller's: `enable.idempotence` on (no duplicates
    /// from producer retries, and per-partition — therefore per-entity — ordering), and
    /// a 30s delivery timeout kept well under the drain's retry cadence so a broker
    /// outage yields a classified transient outcome per attempt instead of a stall.
    ///
    /// # Errors
    ///
    /// [`CdcError::Config`] for an unsafe or malformed endpoint, or unresolvable SASL
    /// credentials; [`CdcError::Connection`] if the producer cannot be created.
    pub fn connect(endpoint: &str, config: CdcSinkConfig) -> Result<Self> {
        let egress = KafkaEgress::connect(
            endpoint,
            &KafkaEgressConfig::new(format!("fraiseql-cdc-{}", config.name)),
        )
        .map_err(|error| match error {
            EgressError::Config(message) => CdcError::Config(message),
            EgressError::Connection(message) => CdcError::Connection(message),
        })?;

        Ok(Self { config, egress })
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

        let outcome = self
            .egress
            .send(EgressRecord {
                topic:   &topic,
                key:     &key,
                payload: &payload,
                headers: &[
                    ("fraiseql-msg-id", &msg_id),
                    ("fraiseql-op", ev.op.as_str()),
                ],
            })
            .await;

        // The two classifications are the same distinction — "could the same bytes
        // succeed later?" — so this maps rather than re-decides. A `_` arm here is what
        // would let a new egress outcome be silently folded into the wrong one, which is
        // why `EgressOutcome` is not `#[non_exhaustive]`.
        match outcome {
            EgressOutcome::Delivered => PublishOutcome::Published,
            EgressOutcome::Transient(message) => PublishOutcome::Transient(message),
            EgressOutcome::Permanent(message) => PublishOutcome::Permanent(message),
        }
    }
}

#[cfg(test)]
mod tests;
