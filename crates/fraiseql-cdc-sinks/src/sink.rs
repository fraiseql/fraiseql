//! The broker-agnostic sink trait, routing config, and pure encoding helpers.
//!
//! Everything here is **always compiled** (no broker feature), so the subject
//! sanitiser, the per-tenant/per-table filter, and the backoff schedule are
//! exercised by the fast unit-test leg, not only behind a broker feature.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::ChangeEvent;

/// Which broker a sink targets. Serialised `kebab-case` so TOML `kind =
/// "nats-jetstream"` round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SinkKind {
    /// Apache Kafka (not yet implemented — see the #382 umbrella).
    Kafka,
    /// NATS `JetStream`. Renamed explicitly so the TOML `kind` is the compact
    /// `"nats-jetstream"` rather than kebab-case's `"nats-jet-stream"`.
    #[serde(rename = "nats-jetstream")]
    NatsJetStream,
    /// AWS Kinesis (not yet implemented).
    Kinesis,
    /// Apache Pulsar (not yet implemented).
    Pulsar,
}

/// The outcome of publishing one event to a broker.
///
/// Mirrors the observer transient-vs-permanent classification
/// (`fraiseql-observers/src/actions.rs`): a transient failure is retried with
/// backoff; a permanent failure goes straight to the dead-letter state (`dead`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PublishOutcome {
    /// The broker acknowledged the record.
    Published,
    /// A retryable failure (broker down, ack timeout) — retry with backoff.
    Transient(String),
    /// A non-retryable failure (un-renderable subject, encode error) — dead-letter.
    Permanent(String),
}

/// Default delivery attempt ceiling before a tracking row is dead-lettered.
const fn default_max_attempts() -> i32 {
    8
}

/// Routing + filtering configuration for a single sink.
///
/// Serde-ready for a future `[[cdc.outbound.sinks]]` TOML surface (server
/// auto-mount is deferred — see the slice plan).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CdcSinkConfig {
    /// Stable sink identifier (the per-sink delivery-state partition key).
    pub name:             String,
    /// Subject/topic template, e.g. `fraiseql.{tenant_id}.{table}`. Supported
    /// placeholders: `{tenant_id}`, `{table}`, `{op}`.
    pub subject_template: String,
    /// Optional table allow-list (`object_type`); `None` ⇒ all tables match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tables:           Option<Vec<String>>,
    /// Optional tenant allow-list; `None` ⇒ all tenants match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenants:          Option<Vec<Uuid>>,
    /// Delivery attempts before a tracking row is dead-lettered.
    #[serde(default = "default_max_attempts")]
    pub max_attempts:     i32,
}

impl CdcSinkConfig {
    /// Construct a config with the default `max_attempts` and no filters.
    #[must_use]
    pub fn new(name: impl Into<String>, subject_template: impl Into<String>) -> Self {
        Self {
            name:             name.into(),
            subject_template: subject_template.into(),
            tables:           None,
            tenants:          None,
            max_attempts:     default_max_attempts(),
        }
    }

    /// Restrict this sink to a set of tables (`object_type`).
    #[must_use]
    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.tables = Some(tables);
        self
    }

    /// Restrict this sink to a set of tenants.
    #[must_use]
    pub fn with_tenants(mut self, tenants: Vec<Uuid>) -> Self {
        self.tenants = Some(tenants);
        self
    }

    /// Whether an event passes this sink's per-tenant + per-table filter.
    ///
    /// This is the pure mirror of the SQL `enqueue` predicate — a `None`
    /// allow-list matches everything; a tenant allow-list rejects events with no
    /// tenant stamp.
    #[must_use]
    pub fn matches(&self, ev: &ChangeEvent) -> bool {
        if let Some(tables) = &self.tables {
            if !tables.iter().any(|t| t == &ev.object_type) {
                return false;
            }
        }
        if let Some(tenants) = &self.tenants {
            match ev.tenant_id {
                Some(tid) if tenants.contains(&tid) => {},
                _ => return false,
            }
        }
        true
    }
}

/// A broker sink that publishes change events.
///
/// Contrast with `fraiseql_core::runtime::subscription::TransportAdapter`
/// (fire-and-forget; an event is lost if the producer call fails): a `CdcSink`
/// is driven by the durable [`crate::DrainWorker`], so a broker outage produces
/// retry/backlog, never loss.
///
/// `publish` is declared with an explicit `impl Future + Send` return type
/// (RPITIT) rather than `async fn`, so no `async_trait` macro is introduced and
/// the returned future is spawnable.
pub trait CdcSink {
    /// The sink's stable name (matches its `CdcSinkConfig::name`).
    fn name(&self) -> &str;

    /// Which broker this sink targets.
    fn kind(&self) -> SinkKind;

    /// Whether this sink should receive the given event.
    fn matches(&self, ev: &ChangeEvent) -> bool;

    /// Publish one event, returning the delivery outcome.
    ///
    /// Implementations must never panic; transport failures are reported as
    /// [`PublishOutcome::Transient`] (retryable) or [`PublishOutcome::Permanent`]
    /// (dead-letter), never via the return type's error channel (there is none).
    fn publish(&self, ev: &ChangeEvent)
    -> impl std::future::Future<Output = PublishOutcome> + Send;
}

/// Render a subject/topic template against an event, sanitising every
/// interpolated value for the NATS subject charset.
///
/// Supported placeholders: `{tenant_id}` (a `None` tenant renders as `_none_`),
/// `{table}`, `{op}`. Returns `Err` — which the caller treats as a *permanent*
/// failure (dead-letter), never a silent re-route — if any interpolated value
/// contains a NATS-illegal character (`.`, `*`, `>`, whitespace, or control),
/// which would otherwise let a crafted tenant key escape into another subject
/// namespace (the topic-injection risk, R2).
///
/// # Errors
///
/// Returns the offending segment description if a value is empty or contains an
/// illegal character.
// Reason: the `{tenant_id}`/`{table}`/`{op}` literals are subject-template
// placeholders matched by `str::replace`, not `format!` arguments.
#[allow(clippy::literal_string_with_formatting_args)]
pub fn render_subject(template: &str, ev: &ChangeEvent) -> Result<String, String> {
    let tenant = ev.tenant_id.map_or_else(|| "_none_".to_owned(), |t| t.to_string());
    let table = sanitize_segment(&ev.object_type)?;
    let tenant = sanitize_segment(&tenant)?;
    let op = sanitize_segment(ev.op.as_str())?;
    Ok(template
        .replace("{tenant_id}", &tenant)
        .replace("{table}", &table)
        .replace("{op}", &op))
}

/// Validate a single interpolated subject segment against the NATS charset.
fn sanitize_segment(segment: &str) -> Result<String, String> {
    if segment.is_empty() {
        return Err("empty subject segment".to_owned());
    }
    for c in segment.chars() {
        if c == '.' || c == '*' || c == '>' || c.is_whitespace() || c.is_control() {
            return Err(format!(
                "subject segment {segment:?} contains NATS-illegal character {c:?}"
            ));
        }
    }
    Ok(segment.to_owned())
}

/// Capped exponential backoff for a 1-based retry attempt: attempt 1 → 1s,
/// 2 → 2s, 3 → 4s, … capped at 5 minutes.
#[must_use]
pub fn next_attempt_delay(attempt: u32) -> Duration {
    let secs = 1u64.checked_shl(attempt.saturating_sub(1)).unwrap_or(u64::MAX).min(300);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests;
