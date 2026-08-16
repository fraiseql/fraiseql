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
//! view_index:   DashMap<view_name,   DashSet<(cache_key, epoch)>>
//! entity_index: DashMap<entity_type, DashMap<entity_id, DashSet<(cache_key, epoch)>>>
//! ```
//!
//! Indexes are populated in `put()` and pruned via moka's eviction listener.
//! `clear()` resets all indexes synchronously.
//!
//! ### Why the indexes key on `(cache_key, epoch)` and not on `cache_key` alone (#740)
//!
//! `put_arc()` registers the key in the reverse indexes *before* `store.insert()`,
//! deliberately: an `invalidate_views()` racing the insert must not miss the key.
//! The consequence is that moka fires the eviction listener for the entry the insert
//! just displaced — while the replacement is already live under the same key. A
//! listener that pruned by `cache_key` would delete the registrations the *live*
//! entry depends on, detaching it from every invalidation path: it would then be
//! served until TTL expiry, or forever for the `cache_ttl_seconds = 0` entries
//! documented as "mutation-invalidated only".
//!
//! Each cached entry therefore carries a process-unique [`CachedResult::epoch`] and
//! registers/deregisters itself under `(cache_key, epoch)`. Registration and removal
//! are symmetric per *entry instance* rather than per key, so the listener needs no
//! knowledge of moka's [`moka::notification::RemovalCause`] taxonomy — `Replaced`,
//! `Expired`, `Size` and `Explicit` are all handled by the same arithmetic. Keep it
//! that way: a cause-inspecting listener is only as correct as the exact set of
//! causes the current moka version reports for a displaced entry.

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

    /// Process-unique identity of this cached *entry instance*.
    ///
    /// Distinguishes an entry from its own replacement under the same cache key.
    /// The reverse indexes register `(cache_key, epoch)` pairs so the eviction
    /// listener can deregister exactly the instance being evicted and never the
    /// live one that displaced it — see the module docs (#740).
    pub epoch: u64,
}

/// A reverse-index registration: the cache key plus the epoch of the entry
/// instance that registered it.
type IndexRef = (u64, u64);

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
/// let fence = cache.invalidation_generation(); // snapshot before the read (#1079)
/// cache.put(
///     12345_u64,
///     result.clone(),
///     vec!["v_user".to_string()],
///     None,        // use global TTL
///     None,        // no entity type index
///     Some(fence), // discard the write if an invalidation raced the read
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

    /// Reverse index: view name → set of `(cache key, epoch)` accessing that view.
    ///
    /// Keys are [`ViewName`] (`Arc<str>` inside) so inserts share the same
    /// allocation as the names stored in [`CachedResult::accessed_views`].
    /// Lookup by `&str` still works via the `Borrow<str>` impl on `ViewName`.
    view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>>,

    /// Reverse index: entity type → entity id → set of `(cache key, epoch)`.
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<IndexRef>>>>,

    /// Source of [`CachedResult::epoch`] values. Monotonic for the process
    /// lifetime; wrap-around at 2^64 puts is not reachable.
    next_epoch: AtomicU64,

    /// Bumped by every invalidation, so a read that straddled one can tell (#1079).
    ///
    /// A read is `get` → miss → **await the database** → `put`. An invalidation landing
    /// inside that await evicts nothing (the key is not in `view_index` yet) and the `put`
    /// then stores rows fetched *before* the mutation committed — the client that just
    /// wrote sees its own write vanish on the next read.
    ///
    /// The counter is **global**, not per view: a skipped cache write costs one uncached
    /// read, whereas a stale entry costs correctness, and a per-view map would inherit
    /// every lifetime question `view_index` already has. If a benchmark ever shows the
    /// hit-rate loss matters, refine it then — with a measurement, not a guess.
    ///
    /// Distinct from [`next_epoch`](Self::next_epoch), which identifies one entry instance
    /// so the eviction listener does not deregister a replacement (#740).
    invalidation_generation: AtomicU64,
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
    view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>>,
    entity_index: Arc<DashMap<String, DashMap<String, DashSet<IndexRef>>>>,
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

            // Deregister exactly THIS entry instance. `_cause` is deliberately
            // unused: `(key, epoch)` already distinguishes a displaced entry from
            // the live replacement that displaced it, so `Replaced` needs no
            // special case and no future moka cause can detach a live entry (#740).
            let reference: IndexRef = (*key, value.epoch);

            // Remove this instance from the view index.
            for view in &value.accessed_views {
                if let Some(keys) = vi.get(view) {
                    keys.remove(&reference);
                }
            }

            // Remove ALL entity_refs from entity index.
            for (et, id) in &*value.entity_refs {
                if let Some(by_type) = ei.get(et) {
                    if let Some(keys) = by_type.get(id) {
                        keys.remove(&reference);
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
        let view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>> = Arc::new(DashMap::new());
        let entity_index: Arc<DashMap<String, DashMap<String, DashSet<IndexRef>>>> =
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
            next_epoch: AtomicU64::new(0),
            invalidation_generation: AtomicU64::new(0),
        }
    }

    /// Snapshot of the invalidation generation, for fencing a cache write (#1079).
    ///
    /// Take this **before** the database round trip and hand it to
    /// [`put_arc`](Self::put_arc). If any invalidation lands while the read is in flight,
    /// the counter moves and the write is refused — the rows are still returned to the
    /// caller, they are simply not cached.
    #[must_use]
    pub fn invalidation_generation(&self) -> u64 {
        self.invalidation_generation.load(Ordering::Acquire)
    }

    /// Returns whether caching is enabled.
    ///
    /// Used by `CachedDatabaseAdapter` to short-circuit key generation
    /// and result clone overhead when caching is disabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// The configuration this cache was built with (immutable after creation).
    ///
    /// The admin surface reports the effective TTL and entry ceiling, which used to be
    /// hard-coded constants in the handler that happened to match a different cache's
    /// defaults (#941).
    #[must_use]
    pub const fn config(&self) -> &CacheConfig {
        &self.config
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
    /// * `fence` - Generation snapshot from
    ///   [`invalidation_generation`](Self::invalidation_generation), taken **before** the database
    ///   round trip whose rows are being stored. `None` stores unconditionally, and is only correct
    ///   when the value did not come from a read that could have raced a mutation (a test fixture,
    ///   or a synchronous re-population).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    /// A fenced-out write is **not** an error — the caller already has its rows; they are
    /// simply not cached. Turning a benign race into a request failure would be worse than
    /// the staleness this prevents.
    pub fn put_arc(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        fence: Option<u64>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Cheap pre-check: if an invalidation already landed, do no work at all. The
        // authoritative check is after registration, below — this one only saves the
        // serialisation and index churn in the common case.
        if fence.is_some_and(|snapshot| snapshot != self.invalidation_generation()) {
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
        // once. The same Arc is then shared by `view_index` and
        // `accessed_views` (the slice stored on the cached entry).
        let accessed_views: Box<[ViewName]> =
            accessed_views.into_iter().map(ViewName::from).collect();

        // Identity of this entry instance, so the eviction listener can
        // deregister it without touching a replacement under the same key (#740).
        let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
        let reference: IndexRef = (cache_key, epoch);

        // Register in view index.
        for view in &accessed_views {
            self.view_index.entry(view.clone()).or_default().insert(reference);
        }

        // Register ALL entity refs in entity index.
        for (et, id) in &*entity_refs {
            self.entity_index
                .entry(et.clone())
                .or_default()
                .entry(id.clone())
                .or_default()
                .insert(reference);
        }

        // ── The fence (#1079) ───────────────────────────────────────────────
        //
        // Authoritative check, AFTER registration and BEFORE `store.insert`. The
        // pre-check at the top of this function is only an optimisation; this is the
        // one that closes the window, because #740's "register before insert" makes the
        // key *visible* to a concurrent `invalidate_views` but does not stop the insert
        // that follows. An invalidation landing between the registrations above and the
        // insert below collects a key that is not in the store yet — invalidating
        // nothing — and the insert then lands stale.
        //
        // Deregistration is symmetric with the eviction listener's: the same
        // `(cache_key, epoch)` reference, removed from the same two indexes, so no index
        // row survives pointing at a key that was never stored. No moka call is made
        // while a DashMap guard is held — the listener runs synchronously on the calling
        // thread and re-enters `view_index`, which is why that rule exists.
        if fence.is_some_and(|snapshot| snapshot != self.invalidation_generation()) {
            for view in &accessed_views {
                if let Some(keys) = self.view_index.get(view) {
                    keys.remove(&reference);
                }
            }
            for (et, id) in &*entity_refs {
                if let Some(by_type) = self.entity_index.get(et) {
                    if let Some(keys) = by_type.get(id) {
                        keys.remove(&reference);
                    }
                }
            }
            return Ok(());
        }

        let cached = CachedResult {
            result,
            accessed_views,
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            ttl_seconds,
            entity_refs,
            epoch,
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
    /// let fence = cache.invalidation_generation();
    /// cache.put(0xabc123, result, vec!["v_user".to_string()], None, Some("User"), Some(fence))?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn put(
        &self,
        cache_key: u64,
        result: Vec<JsonbValue>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        fence: Option<u64>,
    ) -> Result<()> {
        self.put_arc(cache_key, Arc::new(result), accessed_views, ttl_override, entity_type, fence)
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
    /// use fraiseql_db::ViewName;
    ///
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// // After createUser mutation
    /// let invalidated = cache.invalidate_views(&[ViewName::from("v_user")])?;
    /// println!("Invalidated {} cache entries", invalidated);
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn invalidate_views(&self, views: &[ViewName]) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        // Bump BEFORE collecting keys (#1079). A read whose database round trip
        // straddles this call must observe the move even if it snapshotted a moment
        // ago — ordering the bump first means the window can only ever be too wide
        // (a refused cache write), never too narrow (a stale entry).
        self.invalidation_generation.fetch_add(1, Ordering::Release);

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
                // otherwise be counted and invalidated once per view, and a
                // key mid-replacement is registered under two epochs.
                for reference in keys.iter() {
                    keys_to_invalidate.insert(reference.0);
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

        // Bump BEFORE collecting keys (#1079) — see invalidate_views.
        self.invalidation_generation.fetch_add(1, Ordering::Release);

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
        let keys_to_invalidate: HashSet<u64> = self
            .entity_index
            .get(entity_type)
            .and_then(|by_type| {
                by_type.get(entity_id).map(|keys| keys.iter().map(|r| r.0).collect())
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
        // Bump first (#1079): a read in flight across a clear must not repopulate it.
        self.invalidation_generation.fetch_add(1, Ordering::Release);
        self.store.invalidate_all();
        // Reset indexes and memory counter synchronously — don't rely on the
        // async eviction listener to do this.
        self.view_index.clear();
        self.entity_index.clear();
        self.memory_bytes.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Flush pending background tasks in the moka store.
    ///
    /// Moka applies writes and evictions on a background schedule, so `entry_count()`
    /// lags: a `put` followed immediately by `metrics()` reports zero entries. Tests
    /// need this to synchronise before an assertion, and so does the admin stats
    /// endpoint — an operator asking how many entries are cached wants the settled
    /// number, not an estimate that reads as "nothing is cached" (#941).
    ///
    /// Not on the query path: this walks the pending write buffer.
    pub fn run_pending_tasks(&self) {
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
