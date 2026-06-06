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
