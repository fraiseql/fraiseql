//! Query result caching with LRU eviction and TTL expiry.
//!
//! This module provides a 64-shard striped LRU cache for GraphQL query results.
//! Each shard holds `capacity / NUM_SHARDS` entries behind its own
//! [`parking_lot::Mutex`], eliminating the single-lock bottleneck under high
//! concurrency.
//!
//! ## Performance characteristics
//!
//! - **`get()` hot path** (cache hit): one shard lock, O(1) LRU promotion,
//!   `Arc` clone (single atomic increment), one atomic counter bump.
//! - **`put()` path**: early-exit guards (disabled / list / size / TTL=0)
//!   before touching any lock. Entity-ID index is built outside the lock.
//!   Shard lock held only for the `push()` call.
//! - **`metrics()`**: lazily computes `size` by scanning all shards. Called
//!   rarely (monitoring), never on the query hot path.
//! - **Invalidation**: iterates all shards (acceptable — mutations are rare).

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use lru::LruCache;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::config::CacheConfig;
use crate::{
    db::types::JsonbValue,
    error::Result,
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
    /// Stored as a boxed slice (no excess capacity) since views are fixed
    /// at `put()` time and never modified.
    ///
    /// Used for view-based invalidation when mutations modify these views.
    pub accessed_views: Box<[String]>,

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

/// Number of shards for the striped LRU cache.
///
/// 64 shards reduce mutex contention to ~1/64 under uniform key distribution.
const NUM_SHARDS: usize = 64;

/// Thread-safe 64-shard striped LRU cache for query results.
///
/// # Thread Safety
///
/// Each shard is an independent `parking_lot::Mutex<LruCache>` holding
/// `capacity / 64` entries. Concurrent requests that hash to different shards
/// never contend on the same lock. `parking_lot::Mutex` is used over
/// `std::sync::Mutex` for:
/// - **No poisoning**: a panic in one thread does not permanently break the cache
/// - **Smaller footprint**: 1 byte vs 40 bytes per mutex on Linux
/// - **Faster lock/unlock**: optimized futex-based implementation
///
/// Metrics counters use `AtomicU64` / `AtomicUsize` so no second lock is
/// acquired in the hot path.
///
/// # Memory Safety
///
/// - **Hard LRU limit**: Each shard evicts least-recently-used entries independently
/// - **TTL expiry**: Entries older than `ttl_seconds` are considered expired and removed on next
///   access
/// - **Memory tracking**: `memory_bytes` tracked via atomic add/sub; `size` computed lazily in
///   `metrics()`
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
///     12345_u64,
///     result.clone(),
///     vec!["v_user".to_string()],
///     None, // use global TTL
///     None, // no entity type index
/// ).unwrap();
///
/// // Retrieve from cache
/// if let Some(cached) = cache.get(12345).unwrap() {
///     println!("Cache hit! {} results", cached.len());
/// }
/// ```
pub struct QueryResultCache {
    /// Striped LRU shards: key is routed to `shards[key % len]`.
    shards: Box<[Mutex<LruCache<u64, CachedResult>>]>,

    /// Configuration (immutable after creation).
    config: CacheConfig,

    /// Clock for TTL expiry checks. Injectable for deterministic testing.
    clock: Arc<dyn Clock>,

    // Metrics counters — atomic so the hot `get()` path acquires only ONE shard
    // lock, not two. `Relaxed` ordering is sufficient: these counters are
    // independent and used only for monitoring, not for correctness.
    hits:          AtomicU64,
    misses:        AtomicU64,
    total_cached:  AtomicU64,
    invalidations: AtomicU64,
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

/// Estimate the accounting overhead of one cache entry.
///
/// The LRU crate stores the key twice (once in the `HashMap`, once in the
/// linked-list node). We add the `CachedResult` struct size.
const fn entry_overhead() -> usize {
    std::mem::size_of::<CachedResult>() + std::mem::size_of::<u64>() * 2
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
        assert!(config.max_entries > 0, "max_entries must be > 0");

        // Use full sharding only when capacity is large enough (≥ NUM_SHARDS).
        // Below that threshold, a single shard preserves exact global LRU ordering.
        let num_shards = if config.max_entries >= NUM_SHARDS { NUM_SHARDS } else { 1 };
        let per_shard = config.max_entries.div_ceil(num_shards);
        let per_shard_nz = NonZeroUsize::new(per_shard).expect("per_shard > 0");

        let shards: Box<[_]> = (0..num_shards)
            .map(|_| Mutex::new(LruCache::new(per_shard_nz)))
            .collect();

        Self {
            shards,
            config,
            clock,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            memory_bytes: AtomicUsize::new(0),
        }
    }

    /// Returns whether caching is enabled.
    ///
    /// Used by `CachedDatabaseAdapter` to short-circuit key generation
    /// and result clone overhead when caching is disabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Select the shard for a given cache key.
    ///
    /// The key is already a hash (u64), so we just modulo into shard_count
    /// directly — no need to rehash.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    // Reason: truncation is intentional; we only need a uniform index into shard_count
    fn shard_for(&self, key: u64) -> &Mutex<LruCache<u64, CachedResult>> {
        let idx = (key as usize) % self.shards.len();
        &self.shards[idx]
    }

    /// Look up a cached result by its cache key.
    ///
    /// Returns `None` when caching is disabled or the key is not present or expired.
    ///
    /// # Errors
    ///
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
    pub fn get(&self, cache_key: u64) -> Result<Option<Arc<Vec<JsonbValue>>>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let mut cache = self.shard_for(cache_key).lock();

        if let Some(cached) = cache.get_mut(&cache_key) {
            // Check TTL: use per-entry override, fall back to global config.
            let now = self.clock.now_secs();
            if now - cached.cached_at > cached.ttl_seconds {
                // Expired: remove and count as miss.
                cache.pop(&cache_key);
                drop(cache); // Release shard lock before atomic updates

                self.memory_bytes.fetch_sub(entry_overhead(), Ordering::Relaxed);
                self.misses.fetch_add(1, Ordering::Relaxed);
                return Ok(None);
            }

            // Cache hit: clone the Arc (zero-copy) while still holding the shard lock.
            cached.hit_count += 1;
            let result = cached.result.clone();
            drop(cache); // Release shard lock before atomic update
            self.hits.fetch_add(1, Ordering::Relaxed);
            Ok(Some(result))
        } else {
            drop(cache); // Release shard lock before atomic update
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
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
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
    /// cache.put(0xabc123, result, vec!["v_user".to_string()], None, Some("User"))?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn put(
        &self,
        cache_key: u64,
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
            let estimated = serde_json::to_vec(&result).map_or(0, |v| v.len());
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

        let ttl_seconds = ttl_override.unwrap_or(self.config.ttl_seconds);

        // TTL=0 means "never cache this entry" — skip storing it entirely.
        if ttl_seconds == 0 {
            return Ok(());
        }

        let now = self.clock.now_secs();
        // Build entity-ID index outside the lock: scan rows for "id" fields.
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
            accessed_views: accessed_views.into_boxed_slice(),
            cached_at: now,
            ttl_seconds,
            hit_count: 0,
            entity_ids,
        };

        // --- Critical section: hold shard lock only for the insert ---
        let mut guard = self.shard_for(cache_key).lock();
        let evicted = guard.push(cache_key, cached);
        drop(guard);
        // --- End critical section ---

        self.total_cached.fetch_add(1, Ordering::Relaxed);

        // Adjust memory_bytes: add new entry, subtract evicted entry if any.
        // push() returns Some((key, value)) when it evicts the LRU tail OR
        // when the key already existed (replacement). Either way, we subtract.
        // With u64 keys, entry overhead is constant — evicted and new are the same size.
        if evicted.is_none() {
            self.memory_bytes.fetch_add(entry_overhead(), Ordering::Relaxed);
        }

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
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
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
        let mut total_invalidated: u64 = 0;
        let mut total_freed: usize = 0;

        for shard in &*self.shards {
            let mut cache = shard.lock();

            let keys_to_remove: Vec<u64> = cache
                .iter()
                .filter(|(_, cached)| cached.accessed_views.iter().any(|v| views.contains(v)))
                .map(|(k, _)| *k)
                .collect();

            let freed_bytes: usize =
                keys_to_remove.iter().map(|_| entry_overhead()).sum();

            for key in &keys_to_remove {
                cache.pop(key);
            }

            #[allow(clippy::cast_possible_truncation)] // Reason: key count within a shard never exceeds u64
            let count = keys_to_remove.len() as u64;
            total_invalidated += count;
            total_freed += freed_bytes;
        }

        self.invalidations.fetch_add(total_invalidated, Ordering::Relaxed);
        self.memory_bytes.fetch_sub(
            total_freed.min(self.memory_bytes.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );

        Ok(total_invalidated)
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
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
    pub fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        let mut total_invalidated: u64 = 0;
        let mut total_freed: usize = 0;

        for shard in &*self.shards {
            let mut cache = shard.lock();

            let keys_to_remove: Vec<u64> = cache
                .iter()
                .filter(|(_, cached)| {
                    cached.entity_ids.get(entity_type).is_some_and(|ids| ids.contains(entity_id))
                })
                .map(|(k, _)| *k)
                .collect();

            let freed_bytes: usize =
                keys_to_remove.iter().map(|_| entry_overhead()).sum();

            for key in &keys_to_remove {
                cache.pop(key);
            }

            #[allow(clippy::cast_possible_truncation)] // Reason: key count within a shard never exceeds u64
            let count = keys_to_remove.len() as u64;
            total_invalidated += count;
            total_freed += freed_bytes;
        }

        self.invalidations.fetch_add(total_invalidated, Ordering::Relaxed);
        self.memory_bytes.fetch_sub(
            total_freed.min(self.memory_bytes.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );

        Ok(total_invalidated)
    }

    /// Get cache metrics snapshot.
    ///
    /// Returns a consistent snapshot of current counters. Individual fields may
    /// be updated independently (atomics), so the snapshot is not a single
    /// atomic transaction, but is accurate enough for monitoring.
    ///
    /// `size` is computed lazily by scanning all shards — this keeps the
    /// `get()`/`put()` hot paths free of cross-shard coordination.
    ///
    /// # Errors
    ///
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
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
        // Compute size by scanning all shards. This is O(NUM_SHARDS) lock
        // acquisitions but metrics() is called rarely (monitoring endpoints),
        // never on the query hot path.
        let size: usize = self.shards.iter().map(|s| s.lock().len()).sum();

        Ok(CacheMetrics {
            hits:          self.hits.load(Ordering::Relaxed),
            misses:        self.misses.load(Ordering::Relaxed),
            total_cached:  self.total_cached.load(Ordering::Relaxed),
            invalidations: self.invalidations.load(Ordering::Relaxed),
            size,
            memory_bytes:  self.memory_bytes.load(Ordering::Relaxed),
        })
    }

    /// Clear all cache entries.
    ///
    /// Used for testing and manual cache flush.
    ///
    /// # Errors
    ///
    /// This method is infallible with `parking_lot::Mutex` (no poisoning).
    /// The `Result` return type is kept for API compatibility.
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
        for shard in &*self.shards {
            shard.lock().clear();
        }

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
        #[allow(clippy::cast_precision_loss)]  // Reason: precision loss acceptable for metric/ratio calculations
        // Reason: hit-rate is a display metric; f64 precision loss on u64 counters is acceptable
        // here.
        {
            self.hits as f64 / total as f64
        }
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

        let result = cache.get(999_u64).unwrap();
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
            .put(1_u64, result, vec!["v_user".to_string()], None, None)
            .unwrap();

        // Get
        let cached = cache.get(1_u64).unwrap();
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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // First hit
        cache.get(1_u64).unwrap();
        // Second hit
        cache.get(1_u64).unwrap();

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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be expired
        let result = cache.get(1_u64).unwrap();
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
                1_u64,
                test_result(),
                vec!["v_ref".to_string()],
                Some(1), // 1-second per-entry override
                None,
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(2));

        let result = cache.get(1_u64).unwrap();
        assert!(result.is_none(), "Entry with per-entry TTL=1s should have expired");
    }

    #[test]
    fn test_per_entry_ttl_zero_never_cached() {
        // TTL=0 means an entry is immediately expired on the first get()
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_live".to_string()], Some(0), None)
            .unwrap();

        let result = cache.get(1_u64).unwrap();
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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Should still be valid
        let result = cache.get(1_u64).unwrap();
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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(3_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // key1 should be evicted (LRU)
        assert!(cache.get(1_u64).unwrap().is_none(), "Oldest entry should be evicted");
        assert!(cache.get(2_u64).unwrap().is_some());
        assert!(cache.get(3_u64).unwrap().is_some());

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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Access key1 (makes it recently used)
        cache.get(1_u64).unwrap();

        // Add key3 (should evict key2, not key1)
        cache
            .put(3_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        assert!(cache.get(1_u64).unwrap().is_some(), "key1 should remain (recently used)");
        assert!(cache.get(2_u64).unwrap().is_none(), "key2 should be evicted (LRU)");
        assert!(cache.get(3_u64).unwrap().is_some());
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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Get should return None
        assert!(cache.get(1_u64).unwrap().is_none(), "Cache disabled should always miss");

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
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();

        // Invalidate v_user
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        // v_user entry gone, v_post remains
        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_some());
    }

    #[test]
    fn test_invalidate_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();
        cache
            .put(3_u64, test_result(), vec!["v_product".to_string()], None, None)
            .unwrap();

        // Invalidate v_user and v_post
        let invalidated =
            cache.invalidate_views(&["v_user".to_string(), "v_post".to_string()]).unwrap();
        assert_eq!(invalidated, 2);

        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_none());
        assert!(cache.get(3_u64).unwrap().is_some());
    }

    #[test]
    fn test_invalidate_entry_with_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Entry accesses both v_user and v_post
        cache
            .put(
                1_u64,
                test_result(),
                vec!["v_user".to_string(), "v_post".to_string()],
                None,
                None,
            )
            .unwrap();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        assert!(cache.get(1_u64).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_nonexistent_view() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Invalidate view that doesn't exist
        let invalidated = cache.invalidate_views(&["v_nonexistent".to_string()]).unwrap();
        assert_eq!(invalidated, 0);

        // Entry should remain
        assert!(cache.get(1_u64).unwrap().is_some());
    }

    // ========================================================================
    // Clear Tests
    // ========================================================================

    #[test]
    fn test_clear() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v_post".to_string()], None, None)
            .unwrap();

        cache.clear().unwrap();

        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_none());

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
        cache.get(999_u64).unwrap();

        // Put
        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        // Hit
        cache.get(1_u64).unwrap();

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
                1_u64,
                entity_result("uuid-a"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
        cache
            .put(
                2_u64,
                entity_result("uuid-b"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();

        // Invalidate User A — User B must remain
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "User A should be evicted");
        assert!(cache.get(2_u64).unwrap().is_some(), "User B should remain");
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
            .put(1_u64, list, vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidate by User A — the list entry contains A, so it must be evicted
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "List containing A should be evicted"
        );
    }

    #[test]
    fn test_invalidate_by_entity_leaves_unrelated_types() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache a User entry and a Post entry
        cache
            .put(
                1_u64,
                entity_result("uuid-user"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
        cache
            .put(
                2_u64,
                entity_result("uuid-post"),
                vec!["v_post".to_string()],
                None,
                Some("Post"),
            )
            .unwrap();

        // Invalidate the User — Post entry must remain untouched
        let evicted = cache.invalidate_by_entity("User", "uuid-user").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "User entry should be evicted");
        assert!(cache.get(2_u64).unwrap().is_some(), "Post entry should remain");
    }

    #[test]
    fn test_put_builds_entity_id_index() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let rows = vec![
            JsonbValue::new(serde_json::json!({"id": "uuid-1", "name": "Alice"})),
            JsonbValue::new(serde_json::json!({"id": "uuid-2", "name": "Bob"})),
        ];
        cache
            .put(1_u64, rows, vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidating by uuid-1 should evict the entry
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none());
    }

    #[test]
    fn test_put_without_entity_type_not_indexed() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(
                1_u64,
                entity_result("uuid-1"),
                vec!["v_user".to_string()],
                None,
                None, // no entity type
            )
            .unwrap();

        // invalidate_by_entity should not match (no index was built)
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 0);
        assert!(cache.get(1_u64).unwrap().is_some(), "Non-indexed entry should remain");
    }

    // ========================================================================
    // Thread Safety Tests
    // ========================================================================

    #[test]
    fn test_concurrent_access() {
        use std::{sync::Arc, thread};

        let cache = Arc::new(QueryResultCache::new(CacheConfig::enabled()));

        // Spawn multiple threads accessing cache
        let handles: Vec<_> = (0_u64..10)
            .map(|key| {
                let cache_clone = cache.clone();
                thread::spawn(move || {
                    cache_clone
                        .put(key, test_result(), vec!["v_user".to_string()], None, None)
                        .unwrap();
                    cache_clone.get(key).unwrap();
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

    // ========================================================================
    // Sentinel tests — boundary guards for mutation testing
    // ========================================================================

    /// Sentinel: `cache_list_queries = false` must skip results with >1 row.
    ///
    /// Kills the `> → >=` mutation at the list-query guard: `result.len() > 1`.
    #[test]
    fn test_cache_list_queries_false_skips_multi_row() {
        let config = CacheConfig {
            enabled:            true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Two-row result: must be skipped (killed by > → >= mutant)
        let two_rows = vec![
            JsonbValue::new(json!({"id": 1})),
            JsonbValue::new(json!({"id": 2})),
        ];
        cache
            .put(1_u64, two_rows, vec!["v_user".to_string()], None, None)
            .unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "multi-row result must not be cached when cache_list_queries=false"
        );
    }

    /// Sentinel: `cache_list_queries = false` must still store single-row results.
    ///
    /// Complements the above: the single-row path must remain unaffected.
    #[test]
    fn test_cache_list_queries_false_allows_single_row() {
        let config = CacheConfig {
            enabled:            true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // One-row result: must be stored
        let one_row = vec![JsonbValue::new(json!({"id": 1}))];
        cache
            .put(1_u64, one_row, vec!["v_user".to_string()], None, None)
            .unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_some(),
            "single-row result must be cached even when cache_list_queries=false"
        );
    }

    /// Sentinel: entries exceeding `max_entry_bytes` must be silently skipped.
    ///
    /// Kills mutations on the `estimated > max_entry` guard.
    #[test]
    fn test_max_entry_bytes_skips_oversized_entry() {
        let config = CacheConfig {
            enabled:         true,
            max_entry_bytes: Some(10), // 10 bytes — smaller than any JSON row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // A typical row serialises to far more than 10 bytes
        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "oversized entry must be silently skipped"
        );
    }

    /// Sentinel: entries within `max_entry_bytes` must be stored normally.
    ///
    /// Complements the above to pin both sides of the size boundary.
    #[test]
    fn test_max_entry_bytes_allows_small_entry() {
        let config = CacheConfig {
            enabled:         true,
            max_entry_bytes: Some(100_000), // 100 KB — plenty for a test row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_some(),
            "small entry must be cached when within max_entry_bytes"
        );
    }

    /// Sentinel: `put()` must skip new entries when `max_total_bytes` budget is exhausted.
    ///
    /// Kills mutations on the `current >= max_total` guard.
    #[test]
    fn test_max_total_bytes_skips_when_budget_exhausted() {
        let config = CacheConfig {
            enabled:        true,
            max_total_bytes: Some(0), // 0 bytes — always exhausted
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "entry must be skipped when max_total_bytes budget is already exhausted"
        );
    }

    // ========================================================================
    // Sharding Tests
    // ========================================================================

    /// Verify that a large cache uses 64 shards.
    #[test]
    fn test_sharded_cache_has_64_shards() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);
        assert_eq!(cache.shards.len(), NUM_SHARDS);
    }

    /// Small capacities (< 64) fall back to 1 shard for exact LRU ordering.
    #[test]
    fn test_small_capacity_uses_single_shard() {
        let config = CacheConfig {
            max_entries: 10,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);
        assert_eq!(cache.shards.len(), 1);
    }

    /// Cross-shard invalidation: `invalidate_views` clears matching entries
    /// regardless of which shard they reside in.
    #[test]
    fn test_cross_shard_view_invalidation() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Insert many entries across different shards
        for i in 0_u64..200 {
            let view = if i % 2 == 0 { "v_user" } else { "v_post" };
            cache
                .put(i, test_result(), vec![view.to_string()], None, None)
                .unwrap();
        }

        // Invalidate v_user — should remove exactly 100 entries
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 100);

        // All v_user entries gone, all v_post entries remain
        for i in 0_u64..200 {
            if i % 2 == 0 {
                assert!(cache.get(i).unwrap().is_none(), "v_user entry should be invalidated");
            } else {
                assert!(cache.get(i).unwrap().is_some(), "v_post entry should remain");
            }
        }
    }

    /// Cross-shard entity invalidation works across all shards.
    #[test]
    fn test_cross_shard_entity_invalidation() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Insert entries for the same entity across different cache keys
        for i in 0_u64..50 {
            cache
                .put(
                    i,
                    entity_result("uuid-target"),
                    vec!["v_user".to_string()],
                    None,
                    Some("User"),
                )
                .unwrap();
        }

        // Also insert an unrelated entry
        cache
            .put(
                999_u64,
                entity_result("uuid-other"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();

        let evicted = cache.invalidate_by_entity("User", "uuid-target").unwrap();
        assert_eq!(evicted, 50);
        assert!(cache.get(999_u64).unwrap().is_some(), "unrelated entity should remain");
    }

    /// Clear works across all shards.
    #[test]
    fn test_clear_all_shards() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        for i in 0_u64..200 {
            cache
                .put(
                    i,
                    test_result(),
                    vec!["v_user".to_string()],
                    None,
                    None,
                )
                .unwrap();
        }

        cache.clear().unwrap();
        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);

        for i in 0_u64..200 {
            assert!(cache.get(i).unwrap().is_none());
        }
    }

    /// Verify `push()` returns evicted entries for correct memory accounting.
    #[test]
    fn test_memory_bytes_tracked_on_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache
            .put(1_u64, test_result(), vec!["v".to_string()], None, None)
            .unwrap();
        cache
            .put(2_u64, test_result(), vec!["v".to_string()], None, None)
            .unwrap();

        let before = cache.memory_bytes.load(Ordering::Relaxed);
        assert!(before > 0, "memory_bytes should be tracked");

        // Evict k1 by adding k3 (same key length → memory_bytes unchanged)
        cache
            .put(3_u64, test_result(), vec!["v".to_string()], None, None)
            .unwrap();

        let after = cache.memory_bytes.load(Ordering::Relaxed);
        assert_eq!(before, after, "memory_bytes should remain stable after same-size eviction");
    }

    /// Verify `memory_bytes` decreases after invalidation.
    #[test]
    fn test_memory_bytes_decreases_on_invalidation() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_user".to_string()], None, None)
            .unwrap();

        let before = cache.memory_bytes.load(Ordering::Relaxed);
        assert!(before > 0);

        cache.invalidate_views(&["v_user".to_string()]).unwrap();

        let after = cache.memory_bytes.load(Ordering::Relaxed);
        assert_eq!(after, 0, "memory_bytes should be zero after invalidating all entries");
    }
}
