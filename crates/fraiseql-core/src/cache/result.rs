//! Query result caching with LRU eviction and TTL expiry.
//!
//! This module provides thread-safe in-memory caching for GraphQL query results
//! with automatic least-recently-used (LRU) eviction and time-to-live (TTL) expiry.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use lru::LruCache;
use serde::{Deserialize, Serialize};

use super::config::CacheConfig;
use crate::{
    db::types::JsonbValue,
    error::{FraiseQLError, Result},
    utils::clock::{Clock, SystemClock},
};

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

    /// Per-entry TTL in seconds.
    ///
    /// Overrides `CacheConfig::ttl_seconds` when set via `put(..., Some(ttl))`.
    /// Enables per-query cache lifetimes (e.g., reference data lives 1 h,
    /// live prices are never cached with `ttl = 0`).
    pub ttl_seconds: u64,

    /// Number of cache hits for this entry.
    ///
    /// Used for monitoring and optimization. Incremented on each `get()`.
    pub hit_count: u64,

    /// Entity UUID index for selective invalidation.
    ///
    /// Key: GraphQL entity type name (e.g. `"User"`).
    /// Value: set of UUID strings present in this result's rows.
    ///
    /// Built at `put()` time by scanning each row for an `"id"` field. Used by
    /// `invalidate_by_entity()` to evict only the entries that actually contain
    /// a specific entity, leaving unrelated entries warm.
    pub entity_ids: HashMap<String, HashSet<String>>,
}

/// Thread-safe LRU cache for query results.
///
/// # Thread Safety
///
/// The LRU structure uses a single `Mutex` for correctness. Metrics counters
/// use `AtomicU64` / `AtomicUsize` so no second lock is acquired in the hot path.
/// Under high concurrency this eliminates the double-lock contention that caused
/// cache hits to be slower than cache misses.
///
/// # Memory Safety
///
/// - **Hard LRU limit**: Configured via `max_entries`, automatically evicts least-recently-used
///   entries when limit is reached
/// - **TTL expiry**: Entries older than `ttl_seconds` are considered expired and removed on next
///   access
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
///     vec!["v_user".to_string()],
///     None, // use global TTL
///     None, // no entity type index
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

    /// Clock for TTL expiry checks. Injectable for deterministic testing.
    clock: Arc<dyn Clock>,

    // Metrics counters — atomic so the hot `get()` path acquires only ONE lock
    // (the LRU), not two. `Relaxed` ordering is sufficient: these counters are
    // independent and used only for monitoring, not for correctness.
    hits:          AtomicU64,
    misses:        AtomicU64,
    total_cached:  AtomicU64,
    invalidations: AtomicU64,
    size:          AtomicUsize,
    memory_bytes:  AtomicUsize,
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
        Self::new_with_clock(config, Arc::new(SystemClock))
    }

    /// Create a cache with a custom clock for deterministic time-based testing.
    ///
    /// # Panics
    ///
    /// Panics if `config.max_entries` is 0.
    #[must_use]
    pub fn new_with_clock(config: CacheConfig, clock: Arc<dyn Clock>) -> Self {
        let max = NonZeroUsize::new(config.max_entries).expect("max_entries must be > 0");

        Self {
            cache: Arc::new(Mutex::new(LruCache::new(max))),
            config,
            clock,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            size: AtomicUsize::new(0),
            memory_bytes: AtomicUsize::new(0),
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
    /// Returns error if cache mutex is poisoned (should never happen).
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
    /// Returns whether caching is enabled.
    ///
    /// Used by `CachedDatabaseAdapter` to short-circuit the SHA-256 key generation
    /// and result clone overhead when caching is disabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Look up a cached result by its cache key.
    ///
    /// Returns `None` when caching is disabled or the key is not present or expired.
    pub fn get(&self, cache_key: &str) -> Result<Option<Arc<Vec<JsonbValue>>>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let mut cache = self.cache.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Cache lock poisoned: {e}"),
            source:  None,
        })?;

        if let Some(cached) = cache.get_mut(cache_key) {
            // Check TTL: use per-entry override, fall back to global config.
            let now = self.clock.now_secs();
            if now - cached.cached_at > cached.ttl_seconds {
                // Expired: remove and count as miss
                cache.pop(cache_key);
                let new_size = cache.len();
                drop(cache); // Release LRU lock before atomic updates
                self.size.store(new_size, Ordering::Relaxed);
                self.misses.fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }

            // Cache hit: clone the Arc (zero-copy) while still holding the LRU lock
            cached.hit_count += 1;
            let result = cached.result.clone();
            drop(cache); // Release LRU lock before atomic update
            self.hits.fetch_add(1, Ordering::Relaxed);
            Ok(Some(result))
        } else {
            drop(cache); // Release LRU lock before atomic update
            self.misses.fetch_add(1, Ordering::Relaxed);
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
    /// * `ttl_override` - Per-entry TTL in seconds; `None` uses `CacheConfig::ttl_seconds`
    /// * `entity_type` - Optional GraphQL type name (e.g. `"User"`) for entity-ID indexing. When
    ///   provided, each row's `"id"` field is extracted and stored in `entity_ids` so that
    ///   `invalidate_by_entity()` can perform selective eviction.
    ///
    /// # Errors
    ///
    /// Returns error if cache mutex is poisoned.
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
    /// let result = vec![JsonbValue::new(json!({"id": "uuid-1"}))];
    /// cache.put("cache_key_abc123".to_string(), result, vec!["v_user".to_string()], None, Some("User"))?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn put(
        &self,
        cache_key: String,
        result: Vec<JsonbValue>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Respect cache_list_queries: a result with more than one row is considered a list.
        if !self.config.cache_list_queries && result.len() > 1 {
            return Ok(());
        }

        // Enforce per-entry size limit: estimate entry size from serialized JSON.
        if let Some(max_entry) = self.config.max_entry_bytes {
            let estimated = serde_json::to_vec(&result).map(|v| v.len()).unwrap_or(0);
            if estimated > max_entry {
                return Ok(()); // silently skip oversized entries
            }
        }

        // Enforce total cache size limit.
        if let Some(max_total) = self.config.max_total_bytes {
            let current = self.memory_bytes.load(Ordering::Relaxed);
            if current >= max_total {
                return Ok(()); // silently skip when budget is exhausted
            }
        }

        let now = self.clock.now_secs();
        let memory_size = std::mem::size_of::<CachedResult>() + cache_key.len() * 2;
        let ttl_seconds = ttl_override.unwrap_or(self.config.ttl_seconds);

        // TTL=0 means "never cache this entry" — skip storing it entirely.
        if ttl_seconds == 0 {
            return Ok(());
        }

        // Build entity-ID index: scan rows for "id" fields keyed by entity type.
        let entity_ids = if let Some(etype) = entity_type {
            let ids: HashSet<String> = result
                .iter()
                .filter_map(|row| {
                    row.as_value().as_object()?.get("id")?.as_str().map(str::to_string)
                })
                .collect();
            if ids.is_empty() {
                HashMap::new()
            } else {
                HashMap::from([(etype.to_string(), ids)])
            }
        } else {
            HashMap::new()
        };

        let cached = CachedResult {
            result: Arc::new(result),
            accessed_views,
            cached_at: now,
            ttl_seconds,
            hit_count: 0,
            entity_ids,
        };

        let mut cache = self.cache.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Cache lock poisoned: {e}"),
            source:  None,
        })?;
        cache.put(cache_key, cached);
        let new_size = cache.len();
        drop(cache); // Release LRU lock before atomic updates

        self.total_cached.fetch_add(1, Ordering::Relaxed);
        self.size.store(new_size, Ordering::Relaxed);
        self.memory_bytes.fetch_add(memory_size, Ordering::Relaxed);

        Ok(())
    }

    /// Invalidate entries accessing specified views.
    ///
    /// Called after mutations to invalidate affected cache entries.
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
    /// Returns error if cache mutex is poisoned.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::{QueryResultCache, CacheConfig};
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// // After createUser mutation
    /// let invalidated = cache.invalidate_views(&["v_user".to_string()])?;
    /// println!("Invalidated {} cache entries", invalidated);
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn invalidate_views(&self, views: &[String]) -> Result<u64> {
        let mut cache = self.cache.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Cache lock poisoned: {e}"),
            source:  None,
        })?;

        // Collect keys to remove (can't modify during iteration)
        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, cached)| cached.accessed_views.iter().any(|v| views.contains(v)))
            .map(|(k, _)| k.clone())
            .collect();

        for key in &keys_to_remove {
            cache.pop(key);
        }

        let new_size = cache.len();
        let invalidated_count = keys_to_remove.len() as u64;
        drop(cache); // Release LRU lock before atomic updates

        self.invalidations.fetch_add(invalidated_count, Ordering::Relaxed);
        self.size.store(new_size, Ordering::Relaxed);

        Ok(invalidated_count)
    }

    /// Evict cache entries that contain a specific entity UUID.
    ///
    /// Scans all entries whose `entity_ids` index contains the given `entity_id`
    /// under the given `entity_type` key, and removes them. Entries that do not
    /// reference this entity are left untouched.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - GraphQL type name (e.g. `"User"`)
    /// * `entity_id`   - UUID string of the mutated entity
    ///
    /// # Returns
    ///
    /// Number of cache entries evicted.
    ///
    /// # Errors
    ///
    /// Returns error if cache mutex is poisoned.
    pub fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        let mut cache = self.cache.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Cache lock poisoned: {e}"),
            source:  None,
        })?;

        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, cached)| {
                cached.entity_ids.get(entity_type).is_some_and(|ids| ids.contains(entity_id))
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in &keys_to_remove {
            cache.pop(key);
        }

        let new_size = cache.len();
        let invalidated_count = keys_to_remove.len() as u64;
        drop(cache);

        self.invalidations.fetch_add(invalidated_count, Ordering::Relaxed);
        self.size.store(new_size, Ordering::Relaxed);

        Ok(invalidated_count)
    }

    /// Get cache metrics snapshot.
    ///
    /// Returns a consistent snapshot of current counters. Individual fields may
    /// be updated independently (atomics), so the snapshot is not a single
    /// atomic transaction, but is accurate enough for monitoring.
    ///
    /// # Errors
    ///
    /// Always returns `Ok`. The `Result` return type is kept for API compatibility.
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
        Ok(CacheMetrics {
            hits:          self.hits.load(Ordering::Relaxed),
            misses:        self.misses.load(Ordering::Relaxed),
            total_cached:  self.total_cached.load(Ordering::Relaxed),
            invalidations: self.invalidations.load(Ordering::Relaxed),
            size:          self.size.load(Ordering::Relaxed),
            memory_bytes:  self.memory_bytes.load(Ordering::Relaxed),
        })
    }

    /// Clear all cache entries.
    ///
    /// Used for testing and manual cache flush.
    ///
    /// # Errors
    ///
    /// Returns error if cache mutex is poisoned.
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
        self.cache
            .lock()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("Cache lock poisoned: {e}"),
                source:  None,
            })?
            .clear();

        self.size.store(0, Ordering::Relaxed);
        self.memory_bytes.store(0, Ordering::Relaxed);

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    // Helper to create test result
    fn test_result() -> Vec<JsonbValue> {
        vec![JsonbValue::new(json!({"id": 1, "name": "test"}))]
    }

    // ========================================================================
    // Cache Hit/Miss Tests
    // ========================================================================

    #[test]
    fn test_cache_miss() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let result = cache.get("nonexistent_key").unwrap();
        assert!(result.is_none(), "Should be cache miss");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 0);
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let result = test_result();

        // Put
        cache
            .put("key1".to_string(), result, vec!["v_user".to_string()], None, None)
            .unwrap();

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
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

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
        let config = CacheConfig {
            ttl_seconds: 1, // 1 second TTL
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be expired
        let result = cache.get("key1").unwrap();
        assert!(result.is_none(), "Entry should be expired");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1); // Expired counts as miss
    }

    #[test]
    fn test_per_entry_ttl_override_expires_early() {
        // Global config has 1-hour TTL but entry overrides to 1 second
        let config = CacheConfig {
            ttl_seconds: 3600,
            enabled: true,
            ..Default::default()
        };
        let cache = QueryResultCache::new(config);

        cache
            .put(
                "key1".to_string(),
                test_result(),
                vec!["v_ref".to_string()],
                Some(1), // 1-second per-entry override
                None,
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(2));

        let result = cache.get("key1").unwrap();
        assert!(result.is_none(), "Entry with per-entry TTL=1s should have expired");
    }

    #[test]
    fn test_per_entry_ttl_zero_never_cached() {
        // TTL=0 means an entry is immediately expired on the first get()
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_live".to_string()], Some(0), None)
            .unwrap();

        let result = cache.get("key1").unwrap();
        assert!(result.is_none(), "Entry with TTL=0 should be immediately expired");
    }

    #[test]
    fn test_ttl_not_expired() {
        let config = CacheConfig {
            ttl_seconds: 3600, // 1 hour TTL
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Should still be valid
        let result = cache.get("key1").unwrap();
        assert!(result.is_some(), "Entry should not be expired");
    }

    // ========================================================================
    // LRU Eviction Tests
    // ========================================================================

    #[test]
    fn test_lru_eviction() {
        let config = CacheConfig {
            max_entries: 2, // Only 2 entries
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        // Add 3 entries (max is 2)
        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key2".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key3".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // key1 should be evicted (LRU)
        assert!(cache.get("key1").unwrap().is_none(), "Oldest entry should be evicted");
        assert!(cache.get("key2").unwrap().is_some());
        assert!(cache.get("key3").unwrap().is_some());

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 2, "Cache size should be at max");
    }

    #[test]
    fn test_lru_updates_on_access() {
        let config = CacheConfig {
            max_entries: 2,
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key2".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Access key1 (makes it recently used)
        cache.get("key1").unwrap();

        // Add key3 (should evict key2, not key1)
        cache
            .put("key3".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

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
        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

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
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key2".to_string(), test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();

        // Invalidate v_user
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        // v_user entry gone, v_post remains
        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_some());
    }

    #[test]
    fn test_invalidate_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key2".to_string(), test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();
        cache
            .put("key3".to_string(), test_result(), vec!["v_product".to_string()], None, None)
            .unwrap();

        // Invalidate v_user and v_post
        let invalidated =
            cache.invalidate_views(&["v_user".to_string(), "v_post".to_string()]).unwrap();
        assert_eq!(invalidated, 2);

        assert!(cache.get("key1").unwrap().is_none());
        assert!(cache.get("key2").unwrap().is_none());
        assert!(cache.get("key3").unwrap().is_some());
    }

    #[test]
    fn test_invalidate_entry_with_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Entry accesses both v_user and v_post
        cache
            .put(
                "key1".to_string(),
                test_result(),
                vec!["v_user".to_string(), "v_post".to_string()],
                None,
                None,
            )
            .unwrap();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        assert!(cache.get("key1").unwrap().is_none());
    }

    #[test]
    fn test_invalidate_nonexistent_view() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Invalidate view that doesn't exist
        let invalidated = cache.invalidate_views(&["v_nonexistent".to_string()]).unwrap();
        assert_eq!(invalidated, 0);

        // Entry should remain
        assert!(cache.get("key1").unwrap().is_some());
    }

    // ========================================================================
    // Clear Tests
    // ========================================================================

    #[test]
    fn test_clear() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put("key2".to_string(), test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();

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
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Miss
        cache.get("NotThere").unwrap();

        // Put
        cache
            .put("key1".to_string(), test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

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
            hits:          80,
            misses:        20,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };

        assert!((metrics.hit_rate() - 0.8).abs() < f64::EPSILON);
        assert!(metrics.is_healthy());
    }

    #[test]
    fn test_metrics_hit_rate_zero_requests() {
        let metrics = CacheMetrics {
            hits:          0,
            misses:        0,
            total_cached:  0,
            invalidations: 0,
            size:          0,
            memory_bytes:  0,
        };

        assert!((metrics.hit_rate() - 0.0).abs() < f64::EPSILON);
        assert!(!metrics.is_healthy());
    }

    #[test]
    fn test_metrics_is_healthy() {
        let good = CacheMetrics {
            hits:          70,
            misses:        30,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };
        assert!(good.is_healthy()); // 70% > 60%

        let bad = CacheMetrics {
            hits:          50,
            misses:        50,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };
        assert!(!bad.is_healthy()); // 50% < 60%
    }

    // ========================================================================
    // Entity-Aware Invalidation Tests
    // ========================================================================

    fn entity_result(id: &str) -> Vec<JsonbValue> {
        vec![JsonbValue::new(
            serde_json::json!({"id": id, "name": "test"}),
        )]
    }

    #[test]
    fn test_invalidate_by_entity_only_removes_matching_entries() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache User A and User B as separate entries
        cache
            .put(
                "user-a".to_string(),
                entity_result("uuid-a"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
        cache
            .put(
                "user-b".to_string(),
                entity_result("uuid-b"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();

        // Invalidate User A — User B must remain
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get("user-a").unwrap().is_none(), "User A should be evicted");
        assert!(cache.get("user-b").unwrap().is_some(), "User B should remain");
    }

    #[test]
    fn test_invalidate_by_entity_removes_list_containing_entity() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache a "users list" entry that contains both A and B
        let list = vec![
            JsonbValue::new(serde_json::json!({"id": "uuid-a", "name": "Alice"})),
            JsonbValue::new(serde_json::json!({"id": "uuid-b", "name": "Bob"})),
        ];
        cache
            .put("users-list".to_string(), list, vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidate by User A — the list entry contains A, so it must be evicted
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(
            cache.get("users-list").unwrap().is_none(),
            "List containing A should be evicted"
        );
    }

    #[test]
    fn test_invalidate_by_entity_leaves_unrelated_types() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache a User entry and a Post entry
        cache
            .put(
                "user-key".to_string(),
                entity_result("uuid-user"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
        cache
            .put(
                "post-key".to_string(),
                entity_result("uuid-post"),
                vec!["v_post".to_string()],
                None,
                Some("Post"),
            )
            .unwrap();

        // Invalidate the User — Post entry must remain untouched
        let evicted = cache.invalidate_by_entity("User", "uuid-user").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get("user-key").unwrap().is_none(), "User entry should be evicted");
        assert!(cache.get("post-key").unwrap().is_some(), "Post entry should remain");
    }

    #[test]
    fn test_put_builds_entity_id_index() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let rows = vec![
            JsonbValue::new(serde_json::json!({"id": "uuid-1", "name": "Alice"})),
            JsonbValue::new(serde_json::json!({"id": "uuid-2", "name": "Bob"})),
        ];
        cache
            .put("list-key".to_string(), rows, vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidating by uuid-1 should evict the entry
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get("list-key").unwrap().is_none());
    }

    #[test]
    fn test_put_without_entity_type_not_indexed() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(
                "no-type-key".to_string(),
                entity_result("uuid-1"),
                vec!["v_user".to_string()],
                None,
                None, // no entity type
            )
            .unwrap();

        // invalidate_by_entity should not match (no index was built)
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 0);
        assert!(cache.get("no-type-key").unwrap().is_some(), "Non-indexed entry should remain");
    }

    // ========================================================================
    // Thread Safety Tests
    // ========================================================================

    #[test]
    fn test_concurrent_access() {
        use std::{sync::Arc, thread};

        let cache = Arc::new(QueryResultCache::new(CacheConfig::enabled()));

        // Spawn multiple threads accessing cache
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let cache_clone = cache.clone();
                thread::spawn(move || {
                    let key = format!("key{}", i);
                    cache_clone
                        .put(key.clone(), test_result(), vec!["v_user".to_string()], None, None)
                        .unwrap();
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
