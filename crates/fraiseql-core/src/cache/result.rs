//! Query result caching with LRU eviction and TTL expiry.
//!
//! This module provides thread-safe in-memory caching for GraphQL query results
//! with automatic least-recently-used (LRU) eviction and time-to-live (TTL) expiry.

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::types::JsonbValue;
use crate::error::{FraiseQLError, Result};
use super::config::CacheConfig;

/// Cached query result with metadata.
///
/// Stores the query result along with tracking information for
/// TTL expiry, view-based invalidation, and monitoring.
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// The actual query result (JSONB array from database).
    ///
    /// Wrapped in `Arc` for cheap cloning on cache hits (zero-copy).
    pub result: Arc<Vec<JsonbValue>>,

    /// Which views/tables this query accesses.
    ///
    /// Format: `vec!["v_user", "v_post"]`
    ///
    /// Used for view-based invalidation when mutations modify these views.
    pub accessed_views: Vec<String>,

    /// When this entry was cached (Unix timestamp in seconds).
    ///
    /// Used for TTL expiry check on access.
    pub cached_at: u64,

    /// Number of cache hits for this entry.
    ///
    /// Used for monitoring and optimization. Incremented on each `get()`.
    pub hit_count: u64,
}

/// Thread-safe LRU cache for query results.
///
/// # Thread Safety
///
/// All operations use interior mutability (`Arc<Mutex<>>`), making the cache
/// safe to share across async tasks. Lock contention is minimal as critical
/// sections are short.
///
/// # Memory Safety
///
/// - **Hard LRU limit**: Configured via `max_entries`, automatically evicts
///   least-recently-used entries when limit is reached
/// - **TTL expiry**: Entries older than `ttl_seconds` are considered expired
///   and removed on next access
/// - **Memory tracking**: Metrics include estimated memory usage
///
/// # Example
///
/// ```rust
/// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
/// use fraiseql_core::db::types::JsonbValue;
/// use serde_json::json;
///
/// let cache = QueryResultCache::new(CacheConfig::default());
///
/// // Cache a result
/// let result = vec![JsonbValue::new(json!({"id": 1, "name": "Alice"}))];
/// cache.put(
///     "cache_key_123".to_string(),
///     result.clone(),
///     vec!["v_user".to_string()]
/// ).unwrap();
///
/// // Retrieve from cache
/// if let Some(cached) = cache.get("cache_key_123").unwrap() {
///     println!("Cache hit! {} results", cached.len());
/// }
/// ```
pub struct QueryResultCache {
    /// LRU cache: key -> cached result.
    ///
    /// Automatically evicts least-recently-used entries above `max_entries`.
    cache: Arc<Mutex<LruCache<String, CachedResult>>>,

    /// Configuration (immutable after creation).
    config: CacheConfig,

    /// Metrics for monitoring.
    metrics: Arc<Mutex<CacheMetrics>>,
}

/// Cache metrics for monitoring.
///
/// Exposed via API for observability and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    /// Number of cache hits (returned cached result).
    pub hits: u64,

    /// Number of cache misses (executed query).
    pub misses: u64,

    /// Total entries cached across all time.
    pub total_cached: u64,

    /// Number of invalidations triggered.
    pub invalidations: u64,

    /// Current size of cache (number of entries).
    pub size: usize,

    /// Estimated memory usage in bytes.
    ///
    /// This is a rough estimate based on cache key lengths and entry counts.
    /// Actual memory usage may vary based on result sizes.
    pub memory_bytes: usize,
}

impl QueryResultCache {
    /// Create new cache with configuration.
    ///
    /// # Panics
    ///
    /// Panics if `config.max_entries` is 0 (invalid configuration).
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    /// ```
    #[must_use]
    pub fn new(config: CacheConfig) -> Self {
        let max = NonZeroUsize::new(config.max_entries)
            .expect("max_entries must be > 0");

        Self {
            cache: Arc::new(Mutex::new(LruCache::new(max))),
            config,
            metrics: Arc::new(Mutex::new(CacheMetrics {
                hits: 0,
                misses: 0,
                total_cached: 0,
                invalidations: 0,
                size: 0,
                memory_bytes: 0,
            })),
        }
    }

    /// Get cached result by key.
    ///
    /// Returns `None` if:
    /// - Caching is disabled (`config.enabled = false`)
    /// - Entry not in cache (cache miss)
    /// - Entry expired (TTL exceeded)
    ///
    /// # Errors
    ///
    /// Returns error if cache or metrics mutex is poisoned (should never happen).
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// if let Some(result) = cache.get("cache_key_abc123")? {
    ///     // Cache hit - use result
    ///     println!("Found {} results in cache", result.len());
    /// } else {
    ///     // Cache miss - execute query
    ///     println!("Cache miss, executing query");
    /// }
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn get(&self, cache_key: &str) -> Result<Option<Arc<Vec<JsonbValue>>>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let mut cache = self.cache.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Cache lock poisoned: {e}"),
                source: None,
            })?;

        if let Some(cached) = cache.get_mut(cache_key) {
            // Check TTL
            let now = current_timestamp();
            if now - cached.cached_at > self.config.ttl_seconds {
                // Expired - remove and return None
                cache.pop(cache_key);
                self.record_miss()?;
                return Ok(None);
            }

            // Cache hit - update stats and return
            cached.hit_count += 1;
            self.record_hit()?;
            Ok(Some(cached.result.clone()))
        } else {
            // Cache miss
            self.record_miss()?;
            Ok(None)
        }
    }

    /// Store query result in cache.
    ///
    /// If caching is disabled, this is a no-op.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (from `generate_cache_key()`)
    /// * `result` - Query result to cache
    /// * `accessed_views` - List of views accessed by this query
    ///
    /// # Errors
    ///
    /// Returns error if cache or metrics mutex is poisoned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    /// use fraiseql_core::db::types::JsonbValue;
    /// use serde_json::json;
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// let result = vec![JsonbValue::new(json!({"id": 1}))];
    /// cache.put(
    ///     "cache_key_abc123".to_string(),
    ///     result,
    ///     vec!["v_user".to_string()]
    /// )?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn put(
        &self,
        cache_key: String,
        result: Vec<JsonbValue>,
        accessed_views: Vec<String>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let now = current_timestamp();

        let cached = CachedResult {
            result: Arc::new(result),
            accessed_views,
            cached_at: now,
            hit_count: 0,
        };

        // Estimate memory usage (rough approximation)
        let memory_size = std::mem::size_of::<CachedResult>() + cache_key.len() * 2;

        let mut cache = self.cache.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Cache lock poisoned: {e}"),
                source: None,
            })?;

        cache.put(cache_key, cached);

        // Update metrics
        let mut metrics = self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })?;
        metrics.total_cached += 1;
        metrics.size = cache.len();
        metrics.memory_bytes += memory_size;

        Ok(())
    }

    /// Invalidate entries accessing specified views.
    ///
    /// Called after mutations to invalidate affected cache entries.
    ///
    /// # Phase 2 Behavior
    ///
    /// - Simple view-based invalidation
    /// - Removes all entries that read from any specified view
    ///
    /// # Future Enhancements (Phase 4+)
    ///
    /// - Entity-level invalidation (only invalidate specific IDs)
    /// - Cascade-driven invalidation (from mutation metadata)
    ///
    /// # Arguments
    ///
    /// * `views` - List of view/table names modified by mutation
    ///
    /// # Returns
    ///
    /// Number of cache entries invalidated
    ///
    /// # Errors
    ///
    /// Returns error if cache or metrics mutex is poisoned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// // After createUser mutation
    /// let invalidated = cache.invalidate_views(&vec!["v_user".to_string()])?;
    /// println!("Invalidated {} cache entries", invalidated);
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn invalidate_views(&self, views: &[String]) -> Result<u64> {
        let mut invalidated_count = 0u64;

        let mut cache = self.cache.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Cache lock poisoned: {e}"),
                source: None,
            })?;

        // Collect keys to remove (can't modify during iteration)
        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, cached)| {
                // Check if any accessed view matches invalidation list
                cached.accessed_views.iter().any(|v| views.contains(v))
            })
            .map(|(k, _)| k.clone())
            .collect();

        // Remove entries
        for key in keys_to_remove {
            cache.pop(&key);
            invalidated_count += 1;
        }

        // Update metrics
        let mut metrics = self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })?;
        metrics.invalidations += invalidated_count;
        metrics.size = cache.len();

        Ok(invalidated_count)
    }

    /// Get cache metrics.
    ///
    /// Used for monitoring and debugging.
    ///
    /// # Errors
    ///
    /// Returns error if metrics mutex is poisoned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    /// let metrics = cache.metrics()?;
    ///
    /// println!("Hit rate: {:.1}%", metrics.hit_rate() * 100.0);
    /// println!("Size: {} / {} entries", metrics.size, 10_000);
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn metrics(&self) -> Result<CacheMetrics> {
        self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })
            .map(|m| m.clone())
    }

    /// Clear all cache entries.
    ///
    /// Used for testing and manual cache flush.
    ///
    /// # Errors
    ///
    /// Returns error if cache or metrics mutex is poisoned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    /// cache.clear()?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn clear(&self) -> Result<()> {
        self.cache.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Cache lock poisoned: {e}"),
                source: None,
            })?
            .clear();

        let mut metrics = self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })?;
        metrics.size = 0;
        metrics.memory_bytes = 0;

        Ok(())
    }

    // Private helpers

    fn record_hit(&self) -> Result<()> {
        let mut metrics = self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })?;
        metrics.hits += 1;
        Ok(())
    }

    fn record_miss(&self) -> Result<()> {
        let mut metrics = self.metrics.lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Metrics lock poisoned: {e}"),
                source: None,
            })?;
        metrics.misses += 1;
        Ok(())
    }
}

impl CacheMetrics {
    /// Calculate cache hit rate.
    ///
    /// Returns ratio of hits to total requests (0.0 to 1.0).
    ///
    /// # Returns
    ///
    /// - `1.0` if all requests were hits
    /// - `0.0` if all requests were misses
    /// - `0.0` if no requests yet
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheMetrics;
    ///
    /// let metrics = CacheMetrics {
    ///     hits: 80,
    ///     misses: 20,
    ///     total_cached: 100,
    ///     invalidations: 5,
    ///     size: 95,
    ///     memory_bytes: 1_000_000,
    /// };
    ///
    /// assert_eq!(metrics.hit_rate(), 0.8);  // 80% hit rate
    /// ```
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }

    /// Check if cache is performing well.
    ///
    /// Returns `true` if hit rate is above 60% (reasonable threshold).
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheMetrics;
    ///
    /// let good_metrics = CacheMetrics {
    ///     hits: 80,
    ///     misses: 20,
    ///     total_cached: 100,
    ///     invalidations: 5,
    ///     size: 95,
    ///     memory_bytes: 1_000_000,
    /// };
    ///
    /// assert!(good_metrics.is_healthy());  // 80% > 60%
    /// ```
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.hit_rate() > 0.6
    }
}

/// Get current Unix timestamp in seconds.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper to create test result
    fn test_result() -> Vec<JsonbValue> {
        vec![JsonbValue::new(json!({"id": 1, "name": "test"}))]
    }

    // ========================================================================
    // Cache Hit/Miss Tests
    // ========================================================================

    #[test]
    fn test_cache_miss() {
        let cache = QueryResultCache::new(CacheConfig::default());

        let result = cache.get("nonexistent_key").unwrap();
        assert!(result.is_none(), "Should be cache miss");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 0);
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryResultCache::new(CacheConfig::default());
        let result = test_result();

        // Put
        cache.put("key1".to_string(), result.clone(), vec!["v_user".to_string()]).unwrap();

        // Get
        let cached = cache.get("key1").unwrap();
        assert!(cached.is_some(), "Should be cache hit");
        assert_eq!(cached.unwrap().len(), 1);

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.total_cached, 1);
    }

    #[test]
    fn test_cache_hit_updates_hit_count() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // First hit
        cache.get("key1").unwrap();
        // Second hit
        cache.get("key1").unwrap();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 2);
    }

    // ========================================================================
    // TTL Expiry Tests
    // ========================================================================

    #[test]
    fn test_ttl_expiry() {
        let mut config = CacheConfig::default();
        config.ttl_seconds = 1;  // 1 second TTL

        let cache = QueryResultCache::new(config);

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be expired
        let result = cache.get("key1").unwrap();
        assert!(result.is_none(), "Entry should be expired");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1);  // Expired counts as miss
    }

    #[test]
    fn test_ttl_not_expired() {
        let mut config = CacheConfig::default();
        config.ttl_seconds = 3600;  // 1 hour TTL

        let cache = QueryResultCache::new(config);

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Should still be valid
        let result = cache.get("key1").unwrap();
        assert!(result.is_some(), "Entry should not be expired");
    }

    // ========================================================================
    // LRU Eviction Tests
    // ========================================================================

    #[test]
    fn test_lru_eviction() {
        let mut config = CacheConfig::default();
        config.max_entries = 2;  // Only 2 entries

        let cache = QueryResultCache::new(config);

        // Add 3 entries (max is 2)
        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key2".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key3".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // key1 should be evicted (LRU)
        assert!(cache.get("key1").unwrap().is_none(), "Oldest entry should be evicted");
        assert!(cache.get("key2").unwrap().is_some());
        assert!(cache.get("key3").unwrap().is_some());

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 2, "Cache size should be at max");
    }

    #[test]
    fn test_lru_updates_on_access() {
        let mut config = CacheConfig::default();
        config.max_entries = 2;

        let cache = QueryResultCache::new(config);

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key2".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Access key1 (makes it recently used)
        cache.get("key1").unwrap();

        // Add key3 (should evict key2, not key1)
        cache.put("key3".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        assert!(cache.get("key1").unwrap().is_some(), "key1 should remain (recently used)");
        assert!(cache.get("key2").unwrap().is_none(), "key2 should be evicted (LRU)");
        assert!(cache.get("key3").unwrap().is_some());
    }

    // ========================================================================
    // Cache Disabled Tests
    // ========================================================================

    #[test]
    fn test_cache_disabled() {
        let config = CacheConfig::disabled();
        let cache = QueryResultCache::new(config);

        // Put should be no-op
        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Get should return None
        assert!(cache.get("key1").unwrap().is_none(), "Cache disabled should always miss");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.total_cached, 0);
    }

    // ========================================================================
    // Invalidation Tests
    // ========================================================================

    #[test]
    fn test_invalidate_single_view() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key2".to_string(), test_result(), vec!["v_post".to_string()]).unwrap();

        // Invalidate v_user
        let invalidated = cache.invalidate_views(&vec!["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        // v_user entry gone, v_post remains
        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_some());
    }

    #[test]
    fn test_invalidate_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key2".to_string(), test_result(), vec!["v_post".to_string()]).unwrap();
        cache.put("key3".to_string(), test_result(), vec!["v_product".to_string()]).unwrap();

        // Invalidate v_user and v_post
        let invalidated = cache.invalidate_views(&vec![
            "v_user".to_string(),
            "v_post".to_string(),
        ]).unwrap();
        assert_eq!(invalidated, 2);

        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_none());
        assert!(cache.get("key3").unwrap().is_some());
    }

    #[test]
    fn test_invalidate_entry_with_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::default());

        // Entry accesses both v_user and v_post
        cache.put(
            "key1".to_string(),
            test_result(),
            vec!["v_user".to_string(), "v_post".to_string()]
        ).unwrap();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&vec!["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        assert!(cache.get("key1").unwrap().is_none());
    }

    #[test]
    fn test_invalidate_nonexistent_view() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Invalidate view that doesn't exist
        let invalidated = cache.invalidate_views(&vec!["v_nonexistent".to_string()]).unwrap();
        assert_eq!(invalidated, 0);

        // Entry should remain
        assert!(cache.get("key1").unwrap().is_some());
    }

    // ========================================================================
    // Clear Tests
    // ========================================================================

    #[test]
    fn test_clear() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();
        cache.put("key2".to_string(), test_result(), vec!["v_post".to_string()]).unwrap();

        cache.clear().unwrap();

        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_none());

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);
    }

    // ========================================================================
    // Metrics Tests
    // ========================================================================

    #[test]
    fn test_metrics_tracking() {
        let cache = QueryResultCache::new(CacheConfig::default());

        // Miss
        cache.get("NotThere").unwrap();

        // Put
        cache.put("key1".to_string(), test_result(), vec!["v_user".to_string()]).unwrap();

        // Hit
        cache.get("key1").unwrap();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.size, 1);
        assert_eq!(metrics.total_cached, 1);
    }

    #[test]
    fn test_metrics_hit_rate() {
        let metrics = CacheMetrics {
            hits: 80,
            misses: 20,
            total_cached: 100,
            invalidations: 5,
            size: 95,
            memory_bytes: 1_000_000,
        };

        assert_eq!(metrics.hit_rate(), 0.8);
        assert!(metrics.is_healthy());
    }

    #[test]
    fn test_metrics_hit_rate_zero_requests() {
        let metrics = CacheMetrics {
            hits: 0,
            misses: 0,
            total_cached: 0,
            invalidations: 0,
            size: 0,
            memory_bytes: 0,
        };

        assert_eq!(metrics.hit_rate(), 0.0);
        assert!(!metrics.is_healthy());
    }

    #[test]
    fn test_metrics_is_healthy() {
        let good = CacheMetrics {
            hits: 70,
            misses: 30,
            total_cached: 100,
            invalidations: 5,
            size: 95,
            memory_bytes: 1_000_000,
        };
        assert!(good.is_healthy());  // 70% > 60%

        let bad = CacheMetrics {
            hits: 50,
            misses: 50,
            total_cached: 100,
            invalidations: 5,
            size: 95,
            memory_bytes: 1_000_000,
        };
        assert!(!bad.is_healthy());  // 50% < 60%
    }

    // ========================================================================
    // Thread Safety Tests
    // ========================================================================

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(QueryResultCache::new(CacheConfig::default()));

        // Spawn multiple threads accessing cache
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let cache_clone = cache.clone();
                thread::spawn(move || {
                    let key = format!("key{}", i);
                    cache_clone.put(key.clone(), test_result(), vec!["v_user".to_string()]).unwrap();
                    cache_clone.get(&key).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.total_cached, 10);
        assert_eq!(metrics.hits, 10);
    }
}
