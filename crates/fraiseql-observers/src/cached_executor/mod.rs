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
//! The key is derived by [`crate::cache::key::action_result_key`], which is the
//! single definition shared with [`crate::executor::ObserverExecutor`] — this
//! module used to carry its own, and both were wrong the same way (#1011).
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
//! ```ignore
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
/// ```ignore
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
    /// ```ignore
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
}

#[cfg(feature = "caching")]
impl<E: ActionExecutor + Send + Sync, C: CacheBackend + Send + Sync> ActionExecutor
    for CachedActionExecutor<E, C>
{
    async fn execute(&self, event: &EntityEvent, action: &ActionConfig) -> Result<ActionResult> {
        // No key means not cacheable: execute, and do not invent a key that two
        // distinct actions could share (see `cache::key`).
        let Some(cache_key) = crate::cache::key::action_result_key(event, action) else {
            warn!("Action does not render to JSON; executing uncached");
            return self.inner.execute(event, action).await;
        };

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
                    // A cache hit carries no fresh transport round-trip, so there
                    // is no HTTP status to surface (#468).
                    status_code: None,
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
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests;
