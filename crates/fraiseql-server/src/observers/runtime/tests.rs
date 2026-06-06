//! Tests for the in-memory DLQ cap and retry-failure lifecycle (#343).
#![allow(clippy::unwrap_used)] // Reason: test code; lock/await failures should panic to surface bugs.

use std::collections::HashMap;

use fraiseql_observers::{ActionConfig, DeadLetterQueue, EntityEvent, EventKind};
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
        url:           Some("http://localhost/hook".to_string()),
        url_env:       None,
        headers:       HashMap::new(),
        body_template: None,
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
