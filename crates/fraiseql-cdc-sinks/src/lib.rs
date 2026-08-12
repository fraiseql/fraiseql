//! Outbound change-data-capture sinks (#382) — drains the FraiseQL change-log
//! outbox to external message brokers.
//!
//! This crate is the *reader/shipper* half of the Change Spine: the mutation
//! executor (and the #366 external-write capture trigger) already wrote durable
//! `core.tb_entity_change_log` outbox rows in-transaction; [`DrainWorker`] reads
//! those rows and publishes them to a broker [`CdcSink`] with **at-least-once**
//! delivery (consumers dedup on `(object_type, seq)`). A broker outage causes
//! outbox/backlog accumulation and retry — never event loss — because the
//! executor's write and the broker publish are decoupled.
//!
//! # Shape contrast
//!
//! This is *not* the fire-and-forget subscription
//! `fraiseql_core::runtime::subscription::TransportAdapter` (no outbox; events
//! lost on failure) nor the inbound observer NATS *consumer*. It is a durable,
//! outbox-backed firehose *producer*.
//!
//! # Layered optionality
//!
//! The drain worker, every endpoint guard and all pure encoding/sanitisation
//! logic compile unconditionally; each broker sink is gated behind its own
//! feature — NATS `JetStream` (`cdc-nats-jetstream`), Apache Kafka
//! (`cdc-kafka`) and AWS Kinesis (`cdc-kinesis`).
//!
//! Guards live alongside the sink trait rather than beside their clients because
//! they are pure: that keeps the *refusing* half of each one in the cheap,
//! broker-free test build instead of only where its feature is enabled.
//!
//! ```
//! use fraiseql_cdc_sinks::{ChangeEvent, ChangeOp, CdcSinkConfig, render_subject};
//!
//! let cfg = CdcSinkConfig::new("primary", "fraiseql.{tenant_id}.{table}");
//! let ev = ChangeEvent::new(7, "tb_post", ChangeOp::Insert);
//! assert_eq!(render_subject(&cfg.subject_template, &ev).unwrap(), "fraiseql._none_.tb_post");
//! ```

#![deny(missing_docs)]

mod drain;
mod error;
mod event;
mod migrations;
mod sink;

#[cfg(feature = "cdc-kafka")]
mod kafka;
#[cfg(feature = "cdc-kinesis")]
mod kinesis;
#[cfg(feature = "cdc-nats-jetstream")]
mod nats;

pub use drain::{DrainStats, DrainWorker};
pub use error::{CdcError, Result};
pub use event::{ChangeEvent, ChangeOp};
#[cfg(feature = "cdc-kafka")]
pub use kafka::KafkaSink;
#[cfg(feature = "cdc-kinesis")]
pub use kinesis::KinesisSink;
pub use migrations::outbox_sink_state_migration_sql;
#[cfg(feature = "cdc-nats-jetstream")]
pub use nats::NatsJetStreamSink;
pub use sink::{
    CdcSink, CdcSinkConfig, KafkaEndpoint, KafkaSaslCredentials, KafkaSaslMechanism,
    KafkaSecurityProtocol, KinesisEndpoint, PublishOutcome, SinkKind, entity_partition_key,
    guard_kafka_endpoint, guard_kinesis_endpoint, next_attempt_delay, render_kafka_topic,
    render_kinesis_stream, render_subject, resolve_kafka_sasl, resolve_kinesis_endpoint_url,
    validate_kafka_topic, validate_kinesis_stream,
};
