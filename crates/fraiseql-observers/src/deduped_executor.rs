//! Deduplication wrapper around `ObserverExecutor`.
//!
//! This module provides a wrapper that prevents duplicate processing of events
//! by checking a deduplication store before delegating to the inner executor.
//!
//! # Problem Solved
//!
//! With at-least-once delivery (NATS, retries), the same event may arrive multiple times.
//! Without deduplication:
//! - Same webhook fired twice
//! - Duplicate emails sent
//! - Duplicate charges created
//!
//! With deduplication:
//! - First occurrence processed
//! - Duplicates within time window silently skipped
//! - No duplicate side effects
//!
//! # Architecture
//!
//! ```text
//! Event arrives
//!     ↓
//! Check Redis: is event.id processed? (UUIDv4)
//!     ↓
//! If YES → Skip (return early with duplicate_skipped=true)
//! If NO  → Process event
//!     ↓
//! If all actions succeeded → Mark event.id as processed (TTL = 5 min)
//! If any action failed → Don't mark (allow retry)
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use fraiseql_observers::executor::ObserverExecutor;
//! use fraiseql_observers::deduped_executor::DedupedObserverExecutor;
//! use fraiseql_observers::dedup::redis::RedisDeduplicationStore;
//!
//! // Create inner executor
//! let executor = ObserverExecutor::new(matcher, dlq);
//!
//! // Wrap with deduplication
//! let dedup_store = RedisDeduplicationStore::new("redis://localhost:6379", 300);
//! let deduped = DedupedObserverExecutor::new(executor, dedup_store);
//!
//! // Process event (automatically deduplicated)
//! let summary = deduped.process_event(&event).await?;
//! if summary.duplicate_skipped {
//!     println!("Event was duplicate, skipped");
//! }
//! ```

use std::sync::Arc;

use tracing::{debug, warn};

#[cfg(feature = "dedup")]
use crate::dedup::DeduplicationStore;
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{
    error::Result,
    event::EntityEvent,
    executor::{ExecutionSummary, ObserverExecutor},
};

/// `ObserverExecutor` wrapper with deduplication support.
///
/// This wrapper prevents duplicate processing of events by checking
/// a deduplication store (typically Redis-backed) before delegating
/// to the inner executor.
///
/// # Key Decisions
///
/// - **Dedup key**: event.id (UUIDv4, globally unique across all transports)
/// - **Check timing**: Before processing (early return for duplicates)
/// - **Mark timing**: After successful processing (only if all actions succeeded)
/// - **TTL**: Configurable window (default 5 minutes)
///
/// # Guarantees
///
/// - ✅ Prevents duplicate processing within time window
/// - ✅ Allows retry on failure (don't mark if actions failed)
/// - ✅ At-least-once execution preserved
/// - ✅ Dead Letter Queue handles permanent failures
#[cfg(feature = "dedup")]
pub struct DedupedObserverExecutor<D: DeduplicationStore> {
    /// Inner executor that performs actual event processing
    inner:       Arc<ObserverExecutor>,
    /// Deduplication store (typically Redis-backed)
    dedup_store: D,
    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics:     MetricsRegistry,
}

#[cfg(feature = "dedup")]
impl<D: DeduplicationStore> DedupedObserverExecutor<D> {
    /// Create a new deduplication wrapper.
    ///
    /// # Arguments
    ///
    /// * `executor` - The underlying `ObserverExecutor`
    /// * `dedup_store` - Deduplication store implementation (e.g., `RedisDeduplicationStore`)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let executor = ObserverExecutor::new(matcher, dlq);
    /// let dedup_store = RedisDeduplicationStore::new("redis://localhost:6379", 300);
    /// let deduped = DedupedObserverExecutor::new(executor, dedup_store);
    /// ```
    pub fn new(executor: ObserverExecutor, dedup_store: D) -> Self {
        Self {
            inner: Arc::new(executor),
            dedup_store,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Process event with atomic deduplication.
    ///
    /// # Flow
    ///
    /// 1. Generate dedup key from `event.id` (UUIDv4)
    /// 2. `claim_event()` — atomic `SET NX EX`: only one worker wins the claim
    /// 3. If not claimed (duplicate) → return early with `duplicate_skipped=true`
    /// 4. If claimed → process event
    /// 5. If processing failed → `remove()` the claim key so the event can be retried
    ///
    /// Using a single atomic claim eliminates the race condition that existed when
    /// `is_duplicate` and `mark_processed` were called as separate operations.
    ///
    /// # Errors
    ///
    /// Returns error if the inner executor fails. A failed `claim_event` causes
    /// fail-open behaviour (event is processed anyway) to avoid silent event loss.
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        let event_key = format!("event:{}", event.id);

        // Atomically claim the event; fail-open if the store is unavailable.
        let claimed = match self.dedup_store.claim_event(&event_key).await {
            Ok(claimed) => claimed,
            Err(e) => {
                warn!(
                    "Dedup claim failed for event {}: {}. Processing anyway (fail-open).",
                    event.id, e
                );
                true // treat as claimed so we proceed
            },
        };

        if !claimed {
            debug!(
                "Event {} already claimed (within {}-second window), skipping",
                event.id,
                self.dedup_store.window_seconds()
            );

            #[cfg(feature = "metrics")]
            {
                self.metrics.dedup_detected();
                self.metrics.dedup_processing_skipped();
            }

            return Ok(ExecutionSummary {
                successful_actions: 0,
                failed_actions:     0,
                conditions_skipped: 0,
                total_duration_ms:  0.0,
                dlq_errors:         0,
                errors:             Vec::new(),
                duplicate_skipped:  true,
                cache_hits:         0,
                cache_misses:       0,
            });
        }

        debug!("Event {} claimed, processing", event.id);
        let summary = self.inner.process_event(event).await?;

        // Un-claim on failure so the event can be retried.
        if summary.failed_actions > 0 || summary.dlq_errors > 0 {
            warn!(
                "Event {} had {} failed actions and {} DLQ errors — un-claiming for retry.",
                event.id, summary.failed_actions, summary.dlq_errors
            );
            if let Err(e) = self.dedup_store.remove(&event_key).await {
                warn!("Failed to un-claim event {} after failure: {}", event.id, e);
            }
        }

        Ok(summary)
    }

    /// Get reference to inner executor.
    pub fn inner(&self) -> &ObserverExecutor {
        &self.inner
    }

    /// Get reference to deduplication store.
    pub const fn dedup_store(&self) -> &D {
        &self.dedup_store
    }

    /// Get deduplication window in seconds.
    pub fn window_seconds(&self) -> u64 {
        self.dedup_store.window_seconds()
    }
}

#[cfg(all(test, feature = "dedup"))]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::{event::EventKind, matcher::EventMatcher, testing::mocks::MockDeadLetterQueue};

    // Simple in-memory dedup store for testing.
    // `claim_event` uses DashMap's vacant-entry insertion which is atomic at
    // the shard level, matching the SET NX semantics of the Redis backend.
    #[derive(Clone)]
    struct InMemoryDedupStore {
        store:          Arc<dashmap::DashMap<String, bool>>,
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
}
