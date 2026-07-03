//! Tests for the in-memory DLQ cap (#343), retry-failure lifecycle (#343), and
//! the atomic claim primitive (#344).
#![allow(clippy::unwrap_used)] // Reason: test code; lock/await failures should panic to surface bugs.

use std::{
    collections::HashMap,
    sync::{Arc, Barrier},
};

use fraiseql_observers::{ActionConfig, DeadLetterQueue, DlqItem, EntityEvent, EventKind};
use uuid::Uuid;

use super::InMemoryDlq;

fn test_event() -> EntityEvent {
    EntityEvent::new(
        EventKind::Created,
        "TestEntity".to_string(),
        Uuid::new_v4(),
        serde_json::json!({}),
    )
}

fn test_action() -> ActionConfig {
    ActionConfig::Webhook {
        url:                Some("http://localhost/hook".to_string()),
        url_env:            None,
        headers:            HashMap::new(),
        body_template:      None,
        signing_secret:     None,
        signing_secret_env: None,
    }
}

async fn push(dlq: &InMemoryDlq) -> Uuid {
    dlq.push(test_event(), test_action(), "boom".to_string()).await.unwrap()
}

// ── Cycle 1: cap (drop-newest) ──────────────────────────────────────────────

#[tokio::test]
async fn unbounded_dlq_grows_without_limit() {
    let dlq = InMemoryDlq::new_with_max(None);
    for _ in 0..5 {
        push(&dlq).await;
    }
    assert_eq!(dlq.count(), 5);
    assert_eq!(dlq.overflow_count(), 0);
}

#[tokio::test]
async fn capped_dlq_drops_newest_at_capacity() {
    let dlq = InMemoryDlq::new_with_max(Some(2));

    let first = push(&dlq).await;
    let second = push(&dlq).await;
    // Third push is at capacity → dropped (drop-newest): the first two remain.
    push(&dlq).await;

    assert_eq!(dlq.count(), 2, "cap should hold the queue at 2 entries");
    assert_eq!(dlq.overflow_count(), 1, "the dropped entry should bump the overflow counter");

    let ids: Vec<Uuid> = dlq.list_all().into_iter().map(|i| i.id).collect();
    assert!(
        ids.contains(&first) && ids.contains(&second),
        "the first two entries are retained"
    );
}

// ── Cycle 2: mark_retry_failed keeps the item ───────────────────────────────

#[tokio::test]
async fn mark_retry_failed_keeps_item_and_records_failure() {
    let dlq = InMemoryDlq::new_with_max(None);
    let id = push(&dlq).await;

    dlq.mark_retry_failed(id, "second failure").await.unwrap();

    let item = dlq.get(id).expect("item must still be present after a failed retry");
    assert_eq!(item.attempts, 1, "attempts should be incremented");
    assert_eq!(item.error_message, "second failure", "error_message should be updated");
    assert_eq!(dlq.count(), 1);
}

#[tokio::test]
async fn mark_success_removes_item() {
    let dlq = InMemoryDlq::new_with_max(None);
    let id = push(&dlq).await;

    dlq.mark_success(id).await.unwrap();

    assert!(dlq.get(id).is_none(), "a succeeded item should be removed");
    assert_eq!(dlq.count(), 0);
}

// ── #344: atomic claim ──────────────────────────────────────────────────────

#[tokio::test]
async fn try_claim_removes_and_is_idempotent() {
    let dlq = InMemoryDlq::new_with_max(None);
    let id = push(&dlq).await;

    assert!(dlq.try_claim(id).is_some(), "first claim returns the item");
    assert_eq!(dlq.count(), 0, "claim removes the item");
    assert!(dlq.try_claim(id).is_none(), "second claim finds nothing");
}

#[tokio::test]
async fn try_claim_is_atomic_under_concurrency() {
    // Push one item, then race N claimers: exactly one must win. This is the
    // exactly-once gate the retry handlers rely on (#344) — the old
    // get→process→remove path let every racer dispatch the action.
    let dlq = Arc::new(InMemoryDlq::new_with_max(None));
    let id = push(&dlq).await;

    let n = 8;
    let barrier = Arc::new(Barrier::new(n));
    // Spawn ALL threads before joining any — collecting here is intentional so
    // the claimers actually race rather than run serially.
    #[allow(clippy::needless_collect)] // Reason: must spawn all before joining; see comment above.
    let handles: Vec<_> = (0..n)
        .map(|_| {
            let dlq = Arc::clone(&dlq);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                dlq.try_claim(id).is_some()
            })
        })
        .collect();

    let winners = handles.into_iter().map(|h| h.join().unwrap()).filter(|&won| won).count();
    assert_eq!(winners, 1, "exactly one of {n} concurrent claimers should win");
    assert_eq!(dlq.count(), 0);
}

#[tokio::test]
async fn reinsert_bypasses_the_cap() {
    let dlq = InMemoryDlq::new_with_max(Some(1));
    push(&dlq).await; // fills to cap

    // A claimed item whose retry failed must be restored even when the DLQ
    // refilled to capacity during the claim — never silently dropped (#343/#344).
    let claimed = DlqItem {
        id:            Uuid::new_v4(),
        event:         test_event(),
        action:        test_action(),
        error_message: "retry failed".to_string(),
        attempts:      1,
    };
    dlq.reinsert(claimed);

    assert_eq!(dlq.count(), 2, "reinsert must bypass the cap");
    assert_eq!(dlq.overflow_count(), 0, "reinsert is not an overflow");
}

// ── Function-dispatch DLQ ───────────────────────────────────────────────────

mod function_dlq {
    use fraiseql_observers::{DeadLetterQueue, DispatchSource, FunctionDispatchRecord};

    use super::InMemoryDlq;

    fn record(error: &str) -> FunctionDispatchRecord {
        FunctionDispatchRecord::new(
            DispatchSource::AfterMutation,
            "onUserCreated",
            "after:mutation:onUserCreated",
            serde_json::json!({ "event_kind": "insert", "new": { "id": "u1" } }),
            error,
            3,
        )
    }

    #[tokio::test]
    async fn exhausted_dispatch_lands_one_row() {
        // A permanently-failing function dispatch that exhausted its retries
        // lands exactly one inspectable DLQ row.
        let dlq = InMemoryDlq::new_with_max(None);

        let id = dlq.push_function(record("upstream 503")).await.unwrap();

        assert_eq!(dlq.function_count(), 1, "one function DLQ row after exhaustion");
        let pending = dlq.get_pending_functions(10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
        assert_eq!(pending[0].function_name, "onUserCreated");
        assert_eq!(pending[0].attempts, 3);
        assert_eq!(pending[0].error_message, "upstream 503");
    }

    #[tokio::test]
    async fn function_and_observer_entries_are_separate() {
        // The two collections share one store but are counted independently, so a
        // function failure never masks an observer failure or vice versa.
        let dlq = InMemoryDlq::new_with_max(None);

        super::push(&dlq).await; // observer-action failure
        dlq.push_function(record("boom")).await.unwrap();

        assert_eq!(dlq.count(), 1, "observer entries counted separately");
        assert_eq!(dlq.function_count(), 1, "function entries counted separately");
    }

    #[tokio::test]
    async fn capped_function_dlq_drops_newest() {
        let dlq = InMemoryDlq::new_with_max(Some(2));

        dlq.push_function(record("e1")).await.unwrap();
        dlq.push_function(record("e2")).await.unwrap();
        // Third is at capacity → dropped (drop-newest), mirroring `push`.
        dlq.push_function(record("e3")).await.unwrap();

        assert_eq!(dlq.function_count(), 2, "cap holds the function queue at 2");
        assert_eq!(dlq.overflow_count(), 1, "the dropped entry bumps the overflow counter");
    }
}

// ── Listener selection seam (#350) ──────────────────────────────────────────

mod listener_selection {
    use fraiseql_observers::config::TransportKind;

    use super::super::{ListenerSelection, listener_selection};

    #[test]
    fn postgres_uses_the_change_log_listener() {
        assert_eq!(
            listener_selection(TransportKind::Postgres),
            ListenerSelection::PostgresChangeLog,
        );
    }

    #[test]
    fn nats_uses_the_transport_stream_not_the_pg_listener() {
        // The whole point of #350: a NATS selection must NOT fall through to the
        // PostgreSQL listener.
        assert_eq!(listener_selection(TransportKind::Nats), ListenerSelection::TransportStream,);
    }

    #[test]
    fn in_memory_uses_the_transport_stream() {
        assert_eq!(listener_selection(TransportKind::InMemory), ListenerSelection::TransportStream,);
    }
}

// ── Transport boot-fatality predicate (#350) ────────────────────────────────

mod transport_requires_broker {
    use fraiseql_observers::config::TransportKind;

    use super::super::{ObserverRuntime, ObserverRuntimeConfig};

    fn runtime_with(kind: TransportKind) -> ObserverRuntime {
        // A lazy pool never connects, so this needs no database.
        let pool = sqlx::PgPool::connect_lazy("postgres://u:u@127.0.0.1:1/db")
            .expect("lazy pool construction does not connect");
        let mut config = ObserverRuntimeConfig::new(pool);
        config.transport.transport = kind;
        ObserverRuntime::new(config)
    }

    #[tokio::test]
    async fn postgres_start_failure_is_not_boot_fatal() {
        // The default transport keeps the resilient log-and-continue behaviour.
        assert!(!runtime_with(TransportKind::Postgres).transport_requires_broker());
    }

    #[tokio::test]
    async fn nats_start_failure_is_boot_fatal() {
        // A broker-backed transport must take the server down in production if it
        // cannot start, never silently come up on PostgreSQL (#350).
        assert!(runtime_with(TransportKind::Nats).transport_requires_broker());
    }
}

// ── NATS-cannot-run boot gate (#350 acceptance) ─────────────────────────────

// Deliberately NOT gated on `observers-nats`: this must run in the CI `test`
// leg (which compiles `observers` but not `observers-nats`) so it is never a
// false-green. The asserted property — a configured NATS transport that cannot
// run makes `start()` FAIL rather than silently fall back to the PostgreSQL
// listener — holds in both builds, only the failure *reason* differs:
//   • without `observers-nats`: `start_transport_stream` hits the
//     "lacks the observers-nats feature" arm;
//   • with `observers-nats`: `NatsTransport::new` rejects the loopback URL via
//     the transport SSRF guard before any network I/O (a deterministic stand-in
//     for a dead broker, needing neither PG nor NATS infra).
mod nats_unrunnable_gate {
    use fraiseql_observers::config::TransportKind;

    use super::super::{ObserverRuntime, ObserverRuntimeConfig};

    #[tokio::test]
    async fn nats_that_cannot_run_fails_start_with_no_pg_fallback() {
        // A lazy pool never connects, and start_transport_stream builds the
        // transport before touching the database, so this needs no PG.
        let pool = sqlx::PgPool::connect_lazy("postgres://u:u@127.0.0.1:1/db")
            .expect("lazy pool construction does not connect");
        let mut config = ObserverRuntimeConfig::new(pool);
        config.transport.transport = TransportKind::Nats;
        config.transport.nats.url = "nats://127.0.0.1:4222".to_string();

        let mut runtime = ObserverRuntime::new(config);
        let result = runtime.start().await;

        assert!(
            result.is_err(),
            "NATS that cannot run must fail start(), not fall back to PostgreSQL"
        );
        assert!(
            !runtime.is_running(),
            "runtime must not report running after a failed NATS start"
        );
        assert!(
            runtime.transport_requires_broker(),
            "a NATS transport is broker-backed, so the start failure is boot-fatal in production"
        );
    }
}

/// `truncate_log_payload` (#468): small payloads pass through, oversized ones
/// become a bounded marker.
mod log_payload_truncation {
    use super::super::{MAX_LOG_PAYLOAD_BYTES, truncate_log_payload};

    #[test]
    fn small_payload_is_passed_through_unchanged() {
        let data = serde_json::json!({"id": "abc", "status": "new"});
        assert_eq!(truncate_log_payload(&data), data);
    }

    #[test]
    fn oversized_payload_is_replaced_with_a_size_marker() {
        // A string value comfortably larger than the cap.
        let big = "x".repeat(MAX_LOG_PAYLOAD_BYTES + 1_024);
        let data = serde_json::json!({ "blob": big });

        let out = truncate_log_payload(&data);
        assert_eq!(out["_truncated"], serde_json::Value::Bool(true));
        let recorded = out["_original_size_bytes"].as_u64().unwrap();
        assert!(
            recorded > u64::try_from(MAX_LOG_PAYLOAD_BYTES).unwrap(),
            "marker must record the original (oversized) byte length"
        );
        // The original (large) content must not be persisted verbatim.
        assert!(out.get("blob").is_none());
    }
}
