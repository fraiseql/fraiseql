//! Tests for the function-dispatch metrics facade.
//!
//! Following the `sources::metrics` precedent (and `fraiseql-wire` before it): the
//! emitters are exercised for "does not panic" (the facade macro is a no-op without
//! an installed recorder, so there is nothing to snapshot in a unit test) plus a
//! guard on the metric *names* and label *values*, which are a stable wire contract
//! dashboards and the Phase-10 audit depend on.

use super::{
    DISPATCHES_TOTAL, DLQ_EVICTIONS_TOTAL, DLQ_SIZE, KIND_AFTER_CAPTURE, KIND_AFTER_INGEST,
    KIND_AFTER_MUTATION, KIND_CRON, PREDICATE_SKIPS_TOTAL, RESULT_DEAD_LETTERED, RESULT_ERROR,
    RESULT_OK, RUN_DURATION_SECONDS, record_dispatch, record_dlq_eviction, record_predicate_skip,
    set_dlq_size,
};

#[test]
fn metric_names_are_the_expected_wire_contract() {
    assert_eq!(DISPATCHES_TOTAL, "fraiseql_function_dispatches_total");
    assert_eq!(RUN_DURATION_SECONDS, "fraiseql_function_run_duration_seconds");
    assert_eq!(PREDICATE_SKIPS_TOTAL, "fraiseql_function_predicate_skips_total");
    assert_eq!(DLQ_SIZE, "fraiseql_function_dlq_size");
    assert_eq!(DLQ_EVICTIONS_TOTAL, "fraiseql_function_dlq_evictions_total");
}

#[test]
fn result_labels_are_the_expected_wire_contract() {
    assert_eq!(RESULT_OK, "ok");
    assert_eq!(RESULT_ERROR, "error");
    assert_eq!(RESULT_DEAD_LETTERED, "dead_lettered");
}

// The `trigger_kind` label a `DurableDispatcher` emits is `DispatchSource::label()`.
// Pin that the two agree so a relabel on either side is caught here rather than
// silently splitting a dashboard series. Gated on `observers` — the feature that
// makes `fraiseql-observers` (and `DispatchSource`) a direct dependency.
#[cfg(feature = "observers")]
#[test]
fn trigger_kind_labels_match_the_dispatch_source_labels() {
    use fraiseql_observers::DispatchSource;

    assert_eq!(KIND_AFTER_MUTATION, DispatchSource::AfterMutation.label());
    assert_eq!(KIND_AFTER_INGEST, DispatchSource::AfterIngest.label());
    assert_eq!(KIND_AFTER_CAPTURE, DispatchSource::AfterCapture.label());
    // Cron has no dedicated DispatchSource (it borrows `Source` as an idempotency
    // salt); its metric kind is a distinct literal, asserted here so it stays stable.
    assert_eq!(KIND_CRON, "cron");

    // The production mapping (what the dispatcher actually calls) agrees.
    assert_eq!(super::trigger_kind(DispatchSource::AfterMutation), KIND_AFTER_MUTATION);
    assert_eq!(super::trigger_kind(DispatchSource::AfterIngest), KIND_AFTER_INGEST);
    assert_eq!(super::trigger_kind(DispatchSource::AfterCapture), KIND_AFTER_CAPTURE);
}

#[test]
fn record_dispatch_does_not_panic_for_any_result() {
    record_dispatch("notify", KIND_AFTER_MUTATION, RESULT_OK, 0.25);
    record_dispatch("notify", KIND_AFTER_INGEST, RESULT_ERROR, 1.5);
    record_dispatch("sweep", KIND_CRON, RESULT_DEAD_LETTERED, 3.0);
    record_dispatch("mirror", KIND_AFTER_CAPTURE, RESULT_OK, 0.01);
}

#[test]
fn record_predicate_skip_does_not_panic() {
    record_predicate_skip("notify_approved");
}

#[test]
fn dlq_gauge_and_eviction_do_not_panic() {
    set_dlq_size(0);
    set_dlq_size(42);
    record_dlq_eviction();
}
