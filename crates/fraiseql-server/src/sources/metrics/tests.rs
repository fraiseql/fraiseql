//! Tests for the source metrics facade emitters.
//!
//! Following the `fraiseql-wire` metrics precedent, the emitters are exercised for
//! "does not panic" (the facade macro is a no-op without an installed recorder, so
//! there is nothing to snapshot in a unit test) plus a guard on the metric *names*,
//! which are a stable wire contract dashboards depend on.

use super::{
    FIRES_TOTAL, RESULT_ERROR, RESULT_OK, RUN_DURATION_SECONDS, SKIPS_NOT_LEADER_TOTAL,
    record_fire, record_skip_not_leader,
};

#[test]
fn metric_names_are_the_expected_wire_contract() {
    assert_eq!(FIRES_TOTAL, "fraiseql_source_fires_total");
    assert_eq!(SKIPS_NOT_LEADER_TOTAL, "fraiseql_source_skips_not_leader_total");
    assert_eq!(RUN_DURATION_SECONDS, "fraiseql_source_run_duration_seconds");
    assert_eq!(RESULT_OK, "ok");
    assert_eq!(RESULT_ERROR, "error");
}

#[test]
fn record_fire_does_not_panic_for_either_result() {
    record_fire("orders", RESULT_OK, 0.25);
    record_fire("orders", RESULT_ERROR, 1.5);
}

#[test]
fn record_skip_not_leader_does_not_panic() {
    record_skip_not_leader("orders");
}
