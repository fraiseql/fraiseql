//! `[cdc_outbound]` — outbound change-data-capture to external brokers (#382).
//!
//! `fraiseql-cdc-sinks` shipped the whole engine — the durable per-sink
//! delivery state, the anti-join enqueue, the claim-then-publish drain with its
//! head-of-line ordering guard — and a NATS `JetStream` sink, but **nothing in
//! the shipped server constructed a `DrainWorker`**. Operators had to write
//! their own binary to use it. This section is that missing seam.

use serde::{Deserialize, Serialize};

/// Configuration for outbound CDC (`[cdc_outbound]`).
///
/// Presence of the section enables draining; absence leaves it off. Strict
/// (`deny_unknown_fields`): an unrecognised key is a boot error.
///
/// # Example (TOML)
///
/// ```toml
/// [cdc_outbound]
/// tick_interval_secs = 5
///
/// [[cdc_outbound.sinks]]
/// name = "warehouse"
/// kind = "nats-jetstream"
/// endpoint = "tls://nats.internal:4222"
/// subject_template = "fraiseql.{tenant_id}.{table}"
/// tables = ["tb_order", "tb_customer"]
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CdcOutboundConfig {
    /// The broker sinks to drain to. An empty list refuses to boot: a
    /// `[cdc_outbound]` section that drains nowhere is a configuration
    /// mistake, not a way to disable the subsystem (omit the section).
    pub sinks: Vec<CdcSinkSectionConfig>,

    /// Seconds between drain ticks (min 1).
    pub tick_interval_secs: u64,

    /// Rows published per tick, per sink (min 1).
    pub batch_size: i64,
}

impl Default for CdcOutboundConfig {
    fn default() -> Self {
        Self {
            sinks:              Vec::new(),
            tick_interval_secs: 5,
            batch_size:         256,
        }
    }
}

/// One `[[cdc_outbound.sinks]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdcSinkSectionConfig {
    /// Stable sink name — the delivery-state partition key. Renaming a sink
    /// makes it re-drain from its configured start, so treat it as durable.
    pub name: String,

    /// Broker kind. Only `nats-jetstream` is implemented; `kafka`, `kinesis`
    /// and `pulsar` are recognised names that refuse to boot rather than
    /// silently dropping every event (#382 tracks their implementations).
    pub kind: String,

    /// Broker endpoint, e.g. `tls://nats.internal:4222`.
    pub endpoint: String,

    /// Subject/topic template. Placeholders: `{tenant_id}`, `{table}`, `{op}`.
    /// An interpolated value containing broker-illegal characters
    /// dead-letters the event rather than being re-routed.
    pub subject_template: String,

    /// Optional table allow-list (`object_type`); absent drains all tables.
    #[serde(default)]
    pub tables: Option<Vec<String>>,

    /// Optional tenant allow-list; absent drains all tenants.
    #[serde(default)]
    pub tenants: Option<Vec<uuid::Uuid>>,

    /// Delivery attempts before a row is dead-lettered.
    #[serde(default)]
    pub max_attempts: Option<i32>,

    /// `JetStream` stream to ensure exists, capturing this sink's subjects.
    /// Absent means the stream is provisioned out of band (the production
    /// default — FraiseQL does not own the broker's topology).
    #[serde(default)]
    pub ensure_stream: Option<String>,
}

impl CdcOutboundConfig {
    /// Validate the section without touching a broker.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending key when the section drains
    /// nowhere, a tick/batch bound is violated, a sink name is duplicated, or
    /// a `kind` is unknown or not implemented.
    pub fn validate(&self) -> Result<(), String> {
        if self.sinks.is_empty() {
            return Err("[cdc_outbound] declares no sinks; remove the section to disable \
                        outbound CDC rather than configuring a drain with nowhere to go"
                .to_string());
        }
        if self.tick_interval_secs == 0 {
            return Err("[cdc_outbound] tick_interval_secs must be at least 1".to_string());
        }
        if self.batch_size < 1 {
            return Err("[cdc_outbound] batch_size must be at least 1".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for sink in &self.sinks {
            if sink.name.trim().is_empty() {
                return Err("[cdc_outbound] a sink has an empty name; the name is the \
                            delivery-state partition key"
                    .to_string());
            }
            if !seen.insert(sink.name.as_str()) {
                return Err(format!(
                    "[cdc_outbound] duplicate sink name {:?}; two sinks sharing a name would \
                     share one delivery-state partition and each mark the other's rows published",
                    sink.name
                ));
            }
            if sink.endpoint.trim().is_empty() {
                return Err(format!("[cdc_outbound] sink {:?} has an empty endpoint", sink.name));
            }
            if sink.subject_template.trim().is_empty() {
                return Err(format!(
                    "[cdc_outbound] sink {:?} has an empty subject_template",
                    sink.name
                ));
            }
            if sink.max_attempts.is_some_and(|attempts| attempts < 1) {
                return Err(format!(
                    "[cdc_outbound] sink {:?}: max_attempts must be at least 1",
                    sink.name
                ));
            }
            validate_kind(&sink.name, &sink.kind)?;
        }
        Ok(())
    }
}

/// Accept only the broker kinds this build can actually drain to.
///
/// A recognised-but-unimplemented kind is refused by name, and an unknown one
/// is refused as unknown: either way the server does not boot believing it is
/// replicating changes it would in fact drop on the floor.
fn validate_kind(sink: &str, kind: &str) -> Result<(), String> {
    match kind.to_ascii_lowercase().as_str() {
        "nats-jetstream" => Ok(()),
        known @ ("kafka" | "kinesis" | "pulsar") => Err(format!(
            "[cdc_outbound] sink {sink:?}: kind {known:?} is not implemented yet (#382 tracks \
             it). Refusing to boot rather than silently draining nothing."
        )),
        other => Err(format!(
            "[cdc_outbound] sink {sink:?}: unknown kind {other:?}; expected \"nats-jetstream\""
        )),
    }
}
