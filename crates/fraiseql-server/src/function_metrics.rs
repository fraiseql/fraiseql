//! Prometheus metrics for function-trigger dispatch (#598).
//!
//! The function-dispatch sibling of [`sources::metrics`](crate::sources) — sources
//! got a full metric set (`fraiseql_source_*`); function dispatch (after:mutation,
//! after:ingest, after:capture, cron) had none, so nothing about a fired / failed /
//! dead-lettered function reached `/metrics`. This module closes that gap with the
//! same idiom: the lightweight [`metrics`] facade, captured by the global Prometheus
//! recorder installed at startup ([`crate::metrics_recorder`]) and appended to the
//! `/metrics` endpoint, so **emissions surface only when the server is built with the
//! `metrics` feature and metrics are enabled** (the wire-metrics boundary). The macro
//! calls are cheap no-ops when no recorder is installed, so this module — like
//! `sources::metrics` — needs no feature gate of its own.
//!
//! The metric set:
//!
//! - `fraiseql_function_dispatches_total{function, trigger_kind, result}` — one background function
//!   dispatch that ran, by `trigger_kind` (`after:mutation`, `after:ingest`, `after:capture`,
//!   `cron`) and `result` (`ok` | `error` | `dead_lettered`). A fire-and-forget (`re_runnable`)
//!   single-attempt failure is `result="error"` (no retry, no dead-letter — the alertable signal
//!   #598 notes was missing); a durable dispatch that exhausted retries is `dead_lettered`.
//! - `fraiseql_function_run_duration_seconds{function}` — wall-clock of a dispatch that ran (all
//!   retry attempts included), recorded alongside the counter.
//! - `fraiseql_function_predicate_skips_total{function}` — a `when` predicate (Phase 04) evaluated
//!   false: no isolate spun, no dispatch record. This is the counter Phase 10's audit uses to
//!   verify predicate evaluation cost.
//! - `fraiseql_function_dlq_size` — current function-dispatch DLQ depth (this replica's view; each
//!   replica reports its own store count).
//! - `fraiseql_function_dlq_evictions_total` — function-dispatch entries dropped because the DLQ
//!   was at capacity (drop-newest). Eviction must never be Prometheus-invisible — today's
//!   drop-newest already warns and counts internally (`dlq_dropped`) but never reached `/metrics`.
//!
//! **Scope note — trigger kinds not covered here.** `before:mutation` runs
//! synchronously in the request path (its outcome is the mutation's own
//! success/failure, already visible on the GraphQL/HTTP metrics), and `http` edge
//! functions (`POST /functions/v1/{name}`) return their result to the caller and are
//! metered by the HTTP layer — neither is a background dispatch, so neither is a
//! `fraiseql_function_dispatches_total` row. `after:storage` has no runtime dispatch
//! path yet (parsed/validated only), so there is nothing to meter — no silent gap.

use metrics::counter;

/// `fraiseql_function_dispatches_total{function, trigger_kind, result}`.
const DISPATCHES_TOTAL: &str = "fraiseql_function_dispatches_total";
/// `fraiseql_function_run_duration_seconds{function}` — dispatch wall-clock, seconds.
const RUN_DURATION_SECONDS: &str = "fraiseql_function_run_duration_seconds";
/// `fraiseql_function_predicate_skips_total{function}` — `when` evaluated false.
const PREDICATE_SKIPS_TOTAL: &str = "fraiseql_function_predicate_skips_total";
/// `fraiseql_function_dlq_size` — current function-dispatch DLQ depth (this replica).
const DLQ_SIZE: &str = "fraiseql_function_dlq_size";
/// `fraiseql_function_dlq_evictions_total` — entries dropped at DLQ capacity.
const DLQ_EVICTIONS_TOTAL: &str = "fraiseql_function_dlq_evictions_total";

/// `result` label — the dispatch ran to completion.
pub const RESULT_OK: &str = "ok";
/// `result` label — a fire-and-forget dispatch failed its single attempt (not
/// retried, not dead-lettered).
pub const RESULT_ERROR: &str = "error";
/// `result` label — a durable dispatch exhausted its retries (or failed
/// permanently) and was routed to the dead-letter queue.
pub const RESULT_DEAD_LETTERED: &str = "dead_lettered";

/// `trigger_kind` label — an `after:mutation` dispatch. Matches
/// [`DispatchSource::AfterMutation.label()`](fraiseql_observers::DispatchSource::label).
pub const KIND_AFTER_MUTATION: &str = "after:mutation";
/// `trigger_kind` label — an `after:ingest` dispatch.
pub const KIND_AFTER_INGEST: &str = "after:ingest";
/// `trigger_kind` label — an `after:capture` dispatch (externally-captured write).
pub const KIND_AFTER_CAPTURE: &str = "after:capture";
/// `trigger_kind` label — a `cron:` scheduled firing.
pub const KIND_CRON: &str = "cron";

/// The coarse `trigger_kind` metric label for a dispatch `source`.
///
/// The single mapping the durable dispatcher uses, so the label vocabulary lives in
/// one place. The strings equal
/// [`DispatchSource::label()`](fraiseql_observers::DispatchSource::label) (pinned by a test) — that
/// method's value also seeds the idempotency token, a separate concern kept deliberately in
/// agreement.
#[cfg(feature = "observers")]
#[must_use]
pub const fn trigger_kind(source: fraiseql_observers::DispatchSource) -> &'static str {
    use fraiseql_observers::DispatchSource;
    match source {
        DispatchSource::AfterMutation => KIND_AFTER_MUTATION,
        DispatchSource::AfterIngest => KIND_AFTER_INGEST,
        DispatchSource::AfterCapture => KIND_AFTER_CAPTURE,
        // A scheduled `Source` poll is metered by `sources::metrics`, not here (never
        // a `fraiseql_function_dispatches_total` row); any future variant of the
        // `#[non_exhaustive]` enum maps to this stable fallback until it earns a kind.
        _ => "source",
    }
}

/// Record that `function` ran one dispatch of `trigger_kind` with the given
/// `result`, taking `duration_seconds` of wall-clock.
///
/// One call covers both the dispatch counter and the run-duration histogram — the
/// two only ever move together (a dispatch has an outcome and a duration), so
/// pairing them keeps a caller from recording one without the other. Mirrors
/// [`sources::metrics::record_fire`](crate::sources).
pub fn record_dispatch(function: &str, trigger_kind: &str, result: &str, duration_seconds: f64) {
    counter!(
        DISPATCHES_TOTAL,
        "function" => function.to_string(),
        "trigger_kind" => trigger_kind.to_string(),
        "result" => result.to_string(),
    )
    .increment(1);
    metrics::histogram!(RUN_DURATION_SECONDS, "function" => function.to_string())
        .record(duration_seconds);
}

/// Record that `function`'s `when` predicate evaluated false, so no isolate spun
/// and no dispatch record was produced (Phase 04's zero-cost-skip contract).
pub fn record_predicate_skip(function: &str) {
    counter!(PREDICATE_SKIPS_TOTAL, "function" => function.to_string()).increment(1);
}

/// Set the current function-dispatch DLQ depth for this replica.
///
/// A point-in-time gauge: callers set it to the store's function-record count after
/// every push (and eviction). Across replicas each reports its own store's view.
pub fn set_dlq_size(size: usize) {
    #[allow(clippy::cast_precision_loss)]
    // Reason: DLQ depth is small; f64 is exact well past any cap.
    metrics::gauge!(DLQ_SIZE).set(size as f64);
}

/// Record that one function-dispatch DLQ entry was dropped because the store was at
/// capacity (drop-newest). The Prometheus-visible counterpart to the per-drop
/// `warn!` and the internal `overflow_count`.
pub fn record_dlq_eviction() {
    counter!(DLQ_EVICTIONS_TOTAL).increment(1);
}

#[cfg(test)]
mod tests;
