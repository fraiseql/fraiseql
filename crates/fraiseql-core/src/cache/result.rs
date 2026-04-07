//! Query result caching with W-TinyLFU eviction and per-entry TTL.
//!
//! This module provides a `moka::sync::Cache`-backed store for GraphQL query results.
//! Moka uses Concurrent W-TinyLFU policy with lock-free reads — cache hits do NOT
//! acquire any shared lock, eliminating the hot-key serialisation bottleneck present
//! in the old 64-shard `parking_lot::Mutex<LruCache>` design.
//!
//! ## Performance characteristics
//!
//! - **`get()` hot path** (cache hit): lock-free frequency-counter update (thread-local
//!   ring buffer, drained lazily on writes), `Arc` clone (single atomic increment),
//!   one atomic counter bump.
//! - **`put()` path**: early-exit guards (disabled / list / size) before touching
//!   the store. Reverse-index updates use `DashMap` (fine-grained sharding, no global lock).
//! - **`metrics()`**: reads `store.entry_count()` directly — no shard scan.
//! - **`invalidate_views()` / `invalidate_by_entity()`**: O(k) where k = matching entries
//!   (via reverse indexes), not O(total entries).
//!
//! ## Reverse indexes
//!
//! Because `moka` does not support arbitrary iteration, view-based and entity-based
//! invalidation rely on two `DashMap` reverse indexes maintained alongside the store:
//!
//! ```text
//! view_index:   DashMap<view_name,   DashSet<cache_key>>
//! entity_index: DashMap<entity_type, DashMap<entity_id, DashSet<cache_key>>>
//! ```
//!
//! Indexes are populated in `put()` and pruned via moka's eviction listener (fired
//! asynchronously). `clear()` resets all indexes synchronously.

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use moka::sync::Cache as MokaCache;
use serde::{Deserialize, Serialize};

use super::config::CacheConfig;
use crate::{db::types::JsonbValue, error::Result};

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
    pub accessed_views: Box<[String]>,

    /// When this entry was cached (Unix timestamp in seconds).
    ///
    /// Wall-clock timestamp for debugging. TTL enforcement is handled by moka
    /// internally via `CacheEntryExpiry`.
    pub cached_at: u64,

    /// Per-entry TTL in seconds.
    ///
    /// Overrides `CacheConfig::ttl_seconds` when set via `put(..., Some(ttl))`.
    /// Read by `CacheEntryExpiry::expire_after_create` to tell moka the expiry.
    pub ttl_seconds: u64,

    /// Entity reference for selective entity-level invalidation.
    ///
    /// Stores `(entity_type, entity_id)` when `put()` is called with
    /// `entity_type = Some(...)` and the result rows contain an `"id"` field.
    /// Used by the eviction listener to clean up the `entity_index` on eviction.
    pub entity_ref: Option<(String, String)>,
}

/// Moka `Expiry` implementation: reads TTL from `CachedResult.ttl_seconds`.
struct CacheEntryExpiry;

impl moka::Expiry<u64, Arc<CachedResult>> for CacheEntryExpiry {
    fn expire_after_create(
        &self,
        _key: &u64,
        value: &Arc<CachedResult>,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        if value.ttl_seconds == 0 {
            // TTL=0 means "no time-based expiry" — entry lives until explicitly
            // invalidated by a mutation.  Return None so moka never schedules
            // a timer-wheel eviction for this entry.
            None
        } else {
            Some(Duration::from_secs(value.ttl_seconds))
        }
    }

    // `expire_after_read` is intentionally NOT overridden.
    //
    // Moka's default returns `None` (no change to the timer) which skips the
    // internal timer-wheel reschedule on every get().  Overriding it to return
    // `duration_until_expiry` — even though the value is semantically unchanged —
    // forces moka to acquire its timer-wheel lock on every cache hit.  Under 40
    // concurrent workers reading the same key, that lock becomes the new hot-key
    // bottleneck, serialising reads and degrading list-query throughput ~3×.
    //
    // Entries expire at creation_time + ttl_seconds regardless of read frequency,
    // which is the correct fixed-TTL semantics for query result caching.
}

/// Thread-safe W-TinyLFU cache for query results.
///
/// Backed by [`moka::sync::Cache`] which provides lock-free reads via
/// Concurrent `TinyLFU`. Reverse `DashMap` indexes enable O(k) invalidation.
///
/// # Thread Safety
///
/// `moka::sync::Cache` is `Send + Sync`. All reverse indexes use `DashMap`
/// (fine-grained shard locking) and `DashSet` (also shard-locked). There is no
/// global mutex on the read path.
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
    /// Moka W-TinyLFU store.
    ///
    /// `Arc<CachedResult>` rather than `CachedResult` so that `get()` returns in
    /// one atomic increment instead of deep-cloning the struct (which would copy
    /// `accessed_views: Box<[String]>` on every cache hit).
    store: MokaCache<u64, Arc<CachedResult>>,

    /// Configuration (immutable after creation).
    config: CacheConfig,

    // Metrics counters — `Relaxed` ordering is sufficient: these counters are
    // used only for monitoring, not for correctness or synchronisation.
    hits:          AtomicU64,
    misses:        AtomicU64,
    total_cached:  AtomicU64,
    invalidations: AtomicU64,

    /// Estimated total memory in use.
    ///
    /// Wrapped in `Arc` so the eviction listener closure (which requires `'static`)
    /// can hold a clone and decrement on eviction.
    memory_bytes: Arc<AtomicUsize>,

    /// Reverse index: view name → set of cache keys accessing that view.
    view_index: Arc<DashMap<String, DashSet<u64>>>,

    /// Reverse index: entity type → entity id → set of cache keys.
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>>,
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
    /// This is a rough estimate based on `CachedResult` struct size.
    /// Actual memory usage may vary based on result sizes.
    pub memory_bytes: usize,
}

/// Estimate the per-entry accounting overhead.
const fn entry_overhead() -> usize {
    std::mem::size_of::<CachedResult>() + std::mem::size_of::<u64>() * 2
}

/// Build the moka store, wiring the eviction listener to the reverse indexes
/// and memory counter.
fn build_store(
    config: &CacheConfig,
    memory_bytes: Arc<AtomicUsize>,
    view_index: Arc<DashMap<String, DashSet<u64>>>,
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>>,
) -> MokaCache<u64, Arc<CachedResult>> {
    let max_cap = config.max_entries as u64;
    let mb = memory_bytes;
    let vi = view_index;
    let ei = entity_index;

    MokaCache::builder()
        .max_capacity(max_cap)
        .expire_after(CacheEntryExpiry)
        .eviction_listener(move |key: Arc<u64>, value: Arc<CachedResult>, _cause| {
            // Decrement memory budget so put()'s byte-gate stays accurate.
            mb.fetch_sub(entry_overhead(), Ordering::Relaxed);

            // Remove key from view index.
            for view in &value.accessed_views {
                if let Some(keys) = vi.get(view) {
                    keys.remove(&*key);
                }
            }

            // Remove key from entity index.
            if let Some((ref et, ref id)) = value.entity_ref {
                if let Some(by_type) = ei.get(et) {
                    if let Some(keys) = by_type.get(id) {
                        keys.remove(&*key);
                    }
                }
            }
        })
        .build()
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
        assert!(config.max_entries > 0, "max_entries must be > 0");

        let memory_bytes = Arc::new(AtomicUsize::new(0));
        let view_index: Arc<DashMap<String, DashSet<u64>>> = Arc::new(DashMap::new());
        let entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>> =
            Arc::new(DashMap::new());

        let store = build_store(
            &config,
            Arc::clone(&memory_bytes),
            Arc::clone(&view_index),
            Arc::clone(&entity_index),
        );

        Self {
            store,
            config,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            memory_bytes,
            view_index,
            entity_index,
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

    /// Look up a cached result by its cache key.
    ///
    /// Returns `None` when caching is disabled or the key is not present or expired.
    /// Moka handles TTL expiry internally — if `get()` returns `Some`, the entry is live.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn get(&self, cache_key: u64) -> Result<Option<Arc<Vec<JsonbValue>>>> {
        if !self.config.enabled {
            return Ok(None);
        }

        // moka::sync::Cache::get() is lock-free on the read path.
        if let Some(cached) = self.store.get(&cache_key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Ok(Some(Arc::clone(&cached.result)))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            Ok(None)
        }
    }

    /// Store query result in cache, accepting an already-`Arc`-wrapped result.
    ///
    /// Preferred over [`put`](Self::put) on the hot miss path: callers that already
    /// hold an `Arc<Vec<JsonbValue>>` (e.g. `CachedDatabaseAdapter`) can store it
    /// without an extra `Vec` clone.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (from `generate_cache_key()`)
    /// * `result` - Arc-wrapped query result to cache
    /// * `accessed_views` - List of views accessed by this query
    /// * `ttl_override` - Per-entry TTL in seconds; `None` uses `CacheConfig::ttl_seconds`
    /// * `entity_type` - Optional GraphQL type name for entity-ID indexing
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn put_arc(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let ttl_seconds = ttl_override.unwrap_or(self.config.ttl_seconds);

        // TTL=0 means "no time-based expiry" — store the entry and rely entirely
        // on mutation-based invalidation.  expire_after_create returns None for
        // these entries so moka never schedules a timer-wheel eviction.

        // Respect cache_list_queries: a result with more than one row is considered a list.
        if !self.config.cache_list_queries && result.len() > 1 {
            return Ok(());
        }

        // Enforce per-entry size limit: estimate entry size from serialized JSON.
        if let Some(max_entry) = self.config.max_entry_bytes {
            let estimated = serde_json::to_vec(&*result).map_or(0, |v| v.len());
            if estimated > max_entry {
                return Ok(()); // silently skip oversized entries
            }
        }

        // Enforce total cache size limit.
        if let Some(max_total) = self.config.max_total_bytes {
            if self.memory_bytes.load(Ordering::Relaxed) >= max_total {
                return Ok(()); // silently skip when budget is exhausted
            }
        }

        // Extract entity reference outside the hot path.
        let entity_id: Option<String> = entity_type.and_then(|_et| {
            result
                .first()
                .and_then(|row| row.as_value().as_object()?.get("id")?.as_str().map(str::to_string))
        });

        let entity_ref =
            entity_type.zip(entity_id.as_deref()).map(|(et, id)| (et.to_string(), id.to_string()));

        // Register in view index.
        for view in &accessed_views {
            self.view_index.entry(view.clone()).or_default().insert(cache_key);
        }

        // Register in entity index.
        if let Some((ref et, ref id)) = entity_ref {
            self.entity_index
                .entry(et.clone())
                .or_default()
                .entry(id.clone())
                .or_default()
                .insert(cache_key);
        }

        let cached = CachedResult {
            result,
            accessed_views: accessed_views.into_boxed_slice(),
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            ttl_seconds,
            entity_ref,
        };

        self.memory_bytes.fetch_add(entry_overhead(), Ordering::Relaxed);
        // Wrap in Arc so moka's get() costs one atomic increment, not a full clone.
        self.store.insert(cache_key, Arc::new(cached));
        self.total_cached.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Store query result in cache.
    ///
    /// If caching is disabled, this is a no-op.
    ///
    /// Wraps `result` in an `Arc` and delegates to [`put_arc`](Self::put_arc).
    /// Prefer [`put_arc`](Self::put_arc) when the caller already holds an `Arc`.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (from `generate_cache_key()`)
    /// * `result` - Query result to cache
    /// * `accessed_views` - List of views accessed by this query
    /// * `ttl_override` - Per-entry TTL in seconds; `None` uses `CacheConfig::ttl_seconds`
    /// * `entity_type` - Optional GraphQL type name (e.g. `"User"`) for entity-ID indexing.
    ///   When provided, each row's `"id"` field is extracted and stored in `entity_index`
    ///   so that `invalidate_by_entity()` can perform selective eviction.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
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
        self.put_arc(cache_key, Arc::new(result), accessed_views, ttl_override, entity_type)
    }

    /// Invalidate entries accessing specified views.
    ///
    /// Uses the `view_index` for O(k) lookup instead of O(n) full-cache scan.
    /// Keys accessing multiple views in `views` are deduplicated before invalidation.
    ///
    /// # Arguments
    ///
    /// * `views` - List of view/table names modified by mutation
    ///
    /// # Returns
    ///
    /// Number of cache entries invalidated.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
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
        if !self.config.enabled {
            return Ok(0);
        }

        // Collect keys first (releases DashMap guards) then invalidate.
        // Moka's eviction listener fires synchronously on the calling thread, so
        // we must NOT hold any DashMap shard guard when calling store.invalidate() —
        // the listener itself calls view_index.get() on the same shard, which
        // would deadlock on a non-re-entrant parking_lot::RwLock.
        let mut keys_to_invalidate: HashSet<u64> = HashSet::new();
        for view in views {
            if let Some(keys) = self.view_index.get(view) {
                // Dedup: a query accessing multiple views in `views` would
                // otherwise be counted and invalidated once per view.
                for key in keys.iter() {
                    keys_to_invalidate.insert(*key);
                }
            }
            // Guard dropped here — safe to proceed
        }

        #[allow(clippy::cast_possible_truncation)]
        // Reason: entry count never exceeds u64
        let count = keys_to_invalidate.len() as u64;

        for key in keys_to_invalidate {
            self.store.invalidate(&key);
            // Index cleanup handled by eviction listener.
        }

        self.invalidations.fetch_add(count, Ordering::Relaxed);
        Ok(count)
    }

    /// Evict cache entries that contain a specific entity UUID.
    ///
    /// Uses the `entity_index` for O(k) lookup. Entries not referencing this
    /// entity are left untouched.
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
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        // Collect keys first (releases DashMap guards) then invalidate.
        // Moka's eviction listener fires synchronously on the calling thread, so
        // we must NOT hold any DashMap shard guard when calling store.invalidate() —
        // the listener itself calls entity_index.get() on the same shard, which
        // would deadlock on a non-re-entrant parking_lot::RwLock.
        let keys_to_invalidate: Vec<u64> = self
            .entity_index
            .get(entity_type)
            .and_then(|by_type| {
                by_type.get(entity_id).map(|keys| keys.iter().map(|k| *k).collect())
            })
            .unwrap_or_default();

        #[allow(clippy::cast_possible_truncation)]
        // Reason: entry count never exceeds u64
        let count = keys_to_invalidate.len() as u64;

        for key in keys_to_invalidate {
            self.store.invalidate(&key);
            // Index cleanup handled by eviction listener.
        }

        self.invalidations.fetch_add(count, Ordering::Relaxed);
        Ok(count)
    }

    /// Get cache metrics snapshot.
    ///
    /// Returns a consistent snapshot of current counters. Individual fields may
    /// be updated independently (atomics), so the snapshot is not a single atomic
    /// transaction, but is accurate enough for monitoring.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
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
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            total_cached: self.total_cached.load(Ordering::Relaxed),
            invalidations: self.invalidations.load(Ordering::Relaxed),
            #[allow(clippy::cast_possible_truncation)]
            // Reason: entry count fits in usize on any 64-bit target
            size: self.store.entry_count() as usize,
            memory_bytes: self.memory_bytes.load(Ordering::Relaxed),
        })
    }

    /// Clear all cache entries.
    ///
    /// Resets the store, reverse indexes, and `memory_bytes` synchronously.
    /// The eviction listener will still fire asynchronously for each evicted entry,
    /// but its index-cleanup operations will be no-ops on the already-cleared maps.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
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
        self.store.invalidate_all();
        // Reset indexes and memory counter synchronously — don't rely on the
        // async eviction listener to do this.
        self.view_index.clear();
        self.entity_index.clear();
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
        #[allow(clippy::cast_precision_loss)]
        // Reason: hit-rate is a display metric; f64 precision loss on u64 counters is acceptable
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
        cache.put(1_u64, result, vec!["v_user".to_string()], None, None).unwrap();

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

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

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
            ttl_seconds: 1,
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));
        cache.store.run_pending_tasks();

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
        cache.store.run_pending_tasks();

        let result = cache.get(1_u64).unwrap();
        assert!(result.is_none(), "Entry with per-entry TTL=1s should have expired");
    }

    #[test]
    fn test_per_entry_ttl_zero_cached_indefinitely() {
        // TTL=0 = no time-based expiry; entry lives until mutation invalidation.
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_live".to_string()], Some(0), None)
            .unwrap();

        let result = cache.get(1_u64).unwrap();
        assert!(result.is_some(), "Entry with TTL=0 should be cached indefinitely");
    }

    #[test]
    fn test_ttl_not_expired() {
        let config = CacheConfig {
            ttl_seconds: 3600, // 1 hour TTL
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Should still be valid
        let result = cache.get(1_u64).unwrap();
        assert!(result.is_some(), "Entry should not be expired");
    }

    // ========================================================================
    // Eviction Tests (capacity-based)
    // ========================================================================

    #[test]
    fn test_capacity_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        // Add 3 entries (max is 2); moka will evict one
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(3_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Run pending tasks to flush evictions
        cache.store.run_pending_tasks();

        let metrics = cache.metrics().unwrap();
        assert!(metrics.size <= 2, "Cache size should not exceed max capacity");
    }

    // ========================================================================
    // Cache Disabled Tests
    // ========================================================================

    #[test]
    fn test_cache_disabled() {
        let config = CacheConfig::disabled();
        let cache = QueryResultCache::new(config);

        // Put should be no-op
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

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

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();

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

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();
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

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

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

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();

        cache.clear().unwrap();

        // Run pending tasks to flush moka's eviction pipeline
        cache.store.run_pending_tasks();

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
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Hit
        cache.get(1_u64).unwrap();

        // moka::sync::Cache entry_count() is eventually consistent — flush pending
        // write operations before asserting on size.
        cache.store.run_pending_tasks();

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
        vec![JsonbValue::new(serde_json::json!({"id": id, "name": "test"}))]
    }

    #[test]
    fn test_invalidate_by_entity_only_removes_matching_entries() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache User A and User B as separate entries
        cache
            .put(1_u64, entity_result("uuid-a"), vec!["v_user".to_string()], None, Some("User"))
            .unwrap();
        cache
            .put(2_u64, entity_result("uuid-b"), vec!["v_user".to_string()], None, Some("User"))
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

        // Cache a single-entity entry (entity_ref uses first row's id)
        cache
            .put(
                1_u64,
                entity_result("uuid-a"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();

        // Invalidate by User A
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "Entry for A should be evicted");
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

        cache
            .put(
                1_u64,
                entity_result("uuid-1"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
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
            enabled: true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Two-row result: must be skipped (killed by > → >= mutant)
        let two_rows = vec![
            JsonbValue::new(json!({"id": 1})),
            JsonbValue::new(json!({"id": 2})),
        ];
        cache.put(1_u64, two_rows, vec!["v_user".to_string()], None, None).unwrap();
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
            enabled: true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // One-row result: must be stored
        let one_row = vec![JsonbValue::new(json!({"id": 1}))];
        cache.put(1_u64, one_row, vec!["v_user".to_string()], None, None).unwrap();
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
            enabled: true,
            max_entry_bytes: Some(10), // 10 bytes — smaller than any JSON row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // A typical row serialises to far more than 10 bytes
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        assert!(cache.get(1_u64).unwrap().is_none(), "oversized entry must be silently skipped");
    }

    /// Sentinel: entries within `max_entry_bytes` must be stored normally.
    ///
    /// Complements the above to pin both sides of the size boundary.
    #[test]
    fn test_max_entry_bytes_allows_small_entry() {
        let config = CacheConfig {
            enabled: true,
            max_entry_bytes: Some(100_000), // 100 KB — plenty for a test row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
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
            enabled: true,
            max_total_bytes: Some(0), // 0 bytes — always exhausted
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "entry must be skipped when max_total_bytes budget is already exhausted"
        );
    }

    // ========================================================================
    // Cross-key invalidation Tests (replaces cross-shard tests)
    // ========================================================================

    /// `invalidate_views` clears matching entries regardless of cache key.
    #[test]
    fn test_cross_key_view_invalidation() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Insert many entries
        for i in 0_u64..200 {
            let view = if i % 2 == 0 { "v_user" } else { "v_post" };
            cache.put(i, test_result(), vec![view.to_string()], None, None).unwrap();
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

    /// Cross-key entity invalidation works across all cache keys.
    #[test]
    fn test_cross_key_entity_invalidation() {
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

    /// Clear works for all entries.
    #[test]
    fn test_clear_all() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        for i in 0_u64..200 {
            cache.put(i, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        }

        cache.clear().unwrap();
        cache.store.run_pending_tasks();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);

        for i in 0_u64..200 {
            assert!(cache.get(i).unwrap().is_none());
        }
    }

    /// `memory_bytes` is tracked and reported via `metrics()`.
    #[test]
    fn test_memory_bytes_tracked() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v".to_string()], None, None).unwrap();

        let before = cache.metrics().unwrap().memory_bytes;
        assert!(before > 0, "memory_bytes should be tracked");
    }

    /// `memory_bytes` decreases after invalidation (synchronously via clear).
    #[test]
    fn test_memory_bytes_decreases_on_clear() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        let before = cache.metrics().unwrap().memory_bytes;
        assert!(before > 0);

        cache.clear().unwrap();

        let after = cache.metrics().unwrap().memory_bytes;
        assert_eq!(after, 0, "memory_bytes should be zero after clear()");
    }

    // ========================================================================
    // Concurrency regression test (#185)
    // ========================================================================

    /// Regression guard for #185: LRU+Mutex serialized all hot-key reads through
    /// one shard's mutex. With moka, reads are lock-free and should scale near-
    /// linearly with thread count.
    #[test]
    #[ignore = "wall-clock dependent — run manually to confirm lock-free read scaling"]
    fn test_concurrent_reads_do_not_serialize() {
        const ITERS: usize = 10_000;
        let config = CacheConfig::enabled();
        let cache = Arc::new(QueryResultCache::new(config));
        let key = 42_u64;
        cache.put(key, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Single-threaded baseline
        let start = std::time::Instant::now();
        for _ in 0..ITERS {
            let _ = cache.get(key).unwrap();
        }
        let single_elapsed = start.elapsed();

        // 40-thread concurrent
        let start = std::time::Instant::now();
        let handles: Vec<_> = (0..40)
            .map(|_| {
                let c = Arc::clone(&cache);
                std::thread::spawn(move || {
                    for _ in 0..ITERS {
                        let _ = c.get(key).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let multi_elapsed = start.elapsed();

        // 40× the work in ≤2× the time → near-linear scaling.
        // Under old LRU+Mutex, 40-thread took ~20-40× single-thread time.
        assert!(
            multi_elapsed <= single_elapsed * 2,
            "40-thread ({:?}) was more than 2× single-thread ({:?}) — suggests serialization",
            multi_elapsed,
            single_elapsed,
        );
    }
}
