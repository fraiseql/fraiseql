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
//! The drain worker and all pure encoding/sanitisation logic compile
//! unconditionally; each broker sink is gated behind its own feature. The first
//! shipped sink is NATS `JetStream` (`cdc-nats-jetstream`).
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

#[cfg(feature = "cdc-nats-jetstream")]
mod nats;

pub use drain::{DrainStats, DrainWorker};
pub use error::{CdcError, Result};
pub use event::{ChangeEvent, ChangeOp};
pub use migrations::outbox_sink_state_migration_sql;
#[cfg(feature = "cdc-nats-jetstream")]
pub use nats::NatsJetStreamSink;
pub use sink::{
    CdcSink, CdcSinkConfig, PublishOutcome, SinkKind, next_attempt_delay, render_subject,
};
