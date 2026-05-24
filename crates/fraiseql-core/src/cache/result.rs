//! Query result caching with W-TinyLFU eviction and per-entry TTL.
//!
//! This module provides a `moka::sync::Cache`-backed store for GraphQL query results.
//! Moka uses Concurrent W-TinyLFU policy with lock-free reads — cache hits do NOT
//! acquire any shared lock, eliminating the hot-key serialisation bottleneck present
//! in the old 64-shard `parking_lot::Mutex<LruCache>` design.
//!
//! ## Performance characteristics
//!
//! - **`get()` hot path** (cache hit): lock-free frequency-counter update (thread-local ring
//!   buffer, drained lazily on writes), `Arc` clone (single atomic increment), one atomic counter
//!   bump.
//! - **`put()` path**: early-exit guards (disabled / list / size) before touching the store.
//!   Reverse-index updates use `DashMap` (fine-grained sharding, no global lock).
//! - **`metrics()`**: reads `store.entry_count()` directly — no shard scan.
//! - **`invalidate_views()` / `invalidate_by_entity()`**: O(k) where k = matching entries (via
//!   reverse indexes), not O(total entries).
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
use fraiseql_db::ViewName;
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
    /// Format: `[ViewName::from("v_user"), ViewName::from("v_post")]`
    ///
    /// Stored as a boxed slice of [`ViewName`] (each backed by `Arc<str>`)
    /// so cloning a name into the reverse index is a cheap atomic ref-count
    /// bump rather than a fresh heap allocation. Views are fixed at `put()`
    /// time and never modified.
    pub accessed_views: Box<[ViewName]>,

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

    /// Entity references for selective entity-level invalidation.
    ///
    /// Contains one `(entity_type, entity_id)` pair per row in `result` that has
    /// a valid string in its `"id"` field.  Empty for queries with no `id` column
    /// or when `put()` is called without an `entity_type`.
    /// Used by the eviction listener to clean up `entity_index` on eviction.
    pub entity_refs: Box<[(String, String)]>,

    /// True when `result.len() > 1` at put time.
    ///
    /// Used by `invalidate_list_queries()` to avoid evicting single-entity
    /// point-lookup entries on CREATE mutations.
    pub is_list_query: bool,
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
    ///
    /// Keys are [`ViewName`] (`Arc<str>` inside) so inserts share the same
    /// allocation as the names stored in [`CachedResult::accessed_views`].
    /// Lookup by `&str` still works via the `Borrow<str>` impl on `ViewName`.
    view_index: Arc<DashMap<ViewName, DashSet<u64>>>,

    /// Reverse index: entity type → entity id → set of cache keys.
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>>,

    /// Reverse index: view name → set of cache keys for list (multi-row) entries only.
    ///
    /// Populated in `put_arc()` when `result.len() > 1`. Used by
    /// `invalidate_list_queries()` for CREATE-targeted eviction that leaves
    /// point-lookup entries intact.
    list_index: Arc<DashMap<ViewName, DashSet<u64>>>,
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
    view_index: Arc<DashMap<ViewName, DashSet<u64>>>,
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>>,
    list_index: Arc<DashMap<ViewName, DashSet<u64>>>,
) -> MokaCache<u64, Arc<CachedResult>> {
    let max_cap = config.max_entries as u64;
    let mb = memory_bytes;
    let vi = view_index;
    let ei = entity_index;
    let li = list_index;

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

            // Remove key from list index (only populated for multi-row entries).
            if value.is_list_query {
                for view in &value.accessed_views {
                    if let Some(keys) = li.get(view) {
                        keys.remove(&*key);
                    }
                }
            }

            // Remove ALL entity_refs from entity index.
            for (et, id) in &*value.entity_refs {
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
        let view_index: Arc<DashMap<ViewName, DashSet<u64>>> = Arc::new(DashMap::new());
        let entity_index: Arc<DashMap<String, DashMap<String, DashSet<u64>>>> =
            Arc::new(DashMap::new());
        let list_index: Arc<DashMap<ViewName, DashSet<u64>>> = Arc::new(DashMap::new());

        let store = build_store(
            &config,
            Arc::clone(&memory_bytes),
            Arc::clone(&view_index),
            Arc::clone(&entity_index),
            Arc::clone(&list_index),
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
            list_index,
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

        let is_list_query = result.len() > 1;

        // Extract entity refs from ALL rows (not just the first).
        let entity_refs: Box<[(String, String)]> = if let Some(et) = entity_type {
            result
                .iter()
                .filter_map(|row| {
                    row.as_value()
                        .as_object()?
                        .get("id")?
                        .as_str()
                        .map(|id| (et.to_string(), id.to_string()))
                })
                .collect::<Vec<_>>()
                .into_boxed_slice()
        } else {
            Box::default()
        };

        // Promote owned `String` view names into `ViewName(Arc<str>)` exactly
        // once. The same Arc is then shared by `view_index`, `list_index`,
        // and `accessed_views` (the slice stored on the cached entry).
        let accessed_views: Box<[ViewName]> =
            accessed_views.into_iter().map(ViewName::from).collect();

        // Register in view index.
        for view in &accessed_views {
            self.view_index.entry(view.clone()).or_default().insert(cache_key);
        }

        // Register in list index (only for multi-row results).
        if is_list_query {
            for view in &accessed_views {
                self.list_index.entry(view.clone()).or_default().insert(cache_key);
            }
        }

        // Register ALL entity refs in entity index.
        for (et, id) in &*entity_refs {
            self.entity_index
                .entry(et.clone())
                .or_default()
                .entry(id.clone())
                .or_default()
                .insert(cache_key);
        }

        let cached = CachedResult {
            result,
            accessed_views,
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            ttl_seconds,
            entity_refs,
            is_list_query,
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
    /// * `entity_type` - Optional GraphQL type name (e.g. `"User"`) for entity-ID indexing. When
    ///   provided, each row's `"id"` field is extracted and stored in `entity_index` so that
    ///   `invalidate_by_entity()` can perform selective eviction.
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
            // ViewName implements Borrow<str>, so DashMap lookup by &str works
            // without materialising a fresh ViewName.
            if let Some(keys) = self.view_index.get(view.as_str()) {
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

    /// Evict only list (multi-row) cache entries for the given views.
    ///
    /// Unlike `invalidate_views()`, this method leaves single-entity point-lookup
    /// entries intact. Used for CREATE mutations: creating a new entity does not
    /// affect queries that fetch a *different* existing entity by UUID, but it
    /// does invalidate queries that return a variable-length list of entities.
    ///
    /// Uses the `list_index` for O(k) lookup.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_list_queries(&self, views: &[String]) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        let mut keys_to_invalidate: HashSet<u64> = HashSet::new();
        for view in views {
            if let Some(keys) = self.list_index.get(view.as_str()) {
                for k in keys.iter() {
                    keys_to_invalidate.insert(*k);
                }
            }
        }

        #[allow(clippy::cast_possible_truncation)]
        // Reason: entry count never exceeds u64
        let count = keys_to_invalidate.len() as u64;
        for key in keys_to_invalidate {
            self.store.invalidate(&key);
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

        // Short-circuit: if entity_type has no indexed entries, skip the DashMap
        // lookup entirely.  Covers cold-cache and write-heavy workloads where no
        // reads are cached yet.
        if !self.entity_index.contains_key(entity_type) {
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
        self.list_index.clear();
        self.memory_bytes.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Flush pending background tasks in the moka store.
    ///
    /// Used in tests to synchronise async eviction/invalidation before assertions.
    #[cfg(test)]
    pub(crate) fn run_pending_tasks(&self) {
        self.store.run_pending_tasks();
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
