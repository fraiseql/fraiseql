//! Cached action executor wrapper for performance optimization.
//!
//! This module provides a wrapper around `ActionExecutor` that caches successful
//! action results in Redis, dramatically reducing latency for repeated actions.
//!
//! # Problem Solved
//!
//! Without caching:
//! - Every webhook call takes 100-500ms (network latency)
//! - Email sending takes 200-1000ms (SMTP handshake)
//! - Search indexing takes 50-200ms (HTTP roundtrip)
//! - Repeated actions for same event waste resources
//!
//! With caching:
//! - Cache hit: <1ms (Redis lookup)
//! - Cache miss: Normal execution + store result
//! - 100x performance improvement for cache hits
//!
//! # Architecture
//!
//! ```text
//! Action execution request
//!     ↓
//! Generate cache key (event.id + action hash)
//!     ↓
//! Check Redis cache
//!     ├─ HIT → Return cached ActionResult (<1ms)
//!     └─ MISS → Execute action
//!         ↓
//!     Store result in Redis (TTL = 60s)
//!         ↓
//!     Return ActionResult
//! ```
//!
//! # Cache Key Design
//!
//! Cache key = `action_result:{event.id}:{action_hash}`
//!
//! - `event.id`: UUIDv4 (globally unique)
//! - `action_hash`: Hash of action config (type + params)
//!
//! This ensures:
//! - Same event + same action → cached
//! - Different events → not cached
//! - Same event + different action params → not cached
//!
//! # TTL Strategy
//!
//! - Default: 60 seconds (configurable)
//! - Only cache successful results (don't cache failures)
//! - TTL automatically expires old results
//! - Zero manual cleanup needed
//!
//! # Example
//!
//! ```rust,ignore
//! use fraiseql_observers::cached_executor::CachedActionExecutor;
//! use fraiseql_observers::actions::WebhookAction;
//! use fraiseql_observers::cache::redis::RedisCacheBackend;
//!
//! // Create cache backend
//! let cache = RedisCacheBackend::new("redis://localhost:6379");
//!
//! // Wrap action executor with cache
//! let webhook = WebhookAction::new();
//! let cached_webhook = CachedActionExecutor::new(webhook, cache);
//!
//! // Execute action (checks cache first)
//! let result = cached_webhook.execute(&event, &action).await?;
//! ```

use std::sync::Arc;

use tracing::{debug, warn};

#[cfg(feature = "caching")]
use crate::cache::{CacheBackend, CachedActionResult};
#[cfg(feature = "metrics")]
use crate::metrics::MetricsRegistry;
use crate::{
    config::ActionConfig,
    error::Result,
    event::EntityEvent,
    traits::{ActionExecutor, ActionResult},
};

/// `ActionExecutor` wrapper with caching support.
///
/// Checks cache before executing action, stores successful results after execution.
///
/// # Performance
///
/// - **Cache hit**: <1ms (cache lookup)
/// - **Cache miss**: Normal execution time + ~1ms (store result)
/// - **Expected hit rate**: 60-80% for typical workflows
///
/// # Composability
///
/// Can be composed with other wrappers:
///
/// ```rust,ignore
/// // Concurrent + Cached composition
/// let webhook = WebhookAction::new();
/// let cached = CachedActionExecutor::new(webhook, cache);
/// let concurrent = ConcurrentActionExecutor::new(cached, 30000);
///
/// // Result: Parallel execution with cache checking
/// ```
#[cfg(feature = "caching")]
pub struct CachedActionExecutor<E: ActionExecutor, C: CacheBackend> {
    /// Inner action executor
    inner:   E,
    /// Cache backend
    cache:   Arc<C>,
    /// Prometheus metrics registry
    #[cfg(feature = "metrics")]
    metrics: MetricsRegistry,
}

#[cfg(feature = "caching")]
impl<E: ActionExecutor, C: CacheBackend> CachedActionExecutor<E, C> {
    /// Create a new cached executor wrapper.
    ///
    /// # Arguments
    ///
    /// * `executor` - The underlying action executor
    /// * `cache` - Cache backend implementation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let webhook = WebhookAction::new();
    /// let cache = RedisCacheBackend::new("redis://localhost:6379");
    /// let cached = CachedActionExecutor::new(webhook, cache);
    /// ```
    pub fn new(executor: E, cache: C) -> Self {
        Self {
            inner: executor,
            cache: Arc::new(cache),
            #[cfg(feature = "metrics")]
            metrics: MetricsRegistry::global().unwrap_or_default(),
        }
    }

    /// Generate cache key from event and action.
    ///
    /// Format: `action_result:{event.id}:{action_hash}`
    ///
    /// Uses Debug representation of action for hashing (includes all params).
    fn cache_key(event: &EntityEvent, action: &ActionConfig) -> String {
        // Use Debug repr for stable hash of action params
        let action_repr = format!("{action:?}");
        // Simple hash: just concatenate (could use SHA256 for shorter keys)
        format!("action_result:{}:{action_repr}", event.id)
    }
}

#[cfg(feature = "caching")]
#[async_trait::async_trait]
impl<E: ActionExecutor + Send + Sync, C: CacheBackend + Send + Sync> ActionExecutor
    for CachedActionExecutor<E, C>
{
    async fn execute(&self, event: &EntityEvent, action: &ActionConfig) -> Result<ActionResult> {
        let cache_key = Self::cache_key(event, action);

        // Check cache first
        match self.cache.get(&cache_key).await {
            Ok(Some(cached_result)) => {
                // Cache hit - convert to ActionResult
                debug!("Cache HIT for action key: {}", cache_key);
                #[cfg(feature = "metrics")]
                self.metrics.cache_hit();

                return Ok(ActionResult {
                    action_type: cached_result.action_type,
                    success:     cached_result.success,
                    message:     cached_result.message,
                    duration_ms: cached_result.duration_ms,
                });
            },
            Ok(None) => {
                // Cache miss - execute action
                debug!("Cache MISS for action key: {}", cache_key);
                #[cfg(feature = "metrics")]
                self.metrics.cache_miss();
            },
            Err(e) => {
                // Cache check failed - log warning and execute anyway (fail-open)
                warn!(
                    "Cache check failed for key {}: {}. Executing action (fail-open).",
                    cache_key, e
                );
                // Still record as a cache miss since we couldn't use the cache
                #[cfg(feature = "metrics")]
                self.metrics.cache_miss();
            },
        }

        // Cache miss or error - execute action
        let result = self.inner.execute(event, action).await?;

        // Store in cache (only cache successful results)
        if result.success {
            let cached_result = CachedActionResult::new(
                result.action_type.clone(),
                result.success,
                result.message.clone(),
                result.duration_ms,
            );

            match self.cache.set(&cache_key, &cached_result).await {
                Ok(()) => {
                    debug!(
                        "Cached action result for key {} (TTL: {}s)",
                        cache_key,
                        self.cache.ttl_seconds()
                    );
                },
                Err(e) => {
                    warn!("Failed to cache action result: {}. Result not cached.", e);
                },
            }
        } else {
            debug!("Not caching failed action result for key {}", cache_key);
        }

        Ok(result)
    }
}

#[cfg(all(test, feature = "caching"))]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::event::EventKind;

    // Simple mock executor for testing
    #[derive(Clone)]
    struct TestExecutor {
        call_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl TestExecutor {
        fn new() -> Self {
            Self {
                call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ActionExecutor for TestExecutor {
        async fn execute(
            &self,
            _event: &EntityEvent,
            _action: &ActionConfig,
        ) -> Result<ActionResult> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(ActionResult {
                action_type: "test".to_string(),
                success:     true,
                message:     "Test success".to_string(),
                duration_ms: 10.0,
            })
        }
    }

    // Simple in-memory cache for testing
    #[derive(Clone)]
    struct InMemoryCache {
        store:       Arc<dashmap::DashMap<String, CachedActionResult>>,
        ttl_seconds: Arc<std::sync::atomic::AtomicU64>,
    }

    impl InMemoryCache {
        fn new() -> Self {
            Self {
                store:       Arc::new(dashmap::DashMap::new()),
                ttl_seconds: Arc::new(std::sync::atomic::AtomicU64::new(60)),
            }
        }
    }

    #[async_trait::async_trait]
    impl CacheBackend for InMemoryCache {
        async fn get(&self, cache_key: &str) -> Result<Option<CachedActionResult>> {
            Ok(self.store.get(cache_key).map(|entry| entry.value().clone()))
        }

        async fn set(&self, cache_key: &str, result: &CachedActionResult) -> Result<()> {
            self.store.insert(cache_key.to_string(), result.clone());
            Ok(())
        }

        fn ttl_seconds(&self) -> u64 {
            self.ttl_seconds.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn set_ttl_seconds(&mut self, seconds: u64) {
            self.ttl_seconds.store(seconds, std::sync::atomic::Ordering::Relaxed);
        }

        async fn invalidate(&self, cache_key: &str) -> Result<()> {
            self.store.remove(cache_key);
            Ok(())
        }

        async fn clear_all(&self) -> Result<()> {
            self.store.clear();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_cache_hit_does_not_execute_action() {
        let executor = TestExecutor::new();
        let cache = InMemoryCache::new();

        // Pre-populate cache
        let event = EntityEvent::new(
            EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(),
            json!({"name": "test"}),
        );

        let action = ActionConfig::Email {
            to:               Some("test@example.com".to_string()),
            to_template:      None,
            subject:          Some("Test".to_string()),
            subject_template: None,
            body_template:    Some("Test body".to_string()),
            reply_to:         None,
        };

        let cache_key =
            CachedActionExecutor::<TestExecutor, InMemoryCache>::cache_key(&event, &action);
        let cached_result =
            CachedActionResult::new("cached".to_string(), true, "Cached result".to_string(), 1.0);

        cache.set(&cache_key, &cached_result).await.unwrap();

        // Create cached executor
        let cached_executor = CachedActionExecutor::new(executor.clone(), cache);

        // Execute - should return cached result without calling inner executor
        let result = cached_executor.execute(&event, &action).await.unwrap();

        assert_eq!(result.action_type, "cached");
        assert!(result.success);
        assert_eq!(executor.call_count(), 0); // Inner executor NOT called
    }

    #[test]
    fn test_cache_key_generation() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(), // entity_id (not event.id)
            json!({}),
        );

        let action = ActionConfig::Webhook {
            url:           Some("https://example.com".to_string()),
            url_env:       None,
            headers:       std::collections::HashMap::new(),
            body_template: Some("{}".to_string()),
        };

        let key = CachedActionExecutor::<TestExecutor, InMemoryCache>::cache_key(&event, &action);

        // Cache key format: action_result:{event_id}:{action_debug}
        // Verify key contains the actual event.id (auto-generated)
        let expected_event_id = event.id.to_string();
        assert!(
            key.contains(&expected_event_id),
            "Key should contain event ID {expected_event_id}"
        );
        assert!(key.contains("Webhook"), "Key should contain action type");
        assert!(key.starts_with("action_result:"), "Key should start with action_result:");
    }

    #[tokio::test]
    async fn test_cache_miss_executes_and_caches() {
        let executor = TestExecutor::new();
        let cache = InMemoryCache::new();
        let cached_executor = CachedActionExecutor::new(executor.clone(), cache.clone());

        let event = EntityEvent::new(
            EventKind::Created,
            "Test".to_string(),
            Uuid::new_v4(),
            json!({"name": "test"}),
        );

        let action = ActionConfig::Email {
            to:               Some("test@example.com".to_string()),
            to_template:      None,
            subject:          Some("Test".to_string()),
            subject_template: None,
            body_template:    Some("Test body".to_string()),
            reply_to:         None,
        };

        // First execution - cache miss
        let result1 = cached_executor.execute(&event, &action).await.unwrap();
        assert!(result1.success);
        assert_eq!(executor.call_count(), 1); // Inner executor called

        // Second execution - cache hit
        let result2 = cached_executor.execute(&event, &action).await.unwrap();
        assert!(result2.success);
        assert_eq!(executor.call_count(), 1); // Inner executor NOT called again
    }
}
