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
//! Validate tenant_id against TenantScope
//!     ↓
//! If VIOLATION → push raw bytes to DLQ, increment counter, return early
//! If PASS      → continue
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
//! use fraiseql_observers::deduped_executor::{DedupedObserverExecutor, TenantScope};
//! use fraiseql_observers::dedup::redis::RedisDeduplicationStore;
//!
//! // Create inner executor
//! let executor = ObserverExecutor::new(matcher, dlq);
//!
//! // Wrap with deduplication + tenant isolation
//! let dedup_store = RedisDeduplicationStore::new("redis://localhost:6379", 300);
//! let deduped = DedupedObserverExecutor::new_with_scope(
//!     executor,
//!     dedup_store,
//!     TenantScope::Single("acme".to_string()),
//! );
//!
//! // Process event (automatically deduplicated + tenant-validated)
//! let summary = deduped.process_event(&event).await?;
//! if summary.duplicate_skipped {
//!     println!("Event was duplicate, skipped");
//! }
//! if summary.tenant_rejected {
//!     println!("Event was from a different tenant, routed to DLQ");
//! }
//! ```

use std::sync::Arc;

use tracing::{debug, error, warn};

/// Tenant scope configuration for `DedupedObserverExecutor`.
///
/// Controls which `tenant_id` values are permitted to be processed.
/// Events whose `tenant_id` does not satisfy the configured scope are
/// rejected before deduplication: their serialized payload is routed
/// to the dead letter queue and a Prometheus counter is incremented.
///
/// # Startup behaviour
///
/// `TenantScope::Unrestricted` is the default and accepts events from
/// **all** tenants (including events with no `tenant_id`). Because this
/// allows cross-tenant data to flow through a single executor — which is
/// usually undesirable in production — a `tracing::warn!` is emitted
/// when the executor is constructed with `Unrestricted`.
#[derive(Debug, Clone)]
pub enum TenantScope {
    /// Accept every event regardless of `tenant_id`.
    ///
    /// **Warning**: emits a startup log warning. Prefer `Single` or
    /// `AllowList` in multi-tenant deployments.
    Unrestricted,

    /// Accept only events whose `tenant_id` exactly matches the given value.
    ///
    /// Events with `tenant_id = None` are rejected when this variant is active.
    Single(String),

    /// Accept events whose `tenant_id` is one of the listed values.
    ///
    /// Events with `tenant_id = None` are rejected when this variant is active.
    AllowList(Vec<String>),
}

#[cfg(feature = "dedup")]
use crate::dedup::DeduplicationStore;
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{
    error::Result,
    event::EntityEvent,
    executor::{ExecutionSummary, ObserverExecutor},
};

/// `ObserverExecutor` wrapper with deduplication support and tenant boundary enforcement.
///
/// This wrapper prevents duplicate processing of events by checking
/// a deduplication store (typically Redis-backed) before delegating
/// to the inner executor.  It also enforces a [`TenantScope`] policy:
/// events whose `tenant_id` does not satisfy the configured scope are
/// rejected before they reach the dedup check or the inner executor.
///
/// # Key Decisions
///
/// - **Tenant check**: Before dedup — violated events never touch the dedup store
/// - **Dedup key**: event.id (UUIDv4, globally unique across all transports)
/// - **Check timing**: Before processing (early return for duplicates)
/// - **Mark timing**: After successful processing (only if all actions succeeded)
/// - **TTL**: Configurable window (default 5 minutes)
///
/// # Guarantees
///
/// - ✅ Tenant boundary enforced before any processing
/// - ✅ Prevents duplicate processing within time window
/// - ✅ Allows retry on failure (don't mark if actions failed)
/// - ✅ At-least-once execution preserved
/// - ✅ Dead Letter Queue handles permanent failures
#[cfg(feature = "dedup")]
pub struct DedupedObserverExecutor<D: DeduplicationStore> {
    /// Inner executor that performs actual event processing
    inner:         Arc<ObserverExecutor>,
    /// Deduplication store (typically Redis-backed)
    dedup_store:   D,
    /// Tenant scope policy applied before dedup check
    tenant_scope:  TenantScope,
    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics:       MetricsRegistry,
}

#[cfg(feature = "dedup")]
impl<D: DeduplicationStore> DedupedObserverExecutor<D> {
    /// Create a new deduplication wrapper with `TenantScope::Unrestricted`.
    ///
    /// Emits a `tracing::warn!` at construction time because `Unrestricted`
    /// allows cross-tenant event flow, which is usually undesirable in
    /// multi-tenant deployments.  Prefer [`new_with_scope`](Self::new_with_scope)
    /// to set an explicit scope.
    ///
    /// # Arguments
    ///
    /// * `executor` - The underlying `ObserverExecutor`
    /// * `dedup_store` - Deduplication store implementation (e.g., `RedisDeduplicationStore`)
    pub fn new(executor: ObserverExecutor, dedup_store: D) -> Self {
        Self::new_with_scope(executor, dedup_store, TenantScope::Unrestricted)
    }

    /// Create a new deduplication wrapper with an explicit tenant scope.
    ///
    /// A `tracing::warn!` is emitted when `tenant_scope` is
    /// [`TenantScope::Unrestricted`].
    ///
    /// # Arguments
    ///
    /// * `executor` - The underlying `ObserverExecutor`
    /// * `dedup_store` - Deduplication store implementation (e.g., `RedisDeduplicationStore`)
    /// * `tenant_scope` - Tenant boundary policy (see [`TenantScope`])
    pub fn new_with_scope(executor: ObserverExecutor, dedup_store: D, tenant_scope: TenantScope) -> Self {
        match &tenant_scope {
            TenantScope::Unrestricted => {
                warn!(
                    "DedupedObserverExecutor configured with TenantScope::Unrestricted — \
                     events from ALL tenants (including those without a tenant_id) will be \
                     processed. Consider using TenantScope::Single or TenantScope::AllowList \
                     in multi-tenant deployments."
                );
            },
            TenantScope::AllowList(ids) if ids.is_empty() => {
                warn!(
                    "DedupedObserverExecutor configured with an empty TenantScope::AllowList \
                     — every event will be rejected as a tenant violation. \
                     Add at least one tenant ID to the allow list."
                );
            },
            _ => {},
        }
        Self {
            inner: Arc::new(executor),
            dedup_store,
            tenant_scope,
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Returns `true` if `event.tenant_id` satisfies the configured [`TenantScope`].
    fn tenant_allowed(&self, event_tenant: Option<&str>) -> bool {
        match &self.tenant_scope {
            TenantScope::Unrestricted => true,
            TenantScope::Single(required) => event_tenant == Some(required.as_str()),
            TenantScope::AllowList(allowed) => {
                event_tenant.is_some_and(|t| allowed.iter().any(|a| a == t))
            },
        }
    }

    /// Human-readable representation of the current scope (for error messages).
    fn scope_description(&self) -> String {
        match &self.tenant_scope {
            TenantScope::Unrestricted => "Unrestricted".to_string(),
            TenantScope::Single(id) => format!("Single({id})"),
            TenantScope::AllowList(ids) => format!("AllowList([{}])", ids.join(", ")),
        }
    }

    /// Process event with tenant validation and atomic deduplication.
    ///
    /// # Flow
    ///
    /// 1. Validate `event.tenant_id` against the configured [`TenantScope`]
    ///    - Violation → serialize event, push raw bytes to DLQ, increment metric,
    ///      return `Ok(summary { tenant_rejected: true })`
    /// 2. Generate dedup key from `event.id` (UUIDv4)
    /// 3. `claim_event()` — atomic `SET NX EX`: only one worker wins the claim
    /// 4. If not claimed (duplicate) → return early with `duplicate_skipped=true`
    /// 5. If claimed → process event
    /// 6. If processing failed → `remove()` the claim key so the event can be retried
    ///
    /// Using a single atomic claim eliminates the race condition that existed when
    /// `is_duplicate` and `mark_processed` were called as separate operations.
    ///
    /// # Errors
    ///
    /// Returns error if the inner executor fails. A failed `claim_event` causes
    /// fail-open behaviour (event is processed anyway) to avoid silent event loss.
    pub async fn process_event(&self, event: &EntityEvent) -> Result<ExecutionSummary> {
        // --- Tenant boundary check (must happen before dedup claim) ---
        if !self.tenant_allowed(event.tenant_id.as_deref()) {
            let scope = self.scope_description();
            let violation = crate::error::ObserverError::TenantViolation {
                event_tenant:   event.tenant_id.clone(),
                required_scope: scope.clone(),
            };
            error!(
                event_id = %event.id,
                event_tenant = ?event.tenant_id,
                required_scope = %scope,
                "Tenant violation — routing event to DLQ"
            );
            #[cfg(feature = "metrics")]
            self.metrics.tenant_violation();

            // Serialize the event to raw bytes so the DLQ preserves the payload.
            let raw = serde_json::to_vec(event).unwrap_or_default();
            let reason = violation.to_string();
            if let Err(dlq_err) = self.inner.dlq().push_raw(&raw, &reason).await {
                error!("Failed to route tenant-violation event to DLQ: {}", dlq_err);
            }

            return Ok(ExecutionSummary {
                tenant_rejected: true,
                ..ExecutionSummary::new()
            });
        }

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
                tenant_rejected:    false,
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

    /// Get a reference to the configured tenant scope.
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
}

#[cfg(all(test, feature = "dedup"))]
#[allow(clippy::unwrap_used)] // Reason: test code
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

        let event_no_tenant = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({}),
        );
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
        let deduped = make_deduped(TenantScope::AllowList(vec![
            "acme".to_string(),
            "globex".to_string(),
        ]));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_tenant_id("globex");
        let summary = deduped.process_event(&event).await.unwrap();
        assert!(!summary.tenant_rejected);
    }

    /// `AllowList` rejects a tenant not in the list.
    #[tokio::test]
    async fn test_tenant_allowlist_rejects_unlisted_tenant() {
        let deduped = make_deduped(TenantScope::AllowList(vec![
            "acme".to_string(),
            "globex".to_string(),
        ]));

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
}
