//! Pure (no-database) unit tests for the source coordination primitives: the
//! stable lock-id hash, the cursor snapshot semantics, and the in-process runner's
//! acquire → run → release happy path. The database-backed compare-and-swap, RLS,
//! and cross-replica single-firing are covered in `tests/source_cursor_pg.rs`.

use super::{CursorSnapshot, LeaseGuardedRunner, RunOutcome, lock_id};

#[test]
fn lock_id_is_deterministic() {
    assert_eq!(lock_id("orders-poll"), lock_id("orders-poll"));
    assert_eq!(lock_id("email"), lock_id("email"));
}

#[test]
fn lock_id_is_distinct_per_source_name() {
    let names = ["email", "orders-poll", "stripe-sync", "gmail-history", ""];
    for (i, a) in names.iter().enumerate() {
        for b in &names[i + 1..] {
            assert_ne!(
                lock_id(a),
                lock_id(b),
                "distinct names must derive distinct lock ids: {a} vs {b}"
            );
        }
    }
}

#[test]
fn cursor_snapshot_empty_is_unset() {
    let empty = CursorSnapshot::empty();
    assert!(empty.is_unset());
    assert_eq!(empty.value, None);
    assert_eq!(empty.version, 0);
    assert_eq!(empty, CursorSnapshot::default());
}

#[test]
fn cursor_snapshot_with_version_is_set() {
    let snap = CursorSnapshot {
        value:   Some(serde_json::json!({"last_uid": 42})),
        version: 3,
    };
    assert!(!snap.is_unset());
}

#[tokio::test]
async fn in_process_runner_runs_the_closure_and_releases() {
    let runner = LeaseGuardedRunner::in_process("test-source");
    assert_eq!(runner.source_name(), "test-source");

    // First run wins the lease and executes the closure.
    let outcome = runner.run(|| async { 42_u32 }).await.expect("acquire must not fail");
    assert_eq!(outcome, RunOutcome::Ran(42));
    assert_eq!(runner.skips_not_leader(), 0);

    // The lease was released explicitly, so a second run wins it again — proving
    // acquire → run → release round-trips rather than leaving the lock held.
    let outcome = runner.run(|| async { 7_u32 }).await.expect("acquire must not fail");
    assert_eq!(outcome, RunOutcome::Ran(7));
    assert_eq!(runner.skips_not_leader(), 0);
}
