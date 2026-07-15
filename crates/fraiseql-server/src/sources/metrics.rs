//! Prometheus metrics for scheduled ingress sources (#573 Phase 08).
//!
//! Emitted through the lightweight [`metrics`] facade — the same mechanism
//! `fraiseql-wire` uses for its per-entity counters and histograms, and the only
//! idiom in the workspace that natively supports the per-source labels and the
//! run-duration histogram a source needs. Emissions are captured by the global
//! Prometheus recorder installed at startup
//! ([`crate::metrics_recorder`]) and appended to the `/metrics` endpoint, so
//! **they surface only when the server is built with the `metrics` feature and
//! metrics are enabled** (the same boundary as the wire metrics). The macro calls
//! themselves are cheap no-ops when no recorder is installed, so this module needs
//! no feature gate of its own.
//!
//! Three metrics, all driven from [`SourcePoller::fire_once`](super::SourcePoller):
//!
//! - `fraiseql_source_fires_total{source, result}` — a source firing that ran to completion
//!   (`result="ok"`) or whose connector returned an error (`result="error"`). A failed firing
//!   re-runs from the last committed cursor on the next tick (at-least-once), so `result="error"`
//!   is the source-failure signal — there is no separate source dead-letter queue.
//! - `fraiseql_source_skips_not_leader_total{source}` — a tick this replica skipped because another
//!   replica held the single-firing lease. The fleet-wide health of the cross-replica coordination.
//! - `fraiseql_source_run_duration_seconds{source}` — wall-clock of a firing that ran (skips are
//!   not recorded — a skip is near-instant and not a run).

use metrics::{counter, histogram};

/// `fraiseql_source_fires_total{source, result}` — firings that ran, by outcome.
const FIRES_TOTAL: &str = "fraiseql_source_fires_total";
/// `fraiseql_source_skips_not_leader_total{source}` — ticks skipped (not leader).
const SKIPS_NOT_LEADER_TOTAL: &str = "fraiseql_source_skips_not_leader_total";
/// `fraiseql_source_run_duration_seconds{source}` — run wall-clock, seconds.
const RUN_DURATION_SECONDS: &str = "fraiseql_source_run_duration_seconds";

/// The `result` label for a firing that ran: the connector completed.
pub const RESULT_OK: &str = "ok";
/// The `result` label for a firing that ran but whose connector returned an error.
pub const RESULT_ERROR: &str = "error";

/// Record that `source` fired and ran to completion with the given `result`
/// (`"ok"` or `"error"`), taking `duration_seconds` of wall-clock.
///
/// One call covers both the fire counter and the run-duration histogram — the two
/// only ever move together (a run has an outcome and a duration), so pairing them
/// keeps the caller from recording one without the other.
pub fn record_fire(source: &str, result: &str, duration_seconds: f64) {
    counter!(FIRES_TOTAL, "source" => source.to_string(), "result" => result.to_string())
        .increment(1);
    histogram!(RUN_DURATION_SECONDS, "source" => source.to_string()).record(duration_seconds);
}

/// Record that `source` skipped a tick because another replica held the lease.
pub fn record_skip_not_leader(source: &str) {
    counter!(SKIPS_NOT_LEADER_TOTAL, "source" => source.to_string()).increment(1);
}

#[cfg(test)]
mod tests;
