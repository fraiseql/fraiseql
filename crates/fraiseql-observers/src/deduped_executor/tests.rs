use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::{event::EventKind, matcher::EventMatcher, testing::mocks::MockDeadLetterQueue};

// Simple in-memory dedup store for testing.
// `claim_event` uses DashMap's vacant-entry insertion which is atomic at
// the shard level, matching the SET NX semantics of the Redis backend.
#[derive(Clone)]
struct InMemoryDedupStore {
    store: Arc<dashmap::DashMap<String, bool>>,
    window_seconds: u64,
}

impl InMemoryDedupStore {
    fn new(window_seconds: u64) -> Self {
        Self {
            store: Arc::new(dashmap::DashMap::new()),
            window_seconds,
        }
    }
}

#[async_trait::async_trait]
impl DeduplicationStore for InMemoryDedupStore {
    async fn claim_event(&self, event_key: &str) -> Result<bool> {
        use dashmap::Entry;
        match self.store.entry(event_key.to_string()) {
            Entry::Vacant(e) => {
                e.insert(true);
                Ok(true) // claimed by us
            },
            Entry::Occupied(_) => Ok(false), // already claimed
        }
    }

    async fn is_duplicate(&self, event_key: &str) -> Result<bool> {
        Ok(self.store.contains_key(event_key))
    }

    async fn mark_processed(&self, event_key: &str) -> Result<()> {
        self.store.insert(event_key.to_string(), true);
        Ok(())
    }

    fn window_seconds(&self) -> u64 {
        self.window_seconds
    }

    fn set_window_seconds(&mut self, seconds: u64) {
        self.window_seconds = seconds;
    }

    async fn remove(&self, event_key: &str) -> Result<()> {
        self.store.remove(event_key);
        Ok(())
    }
}

#[tokio::test]
async fn test_dedup_prevents_duplicate_processing() {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq);

    let dedup_store = InMemoryDedupStore::new(300);
    let deduped = DedupedObserverExecutor::new(executor, dedup_store);

    let event = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(),
        json!({"name": "test"}),
    );

    // First processing - should succeed
    let summary1 = deduped.process_event(&event).await.unwrap();
    assert!(!summary1.duplicate_skipped);

    // Second processing (duplicate) - should skip
    let summary2 = deduped.process_event(&event).await.unwrap();
    assert!(summary2.duplicate_skipped);
    assert_eq!(summary2.successful_actions, 0);
    assert_eq!(summary2.failed_actions, 0);
}

#[tokio::test]
async fn test_dedup_different_events_not_deduplicated() {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq);

    let dedup_store = InMemoryDedupStore::new(300);
    let deduped = DedupedObserverExecutor::new(executor, dedup_store);

    let event1 = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(),
        json!({"name": "test1"}),
    );

    let event2 = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(),
        json!({"name": "test2"}),
    );

    // Process first event
    let summary1 = deduped.process_event(&event1).await.unwrap();
    assert!(!summary1.duplicate_skipped);

    // Process second event (different UUIDv4) - should not be deduplicated
    let summary2 = deduped.process_event(&event2).await.unwrap();
    assert!(!summary2.duplicate_skipped);
}

#[tokio::test]
async fn test_dedup_window_seconds() {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq);

    let dedup_store = InMemoryDedupStore::new(600);
    let deduped = DedupedObserverExecutor::new(executor, dedup_store);

    assert_eq!(deduped.window_seconds(), 600);
}

#[tokio::test]
async fn test_dedup_inner_access() {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq);

    let dedup_store = InMemoryDedupStore::new(300);
    let deduped = DedupedObserverExecutor::new(executor, dedup_store);

    let _inner = deduped.inner();
    let _store = deduped.dedup_store();
}

/// Verify that concurrent `claim_event` calls for the same key result in
/// exactly one winner — the core property that eliminates the TOCTOU race.
#[tokio::test]
async fn test_claim_event_concurrent_only_one_winner() {
    let store = InMemoryDedupStore::new(300);
    let store = Arc::new(store);

    let tasks: Vec<_> = (0..16)
        .map(|_| {
            let s = store.clone();
            tokio::spawn(async move { s.claim_event("event:concurrent-key").await.unwrap() })
        })
        .collect();

    let results: Vec<bool> =
        futures::future::join_all(tasks).await.into_iter().map(|r| r.unwrap()).collect();

    let winners = results.iter().filter(|&&v| v).count();
    assert_eq!(winners, 1, "exactly one worker must win the claim");
}

/// Verify that un-claiming (remove) after a processing failure allows
/// the same event to be claimed again on the next attempt.
#[tokio::test]
async fn test_claim_event_unclaim_on_failure_allows_retry() {
    let store = InMemoryDedupStore::new(300);

    // First claim succeeds.
    assert!(store.claim_event("event:retry-key").await.unwrap());
    // Key is now held — second claim fails.
    assert!(!store.claim_event("event:retry-key").await.unwrap());

    // Simulate processing failure: un-claim the key.
    store.remove("event:retry-key").await.unwrap();

    // Event can be claimed again for retry.
    assert!(store.claim_event("event:retry-key").await.unwrap());
}

// -------------------------------------------------------------------
// TenantScope unit tests
// -------------------------------------------------------------------

fn make_executor() -> ObserverExecutor {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    ObserverExecutor::new(matcher, dlq)
}

fn make_deduped(scope: TenantScope) -> DedupedObserverExecutor<InMemoryDedupStore> {
    DedupedObserverExecutor::new_with_scope(make_executor(), InMemoryDedupStore::new(300), scope)
}

/// `Unrestricted` passes events with any tenant (including None).
#[tokio::test]
async fn test_tenant_unrestricted_allows_all() {
    let deduped = make_deduped(TenantScope::Unrestricted);

    let event_no_tenant =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
    let summary = deduped.process_event(&event_no_tenant).await.unwrap();
    assert!(!summary.tenant_rejected);

    let event_with_tenant =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("any-tenant");
    let summary2 = deduped.process_event(&event_with_tenant).await.unwrap();
    assert!(!summary2.tenant_rejected);
}

/// `Single` rejects an event whose `tenant_id` does not match.
#[tokio::test]
async fn test_tenant_single_rejects_wrong_tenant() {
    let deduped = make_deduped(TenantScope::Single("acme".to_string()));

    let wrong_tenant =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("other");
    let summary = deduped.process_event(&wrong_tenant).await.unwrap();
    assert!(summary.tenant_rejected);
    assert_eq!(summary.successful_actions, 0);
}

/// `Single` allows an event with the matching `tenant_id`.
#[tokio::test]
async fn test_tenant_single_allows_correct_tenant() {
    let deduped = make_deduped(TenantScope::Single("acme".to_string()));

    let correct_tenant =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("acme");
    let summary = deduped.process_event(&correct_tenant).await.unwrap();
    assert!(!summary.tenant_rejected);
}

/// `Single` rejects an event with no `tenant_id`.
#[tokio::test]
async fn test_tenant_single_rejects_no_tenant() {
    let deduped = make_deduped(TenantScope::Single("acme".to_string()));

    let no_tenant =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
    let summary = deduped.process_event(&no_tenant).await.unwrap();
    assert!(summary.tenant_rejected);
}

/// `AllowList` accepts a tenant in the list.
#[tokio::test]
async fn test_tenant_allowlist_accepts_listed_tenant() {
    let deduped =
        make_deduped(TenantScope::AllowList(vec!["acme".to_string(), "globex".to_string()]));

    let event =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("globex");
    let summary = deduped.process_event(&event).await.unwrap();
    assert!(!summary.tenant_rejected);
}

/// `AllowList` rejects a tenant not in the list.
#[tokio::test]
async fn test_tenant_allowlist_rejects_unlisted_tenant() {
    let deduped =
        make_deduped(TenantScope::AllowList(vec!["acme".to_string(), "globex".to_string()]));

    let event =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("umbrella");
    let summary = deduped.process_event(&event).await.unwrap();
    assert!(summary.tenant_rejected);
}

/// `AllowList` rejects an event with no `tenant_id`.
#[tokio::test]
async fn test_tenant_allowlist_rejects_no_tenant() {
    let deduped = make_deduped(TenantScope::AllowList(vec!["acme".to_string()]));

    let event =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
    let summary = deduped.process_event(&event).await.unwrap();
    assert!(summary.tenant_rejected);
}

/// A tenant-rejected event must not be deduplicated — it never reaches the
/// dedup store — so the same event can still be processed later if the scope
/// is changed or if the event is re-routed.
#[tokio::test]
async fn test_tenant_rejection_does_not_claim_dedup_key() {
    let dedup_store = InMemoryDedupStore::new(300);
    let deduped = DedupedObserverExecutor::new_with_scope(
        make_executor(),
        dedup_store,
        TenantScope::Single("acme".to_string()),
    );

    let event =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
            .with_tenant_id("other");
    let key = format!("event:{}", event.id);

    let summary = deduped.process_event(&event).await.unwrap();
    assert!(summary.tenant_rejected);

    // Dedup store must not hold a claim for the rejected event.
    assert!(
        !deduped.dedup_store().is_duplicate(&key).await.unwrap(),
        "rejected event must not occupy the dedup slot"
    );
}

/// `tenant_scope()` accessor returns the configured scope.
#[test]
fn test_tenant_scope_accessor() {
    let deduped = make_deduped(TenantScope::Single("acme".to_string()));
    assert!(matches!(deduped.tenant_scope(), TenantScope::Single(s) if s == "acme"));
}
