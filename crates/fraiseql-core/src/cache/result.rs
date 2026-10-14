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
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use fraiseql_db::ViewName;
use moka::sync::Cache as MokaCache;
use serde::{Deserialize, Serialize};

use super::{config::CacheConfig, fence::InvalidationFence, tviews_feed::Collection};
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

    /// Views of the levels of a composed read tagged by collection (#1589), indexed apart
    /// from [`accessed_views`](Self::accessed_views): see [`ComposedTags`].
    pub collected_views: Box<[ViewName]>,

    /// The collections a composed read composes, each with its parent's id (#1589).
    pub collections: Box<[(Collection, String)]>,

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

/// Reverse index: child type → `(column, parent id)` → set of `(cache key, epoch)` (#1589).
type CollectionIndex = DashMap<String, DashMap<(String, String), DashSet<IndexRef>>>;

/// What a composed read's entry is indexed under besides its views and entity (#1589).
///
/// A level composing the comments of each post it reads is stale when a comment joins,
/// leaves or changes in one of those posts' collections, which a row tag cannot catch (the
/// new comment was never in the result). Such a level is tagged by collection and parent,
/// and its view goes to [`collected_views`](Self::collected_views) rather than to the
/// entry's views.
#[derive(Debug, Clone, Default)]
pub struct ComposedTags {
    /// Views of the levels tagged by collection. A write FraiseQL ran sweeps them like any
    /// view ([`QueryResultCache::invalidate_views`]); a change feed that names collections
    /// does not ([`QueryResultCache::invalidate_views_except_collections`]).
    pub collected_views: Vec<String>,
    /// Each collection the entry composes, with the parent's id.
    pub collections:     Vec<(Collection, String)>,
    /// The `(type, id)` of every row of every level, as [`CachedResult::entity_refs`] holds a
    /// flat read's: an eviction by entity reaches the entry whichever level holds the row.
    pub entities:        Vec<(String, String)>,
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

    /// Reverse index: view of a level tagged by collection → set of `(cache key, epoch)`
    /// (#1589). Swept by [`invalidate_views`](Self::invalidate_views), not by
    /// [`invalidate_views_except_collections`](Self::invalidate_views_except_collections).
    collected_view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>>,

    /// Reverse index of the collections composed reads compose (#1589).
    collection_index: Arc<CollectionIndex>,

    /// Source of [`CachedResult::epoch`] values. Monotonic for the process
    /// lifetime; wrap-around at 2^64 puts is not reachable.
    next_epoch: AtomicU64,

    /// Refuses a fill whose read straddled an invalidation that reaches it (#1079), and
    /// only those (#1584). See [`InvalidationFence`].
    ///
    /// Distinct from [`next_epoch`](Self::next_epoch), which identifies one entry instance
    /// so the eviction listener does not deregister a replacement (#740).
    fence: InvalidationFence,

    /// While set, nothing is stored (#1577): the server cannot hear other servers' writes,
    /// so it cannot know which of its entries they made stale.
    suspended: AtomicBool,
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
    collected_view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>>,
    collection_index: Arc<CollectionIndex>,
) -> MokaCache<u64, Arc<CachedResult>> {
    let max_cap = config.max_entries as u64;
    let mb = memory_bytes;
    let vi = view_index;
    let ei = entity_index;
    let cvi = collected_view_index;
    let ci = collection_index;

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

            // And the composed read's collection tags (#1589).
            for view in &value.collected_views {
                if let Some(keys) = cvi.get(view) {
                    keys.remove(&reference);
                }
            }
            for (collection, parent) in &*value.collections {
                if let Some(by_type) = ci.get(&collection.child_type) {
                    if let Some(keys) = by_type.get(&(collection.column.clone(), parent.clone())) {
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
        let collected_view_index: Arc<DashMap<ViewName, DashSet<IndexRef>>> =
            Arc::new(DashMap::new());
        let collection_index: Arc<CollectionIndex> = Arc::new(DashMap::new());

        let store = build_store(
            &config,
            Arc::clone(&memory_bytes),
            Arc::clone(&view_index),
            Arc::clone(&entity_index),
            Arc::clone(&collected_view_index),
            Arc::clone(&collection_index),
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
            collected_view_index,
            collection_index,
            next_epoch: AtomicU64::new(0),
            fence: InvalidationFence::new(),
            suspended: AtomicBool::new(false),
        }
    }

    /// Snapshot of the invalidation sequence, for fencing a cache write (#1079).
    ///
    /// Take this **before** the database round trip and hand it to
    /// [`put_arc`](Self::put_arc). If an invalidation that reaches the entry (one of its
    /// views, its entity type, or a `clear`) lands while the read is in flight, the write is
    /// refused — the rows are still returned to the caller, they are simply not cached. An
    /// invalidation that cannot reach it does not refuse it (#1584).
    #[must_use]
    pub fn invalidation_generation(&self) -> u64 {
        self.fence.snapshot()
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
        self.put_arc_with(
            cache_key,
            result,
            accessed_views,
            ttl_override,
            entity_type,
            fence,
            || {},
            || {},
        )
    }

    /// [`put_arc`](Self::put_arc), running `before_insert` after the pre-check and before the
    /// entry reaches the store (the window an invalidation can land in unseen), and
    /// `after_insert` once it is in the store, before the authoritative check. Tests use them
    /// to put an invalidation, or a concurrent reader, exactly there.
    #[allow(clippy::too_many_arguments)] // Reason: put_arc's arguments plus the test seam
    pub(super) fn put_arc_with(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        fence: Option<u64>,
        before_insert: impl FnOnce(),
        after_insert: impl FnOnce(),
    ) -> Result<()> {
        self.put_tagged(
            cache_key,
            result,
            accessed_views,
            ttl_override,
            entity_type,
            ComposedTags::default(),
            fence,
            before_insert,
            after_insert,
        )
    }

    /// [`put_arc_with`](Self::put_arc_with), indexed under `tags` too.
    #[allow(clippy::too_many_arguments)] // Reason: put_arc_with's arguments plus the tags
    fn put_tagged(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        tags: ComposedTags,
        fence: Option<u64>,
        before_insert: impl FnOnce(),
        after_insert: impl FnOnce(),
    ) -> Result<()> {
        if !self.config.enabled || self.suspended.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Cheap pre-check: if an invalidation reaching this fill already landed, do no work
        // at all. The authoritative check is after the insert, below — this one saves the
        // serialisation and index churn, and keeps the common straddle from ever being
        // visible in the store.
        if fence.is_some_and(|snapshot| {
            self.fence.moved_since(
                snapshot,
                accessed_views.iter().chain(&tags.collected_views).map(String::as_str),
                entity_type.into_iter().chain(tags.entities.iter().map(|(t, _)| t.as_str())),
                tags.collections.iter().map(|(c, _)| c.child_type.as_str()),
            )
        }) {
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
                .chain(tags.entities.iter().cloned())
                .collect::<Vec<_>>()
                .into_boxed_slice()
        } else {
            tags.entities.clone().into_boxed_slice()
        };

        // Promote owned `String` view names into `ViewName(Arc<str>)` exactly
        // once. The same Arc is then shared by `view_index` and
        // `accessed_views` (the slice stored on the cached entry).
        let accessed_views: Box<[ViewName]> =
            accessed_views.into_iter().map(ViewName::from).collect();
        let collected_views: Box<[ViewName]> =
            tags.collected_views.into_iter().map(ViewName::from).collect();
        let collections: Box<[(Collection, String)]> = tags.collections.into_boxed_slice();

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

        // Register the composed read's collection tags (#1589).
        for view in &collected_views {
            self.collected_view_index.entry(view.clone()).or_default().insert(reference);
        }
        for (collection, parent) in &*collections {
            self.collection_index
                .entry(collection.child_type.clone())
                .or_default()
                .entry((collection.column.clone(), parent.clone()))
                .or_default()
                .insert(reference);
        }

        let cached = CachedResult {
            result,
            accessed_views,
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            ttl_seconds,
            entity_refs,
            collected_views,
            collections,
            epoch,
        };

        before_insert();

        self.memory_bytes.fetch_add(entry_overhead(), Ordering::Relaxed);
        // Wrap in Arc so moka's get() costs one atomic increment, not a full clone.
        let entry = Arc::new(cached);
        self.store.insert(cache_key, Arc::clone(&entry));
        after_insert();

        // ── The fence (#1079) ───────────────────────────────────────────────
        //
        // Authoritative check, AFTER the insert. #740's "register before insert" makes the
        // key visible to a concurrent invalidation, but one landing between the pre-check
        // and the insert collects a key that is not stored yet and evicts nothing. Checked
        // here, either this check sees its stamp and the fill withdraws itself, or the
        // insert is ordered before the invalidation's key collection, which evicts it
        // (`fence.rs`, Ordering). The eviction listener deregisters the withdrawn entry.
        // No DashMap guard is held here: the listener re-enters `view_index`.
        if fence.is_some_and(|snapshot| {
            self.fence.moved_since_inserted(
                snapshot,
                entry.accessed_views.iter().chain(&*entry.collected_views).map(ViewName::as_str),
                entry
                    .entity_refs
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .chain(entity_type)
                    .collect::<std::collections::BTreeSet<_>>(),
                entry.collections.iter().map(|(c, _)| c.child_type.as_str()),
            )
        }) {
            self.store.invalidate(&cache_key);
            return Ok(());
        }
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

    /// Store a composed read's rows, indexed under `accessed_views`, `entity_type` and
    /// `tags` (#1589). Otherwise [`put_arc`](Self::put_arc).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    #[allow(clippy::too_many_arguments)] // Reason: put_arc's arguments plus the tags
    pub fn put_composed(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        tags: ComposedTags,
        fence: Option<u64>,
    ) -> Result<()> {
        self.put_tagged(
            cache_key,
            result,
            accessed_views,
            ttl_override,
            entity_type,
            tags,
            fence,
            || {},
            || {},
        )
    }

    /// Evict the entries composing `collection` of the parent `parent_id` (#1589).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_collection(&self, collection: &Collection, parent_id: &str) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }
        // Stamp before collecting keys (#1079), as invalidate_by_entity does.
        self.fence.stamp_collection_type(&collection.child_type);
        // Collect first: the eviction listener re-enters the index (see invalidate_views).
        let keys: HashSet<u64> = self
            .collection_index
            .get(&collection.child_type)
            .and_then(|by_type| {
                by_type
                    .get(&(collection.column.clone(), parent_id.to_string()))
                    .map(|refs| refs.iter().map(|r| r.0).collect())
            })
            .unwrap_or_default();
        Ok(self.evict(keys))
    }

    /// Evict the entries composing any collection of `child_type` (#1589).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_collections_of(&self, child_type: &str) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }
        self.fence.stamp_collection_type(child_type);
        let keys: HashSet<u64> = self
            .collection_index
            .get(child_type)
            .map(|by_type| {
                by_type
                    .iter()
                    .flat_map(|refs| refs.iter().map(|r| r.0).collect::<Vec<_>>())
                    .collect()
            })
            .unwrap_or_default();
        Ok(self.evict(keys))
    }

    /// Evict `keys`, counting them as invalidations. No index guard may be held: the eviction
    /// listener re-enters the indexes.
    fn evict(&self, keys: HashSet<u64>) -> u64 {
        #[allow(clippy::cast_possible_truncation)]
        // Reason: entry count never exceeds u64
        let count = keys.len() as u64;
        for key in keys {
            self.store.invalidate(&key);
        }
        self.invalidations.fetch_add(count, Ordering::Relaxed);
        count
    }

    /// [`invalidate_views`](Self::invalidate_views), but not by the views of levels tagged by
    /// collection (#1589).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_views_except_collections(&self, views: &[ViewName]) -> Result<u64> {
        self.sweep_views(views, false)
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
        self.sweep_views(views, true)
    }

    /// Evict the entries indexed under `views`; with `collected`, also those whose levels
    /// tagged by collection read one of them (#1589).
    fn sweep_views(&self, views: &[ViewName], collected: bool) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        // Stamp BEFORE collecting keys (#1079). A read whose database round trip
        // straddles this call must observe the stamp even if it snapshotted a moment
        // ago — stamping first means the window can only ever be too wide (a refused
        // cache write), never too narrow (a stale entry). Only these views' fills are
        // fenced (#1584).
        self.fence.stamp_views(views.iter().map(ViewName::as_str));

        // Collect keys first (releases DashMap guards) then invalidate.
        // Moka's eviction listener fires synchronously on the calling thread, so
        // we must NOT hold any DashMap shard guard when calling store.invalidate() —
        // the listener itself calls view_index.get() on the same shard, which
        // would deadlock on a non-re-entrant parking_lot::RwLock.
        let mut keys_to_invalidate: HashSet<u64> = HashSet::new();
        for view in views {
            // ViewName implements Borrow<str>, so DashMap lookup by &str works
            // without materialising a fresh ViewName.
            let indexes = std::iter::once(&self.view_index)
                .chain(collected.then_some(&self.collected_view_index));
            for index in indexes {
                if let Some(keys) = index.get(view.as_str()) {
                    // Dedup: a query accessing multiple views in `views` would
                    // otherwise be counted and invalidated once per view, and a
                    // key mid-replacement is registered under two epochs.
                    for reference in keys.iter() {
                        keys_to_invalidate.insert(reference.0);
                    }
                }
                // Guard dropped here — safe to proceed
            }
        }

        // Index cleanup handled by eviction listener.
        Ok(self.evict(keys_to_invalidate))
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

        // Stamp BEFORE collecting keys (#1079) — see invalidate_views. Only fills that
        // index this entity type can hold the entity (#1584).
        self.fence.stamp_entity_type(entity_type);

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

        // Index cleanup handled by eviction listener.
        Ok(self.evict(keys_to_invalidate))
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
        // Stamp first (#1079): a read in flight across a clear must not repopulate it.
        self.fence.stamp_everything();
        self.store.invalidate_all();
        // Reset indexes and memory counter synchronously — don't rely on the
        // async eviction listener to do this.
        self.view_index.clear();
        self.entity_index.clear();
        self.collected_view_index.clear();
        self.collection_index.clear();
        self.memory_bytes.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Clear every entry, and store nothing until [`resume`](Self::resume) (#1577).
    ///
    /// For a server that can no longer hear other servers' writes. A fill already past the
    /// suspension check when it lands is refused by the clear's fence.
    pub fn suspend(&self) {
        self.suspended.store(true, Ordering::SeqCst);
        let _ = self.clear();
    }

    /// Store again, from a cleared cache (#1577).
    ///
    /// Cleared **after** storing is allowed again: the clear's fence refuses every fill whose
    /// read began before it, which includes every read made while suspended.
    pub fn resume(&self) {
        self.suspended.store(false, Ordering::SeqCst);
        let _ = self.clear();
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

#[cfg(test)]
#[path = "result_collections_tests.rs"]
mod result_collections_tests;
