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
//! - **`invalidate_views()`**: one stamp per view, O(views): nothing is collected or evicted. Each
//!   entry reading one is refused on its next hit (see `fence.rs`, names).
//! - **`invalidate_by_entity()`**: O(k) where k = matching entries (via the row index), not O(total
//!   entries).
//!
//! ## The row index
//!
//! Because `moka` does not support arbitrary iteration, invalidation by row relies on one
//! `DashMap` reverse index maintained alongside the store:
//!
//! ```text
//! rows: DashMap<RowRef, DashSet<(cache_key, epoch)>>
//! ```
//!
//! A [`RowRef`] is a row an entry holds, the row it reads by id, or a collection of one
//! parent it composes. It is populated in `put()` and pruned via moka's eviction listener,
//! which drops a row's key with its last entry. `clear()` resets it synchronously.
//!
//! ### Why the indexes key on `(cache_key, epoch)` and not on `cache_key` alone (#740)
//!
//! `put_arc()` registers the key in the reverse indexes *before* `store.insert()`,
//! deliberately: an `invalidate_by_entity()` racing the insert must not miss the key.
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

use super::{
    config::CacheConfig,
    fence::{InvalidationFence, Reach},
    tviews_feed::Collection,
};
use crate::{db::types::JsonbValue, error::Result};

/// Cached query result with metadata.
///
/// Stores the query result along with tracking information for
/// TTL expiry, view-based invalidation, and monitoring.
#[derive(Debug)]
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
    /// Used by the eviction listener to clean up the row index on eviction.
    pub entity_refs: Box<[(String, String)]>,

    /// Views of the levels of a composed read tagged by collection (#1589), indexed apart
    /// from [`accessed_views`](Self::accessed_views): see [`ComposedTags`].
    pub collected_views: Box<[ViewName]>,

    /// The collections a composed read composes, each with its parent's id (#1589).
    pub collections: Box<[(Collection, String)]>,

    /// The one row this entry reads, by its id, as `(type, id)` (#1576). Such an entry is
    /// indexed under its views apart from other entries: a full sweep of them evicts it, a
    /// write that reports the rows it changed evicts it only by that row or its type.
    pub by_id: Option<(String, String)>,

    /// Process-unique identity of this cached *entry instance*.
    ///
    /// Distinguishes an entry from its own replacement under the same cache key.
    /// The reverse indexes register `(cache_key, epoch)` pairs so the eviction
    /// listener can deregister exactly the instance being evicted and never the
    /// live one that displaced it — see the module docs (#740).
    pub epoch: u64,

    /// The invalidation sequence as it was before the read that filled this entry (#1778).
    pub snapshot: u64,

    /// The counters of the names this entry reads: it is served only while none has moved
    /// past [`snapshot`](Self::snapshot) (#1778).
    pub counters: Box<[Arc<AtomicU64>]>,

    /// Whether a hit may serve it. A fill fenced by a snapshot is stored unconfirmed, and
    /// confirmed once its post-insert check found no row eviction reaching it: one that
    /// lost that race is in the store until it withdraws itself, and must not be served
    /// in between (#1778).
    confirmed: AtomicBool,
}

/// A reverse-index registration: the cache key plus the epoch of the entry
/// instance that registered it.
type IndexRef = (u64, u64);

/// A row an entry is indexed under, and evicted by eagerly (#1778).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum RowRef {
    /// A row its result holds, `(type, id)`: evicted by entity (#1605).
    Entity(String, String),
    /// The one row it reads by id, `(type, id)` (#1576).
    ById(String, String),
    /// A collection it composes, `(child type, column, parent id)` (#1589).
    Collection(String, String, String),
}

/// Reverse index: row → set of `(cache key, epoch)`. A row no entry holds has no key.
type RowIndex = DashMap<RowRef, DashSet<IndexRef>>;

/// Every row `entry` is indexed under: what `put` registers and the eviction listener
/// deregisters, one list for both.
fn row_refs(entry: &CachedResult) -> impl Iterator<Item = RowRef> + '_ {
    let entities = entry.entity_refs.iter().map(|(t, id)| RowRef::Entity(t.clone(), id.clone()));
    let by_id = entry.by_id.iter().map(|(t, id)| RowRef::ById(t.clone(), id.clone()));
    let collections = entry.collections.iter().map(|(collection, parent)| {
        RowRef::Collection(collection.child_type.clone(), collection.column.clone(), parent.clone())
    });
    entities.chain(by_id).chain(collections)
}

/// How far a sweep of views reaches.
#[derive(Clone, Copy)]
enum Sweep {
    /// Every entry reading a view: its own views, its levels', its row's (by id).
    Full,
    /// A write that reported the rows it changed (#1576): not the reads of one row by id,
    /// which those rows evict.
    Write,
    /// A change feed naming rows and collections (#1589, #1576), or a write whose report
    /// names both (#1603): an entry's own views only.
    Feed,
}

/// What a composed read's entry is indexed under besides its views and entity (#1589).
///
/// A level composing the comments of each post it reads is stale when a comment joins,
/// leaves or changes in one of those posts' collections, which a row tag cannot catch (the
/// new comment was never in the result). Such a level is tagged by collection and parent,
/// and its view goes to [`collected_views`](Self::collected_views) rather than to the
/// entry's views.
#[derive(Debug, Clone, Default)]
pub struct ComposedTags {
    /// Views of the levels tagged by collection. A write whose report names no collection
    /// sweeps them like any view ([`QueryResultCache::invalidate_views`]); a change feed that
    /// names collections, or a write whose report does (#1603), does not
    /// ([`QueryResultCache::invalidate_views_except_collections`]).
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

    /// Reverse index: each row an entry holds, reads by id and collections → the entries
    /// holding it.
    rows: Arc<RowIndex>,

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

/// Build the moka store, wiring the eviction listener to the row index and memory counter.
fn build_store(
    config: &CacheConfig,
    memory_bytes: Arc<AtomicUsize>,
    rows: Arc<RowIndex>,
) -> MokaCache<u64, Arc<CachedResult>> {
    let max_cap = config.max_entries as u64;
    let mb = memory_bytes;

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

            for row in row_refs(&value) {
                // The `get` guard drops before `remove_if` takes the same shard. A
                // registration holds the shard while it inserts, so an emptied set is
                // removed only if nothing registered under it since.
                let emptied = rows.get(&row).is_some_and(|keys| {
                    keys.remove(&reference);
                    keys.is_empty()
                });
                if emptied {
                    rows.remove_if(&row, |_, keys| keys.is_empty());
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
        let rows: Arc<RowIndex> = Arc::new(DashMap::new());

        let store = build_store(&config, Arc::clone(&memory_bytes), Arc::clone(&rows));

        Self {
            store,
            config,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_cached: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            memory_bytes,
            rows,
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
        if let Some(cached) = self.store.get(&cache_key).filter(|cached| {
            cached.confirmed.load(Ordering::Acquire)
                && self.fence.current(cached.snapshot, &cached.counters)
        }) {
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
            None,
            fence,
            before_insert,
            after_insert,
        )
    }

    /// Store the rows of a read of one row by its id, `by_id` = `(type, id)`, indexed under
    /// that row and `accessed_views` apart from other entries (#1576). Otherwise
    /// [`put_arc`](Self::put_arc), with no entity index: the row is the tag.
    ///
    /// Only for a read whose rows depend on that one row and nothing else: a write that
    /// reports the rows it changed evicts it by them, not by its views.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn put_by_id(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        by_id: (String, String),
        fence: Option<u64>,
    ) -> Result<()> {
        self.put_by_id_with(
            cache_key,
            result,
            accessed_views,
            ttl_override,
            by_id,
            fence,
            || {},
            || {},
        )
    }

    /// [`put_by_id`](Self::put_by_id) with [`put_arc_with`](Self::put_arc_with)'s test seams.
    #[allow(clippy::too_many_arguments)] // Reason: put_by_id's arguments plus the test seam
    pub(super) fn put_by_id_with(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        by_id: (String, String),
        fence: Option<u64>,
        before_insert: impl FnOnce(),
        after_insert: impl FnOnce(),
    ) -> Result<()> {
        self.put_tagged(
            cache_key,
            result,
            accessed_views,
            ttl_override,
            None,
            ComposedTags::default(),
            Some(by_id),
            fence,
            before_insert,
            after_insert,
        )
    }

    /// [`put_arc_with`](Self::put_arc_with), indexed under `tags`, or the row `by_id`, too.
    #[allow(clippy::too_many_arguments)] // Reason: put_arc_with's arguments plus the tags
    fn put_tagged(
        &self,
        cache_key: u64,
        result: Arc<Vec<JsonbValue>>,
        accessed_views: Vec<String>,
        ttl_override: Option<u64>,
        entity_type: Option<&str>,
        tags: ComposedTags,
        by_id: Option<(String, String)>,
        fence: Option<u64>,
        before_insert: impl FnOnce(),
        after_insert: impl FnOnce(),
    ) -> Result<()> {
        if !self.config.enabled || self.suspended.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Extract entity refs from ALL rows (not just the first). A read by id is tagged by
        // its row instead (#1576). Before the fence: an eviction by entity reaches exactly
        // the fills holding its row, so these are what the fence checks (#1605).
        let entity_type = entity_type.filter(|_| by_id.is_none());
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

        let reach = Reach {
            views:           by_id
                .is_none()
                .then(|| accessed_views.iter().map(String::as_str))
                .into_iter()
                .flatten()
                .collect(),
            collected_views: tags.collected_views.iter().map(String::as_str).collect(),
            entities:        entity_refs.iter().map(|(t, id)| (t.as_str(), id.as_str())).collect(),
            collections:     tags
                .collections
                .iter()
                .map(|(c, p)| (c.child_type.as_str(), c.column.as_str(), p.as_str()))
                .collect(),
            by_id:           by_id.as_ref().map(|(t, id)| {
                ((t.as_str(), id.as_str()), accessed_views.iter().map(String::as_str).collect())
            }),
        };

        // Cheap pre-check: if an invalidation reaching this fill already landed, do no work
        // at all. The authoritative check is after the insert, below — this one saves the
        // serialisation and index churn, and keeps the common straddle from ever being
        // visible in the store.
        if fence.is_some_and(|snapshot| self.fence.moved_since(snapshot, &reach)) {
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

        // A fill with no snapshot raced nothing: its hits check the names from now on.
        let snapshot = fence.unwrap_or_else(|| self.fence.snapshot());
        let counters = self.fence.counters(&reach);

        let accessed_views: Box<[ViewName]> =
            accessed_views.into_iter().map(ViewName::from).collect();
        let collected_views: Box<[ViewName]> =
            tags.collected_views.into_iter().map(ViewName::from).collect();
        let collections: Box<[(Collection, String)]> = tags.collections.into_boxed_slice();

        // Identity of this entry instance, so the eviction listener can
        // deregister it without touching a replacement under the same key (#740).
        let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
        let reference: IndexRef = (cache_key, epoch);

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
            by_id,
            epoch,
            snapshot,
            counters,
            confirmed: AtomicBool::new(fence.is_none()),
        };

        // Register under every row it holds (#1576, #1589, #1605).
        for row in row_refs(&cached) {
            self.rows.entry(row).or_default().insert(reference);
        }

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
        // (`fence.rs`, Ordering). Until it passes the entry is unconfirmed, so a reader between
        // the insert and a withdrawal is not served it. The eviction listener deregisters
        // the withdrawn entry. No DashMap guard is held here: the listener re-enters the row
        // index.
        if fence.is_some_and(|snapshot| {
            self.fence.moved_since_inserted(
                snapshot,
                &Reach {
                    entities: entry
                        .entity_refs
                        .iter()
                        .map(|(t, id)| (t.as_str(), id.as_str()))
                        .collect(),
                    collections: entry
                        .collections
                        .iter()
                        .map(|(c, p)| (c.child_type.as_str(), c.column.as_str(), p.as_str()))
                        .collect(),
                    by_id: entry
                        .by_id
                        .as_ref()
                        .map(|(t, id)| ((t.as_str(), id.as_str()), Vec::new())),
                    ..Reach::default()
                },
            )
        }) {
            self.store.invalidate(&cache_key);
            return Ok(());
        }
        // No eviction of its rows was stamped before this check, so each one landing after
        // it collects this entry and evicts it (`fence.rs`, Ordering): it may be served.
        entry.confirmed.store(true, Ordering::Release);
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
    ///   provided, each row's `"id"` field is extracted and stored in the row index so that
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
            None,
            fence,
            || {},
            || {},
        )
    }

    /// Evict the reads of the row `id` of `type_name` by its id (#1576).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_by_id(&self, type_name: &str, id: &str) -> Result<u64> {
        if !self.config.enabled {
            return Ok(0);
        }
        // Stamp before collecting keys (#1079).
        self.fence.stamp_by_id(type_name, id);
        Ok(self.evict_row(&RowRef::ById(type_name.to_string(), id.to_string())))
    }

    /// Refuse every read of a row of `type_name` by its id from now on (#1576): each is
    /// left in the store, refused on its next hit (#1778).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_by_id_type(&self, type_name: &str) -> Result<()> {
        if self.config.enabled {
            self.fence.stamp_by_id_type(type_name);
        }
        Ok(())
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
        self.fence
            .stamp_collection(&collection.child_type, &collection.column, parent_id);
        Ok(self.evict_row(&RowRef::Collection(
            collection.child_type.clone(),
            collection.column.clone(),
            parent_id.to_string(),
        )))
    }

    /// Refuse the entries composing any collection of `child_type` from now on (#1589), on
    /// their next hit (#1778).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_collections_of(&self, child_type: &str) -> Result<()> {
        if self.config.enabled {
            self.fence.stamp_collection_type(child_type);
        }
        Ok(())
    }

    /// Evict the entries holding `row`, counting them as invalidations.
    ///
    /// Collected first: moka's eviction listener fires synchronously on the calling thread
    /// and re-enters the row index, so no shard guard may be held while evicting.
    fn evict_row(&self, row: &RowRef) -> u64 {
        let keys: HashSet<u64> = self
            .rows
            .get(row)
            .map(|refs| refs.iter().map(|r| r.0).collect())
            .unwrap_or_default();
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
    /// collection (#1589), nor the reads of one row by id (#1576): for a change feed, or a
    /// write whose report names the rows and collections it changed (#1603), which
    /// [`invalidate_collection`](Self::invalidate_collection) and
    /// [`invalidate_by_id`](Self::invalidate_by_id) evict.
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_views_except_collections(&self, views: &[ViewName]) -> Result<()> {
        self.sweep_views(views, Sweep::Feed)
    }

    /// [`invalidate_views`](Self::invalidate_views), but not the reads of one row by id: for a
    /// write that reported every row it changed, which [`invalidate_by_id`](Self::invalidate_by_id)
    /// and [`invalidate_by_id_type`](Self::invalidate_by_id_type) evict (#1576).
    ///
    /// # Errors
    ///
    /// This method is infallible. The `Result` return type is kept for API compatibility.
    pub fn invalidate_views_except_by_id(&self, views: &[ViewName]) -> Result<()> {
        self.sweep_views(views, Sweep::Write)
    }

    /// Refuse every entry reading one of `views` from now on.
    ///
    /// Nothing is evicted, so nothing is counted: each entry is left in the store and refused
    /// on its next hit, until a fill of its key replaces it or the store's capacity policy
    /// drops it (#1778). A read in flight across this call is never served either: its
    /// entry's snapshot predates the stamp.
    ///
    /// # Arguments
    ///
    /// * `views` - List of view/table names modified by mutation
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
    /// cache.invalidate_views(&[ViewName::from("v_user")])?;
    /// # Ok::<(), fraiseql_core::error::FraiseQLError>(())
    /// ```
    pub fn invalidate_views(&self, views: &[ViewName]) -> Result<()> {
        self.sweep_views(views, Sweep::Full)
    }

    /// Stamp `views`, as far as `sweep` reaches: also the entries whose levels tagged by
    /// collection read one of them (#1589), and those reading one row of one by id (#1576).
    fn sweep_views(&self, views: &[ViewName], sweep: Sweep) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let names = views.iter().map(ViewName::as_str);
        match sweep {
            Sweep::Full => self.fence.stamp_views(names),
            Sweep::Write => self.fence.stamp_write_views(names),
            Sweep::Feed => self.fence.stamp_feed_views(names),
        }
        Ok(())
    }

    /// Evict cache entries that contain a specific entity UUID.
    ///
    /// Uses the row index for O(k) lookup. Entries not referencing this
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

        // A read of the entity by its id is indexed under its row instead (#1576).
        let by_id = self.invalidate_by_id(entity_type, entity_id)?;

        // Stamp BEFORE collecting keys (#1079) — see invalidate_views. This evicts the
        // entries holding this row and no other, so it fences the fills holding it (#1605):
        // a fill of another row of the type, stored first, would have been left in place.
        self.fence.stamp_entity(entity_type, entity_id);

        Ok(by_id + self.evict_row(&RowRef::Entity(entity_type.to_string(), entity_id.to_string())))
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
        self.rows.clear();
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

    /// How many row keys the row indexes hold (#1778).
    #[cfg(test)]
    fn row_key_count(&self) -> usize {
        self.rows.len()
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

#[cfg(test)]
#[path = "result_by_id_tests.rs"]
mod result_by_id_tests;

#[cfg(test)]
#[path = "result_names_tests.rs"]
mod result_names_tests;

#[cfg(test)]
#[path = "result_rows_tests.rs"]
mod result_rows_tests;

#[cfg(test)]
#[path = "result_stress_tests.rs"]
mod result_stress_tests;
