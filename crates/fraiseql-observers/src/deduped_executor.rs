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

    /// Process event with deduplication check.
    ///
    /// # Flow
    ///
    /// 1. Generate dedup key from event.id (UUIDv4)
    /// 2. Check if already processed (within time window)
    /// 3. If duplicate → return early with `duplicate_skipped=true`
    /// 4. If new → process event
    /// 5. If all actions succeeded → mark as processed
    /// 6. If any action failed → don't mark (allow retry)
    ///
    /// # Arguments
    ///
    /// * `event` - The entity event to process
    ///
    /// # Returns
    ///
    /// `ExecutionSummary` with `duplicate_skipped=true` if event was duplicate,
    /// otherwise summary from inner executor.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Deduplication store check fails
    /// - Inner executor fails
    /// - Marking as processed fails
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        // Generate deduplication key from event.id (UUIDv4)
        let event_key = format!("event:{}", event.id);

        // Check if already processed
        match self.dedup_store.is_duplicate(&event_key).await {
            Ok(true) => {
                // Event is duplicate - skip processing
                debug!(
                    "Event {} is duplicate (within {}-second window), skipping",
                    event.id,
                    self.dedup_store.window_seconds()
                );

                // Record deduplication metrics
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
            },
            Ok(false) => {
                // Event is new - proceed with processing
                debug!("Event {} is new (not in dedup store), processing", event.id);
            },
            Err(e) => {
                // Deduplication check failed - log warning and process anyway
                // (fail-open: better to process duplicate than miss event)
                warn!(
                    "Deduplication check failed for event {}: {}. Processing anyway (fail-open).",
                    event.id, e
                );
            },
        }

        // Process event (not a duplicate or check failed)
        let summary = self.inner.process_event(event).await?;

        // Mark as processed (only if all actions succeeded)
        if summary.failed_actions == 0 && summary.dlq_errors == 0 {
            match self.dedup_store.mark_processed(&event_key).await {
                Ok(()) => {
                    debug!(
                        "Marked event {} as processed (TTL: {} seconds)",
                        event.id,
                        self.dedup_store.window_seconds()
                    );
                },
                Err(e) => {
                    // Failed to mark as processed - log warning but don't fail the request
                    // (event was processed successfully, just couldn't record it)
                    warn!(
                        "Failed to mark event {} as processed: {}. Event executed successfully but may be reprocessed if delivered again.",
                        event.id, e
                    );
                },
            }
        } else {
            warn!(
                "Event {} had {} failed actions and {} DLQ errors. NOT marking as processed (will allow retry).",
                event.id, summary.failed_actions, summary.dlq_errors
            );
        }

        Ok(summary)
    }

    /// Get reference to inner executor.
    pub fn inner(&self) -> &ObserverExecutor {
        &self.inner
    }

    /// Get reference to deduplication store.
    pub fn dedup_store(&self) -> &D {
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

    // Simple in-memory dedup store for testing
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

        // Should be able to access inner executor
        let _inner = deduped.inner();
        let _store = deduped.dedup_store();
    }
}
