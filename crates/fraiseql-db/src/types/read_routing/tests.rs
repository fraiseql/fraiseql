//! Unit tests for [`ReadRouting`](super::ReadRouting).
#![allow(clippy::unwrap_used)] // Reason: test code — a serde round-trip that fails should panic

use super::*;

#[test]
fn any_is_the_default_and_changes_nothing() {
    let routing = ReadRouting::default();
    assert_eq!(routing, ReadRouting::Any);
    assert!(routing.is_default());
    assert!(routing.allows_replica());
    assert!(routing.honours_write_pin());
    assert!(routing.allows_cached_result());
    assert_eq!(routing.effective_max_lag_ms(Some(500)), Some(500));
}

#[test]
fn primary_refuses_replicas_and_cached_results() {
    let routing = ReadRouting::Primary;
    assert!(!routing.allows_replica());
    assert!(
        !routing.allows_cached_result(),
        "a query that refused a stale replica must not then be served a stale cache entry"
    );
    assert!(routing.honours_write_pin());
}

#[test]
fn replica_opts_out_of_the_write_pin_and_may_set_its_own_budget() {
    let routing = ReadRouting::Replica {
        max_lag_ms: Some(30_000),
    };
    assert!(routing.allows_replica());
    assert!(!routing.honours_write_pin());
    assert_eq!(
        routing.effective_max_lag_ms(Some(500)),
        Some(30_000),
        "a per-query budget replaces the server's rather than intersecting with it"
    );
    assert_eq!(
        ReadRouting::Replica { max_lag_ms: None }.effective_max_lag_ms(Some(500)),
        Some(500),
        "without one of its own, the server's budget applies"
    );
}

#[test]
fn serde_round_trips_and_omits_the_default() {
    // The default must serialise to nothing at all where it is skipped, so a
    // schema authored before this field existed stays byte-identical.
    assert_eq!(serde_json::to_string(&ReadRouting::Any).unwrap(), "\"any\"");
    assert_eq!(serde_json::to_string(&ReadRouting::Primary).unwrap(), "\"primary\"");
    let replica = ReadRouting::Replica {
        max_lag_ms: Some(250),
    };
    let json = serde_json::to_string(&replica).unwrap();
    assert_eq!(json, r#"{"replica":{"max_lag_ms":250}}"#);
    assert_eq!(serde_json::from_str::<ReadRouting>(&json).unwrap(), replica);
    assert_eq!(
        serde_json::to_string(&ReadRouting::Replica { max_lag_ms: None }).unwrap(),
        r#"{"replica":{}}"#
    );
}
