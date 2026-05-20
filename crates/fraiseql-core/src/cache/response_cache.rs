//! Executor-level response cache.
//!
//! Caches the final projected GraphQL response value (after RBAC filtering,
//! projection, and envelope wrapping) to skip all redundant work on cache
//! hits for the same user + query combination.
//!
//! This is a **second cache tier** above the adapter-level row cache:
//! - Row cache: raw JSONB rows, shared across projection shapes
//! - Response cache: final `serde_json::Value`, keyed per (query + security context)
//!
//! ## Performance characteristics
//!
//! Backed by `moka::sync::Cache` (W-TinyLFU, lock-free reads). Invalidation
//! uses a `DashMap` reverse index (view name → set of keys), enabling O(k)
//! eviction without scanning the full cache.
//!
//! ## Security
//!
//! The response cache key includes a hash of the `SecurityContext` fields
//! that affect response content (`user_id`, roles, `tenant_id`, scopes,
//! attributes). Different RBAC scopes produce different cache entries.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use moka::sync::Cache as MokaCache;
use serde_json::Value;

use crate::{error::Result, security::SecurityContext};

/// Configuration for the response cache.
#[derive(Debug, Clone, Copy)]
pub struct ResponseCacheConfig {
    /// Enable the response cache.
    pub enabled: bool,

    /// Maximum number of cached responses.
    pub max_entries: usize,

    /// TTL in seconds (0 = no time-based expiry, live until invalidated).
    pub ttl_seconds: u64,
}

impl Default for ResponseCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false, // opt-in
            max_entries: 10_000,
            ttl_seconds: 300,
        }
    }
}

/// Per-entry value stored in moka alongside the response value.
///
/// Contains the accessed views so the eviction listener can clean up
/// the reverse index when an entry is evicted by TTL or LFU policy.
struct ResponseEntry {
    /// The projected GraphQL response value.
    response: Arc<Value>,

    /// Views accessed by this query (for invalidation).
    accessed_views: Box<[String]>,
}

/// Executor-level cache for projected GraphQL responses.
///
/// Stores the final serialized response keyed by `(query_hash, security_hash)`.
/// On hit, the entire projection + RBAC + serialization pipeline is skipped.
///
/// # Thread Safety
///
/// `moka::sync::Cache` is `Send + Sync` with lock-free reads. The view reverse
/// index uses `DashMap` (fine-grained shard locking). There is no global mutex
/// on the read path.
pub struct ResponseCache {
    store: MokaCache<(u64, u64), Arc<ResponseEntry>>,

    /// Reverse index: view name → set of `(query_hash, sec_hash)` keys.
    ///
    /// Maintained in `put()` and pruned by the moka eviction listener.
    /// Enables O(k) invalidation without scanning the full cache.
    view_index: Arc<DashMap<String, DashSet<(u64, u64)>>>,

    enabled: bool,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl ResponseCache {
    /// Create a new response cache from configuration.
    #[must_use]
    pub fn new(config: ResponseCacheConfig) -> Self {
        let view_index: Arc<DashMap<String, DashSet<(u64, u64)>>> = Arc::new(DashMap::new());
        let vi = Arc::clone(&view_index);

        let mut builder = MokaCache::builder()
            .max_capacity(config.max_entries as u64)
            .eviction_listener(move |key: Arc<(u64, u64)>, value: Arc<ResponseEntry>, _cause| {
                for view in &value.accessed_views {
                    if let Some(keys) = vi.get(view) {
                        keys.remove(&*key);
                    }
                }
            });

        if config.ttl_seconds > 0 {
            builder = builder.time_to_live(Duration::from_secs(config.ttl_seconds));
        }

        let store = builder.build();

        Self {
            store,
            view_index,
            enabled: config.enabled,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Whether the response cache is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Look up a cached response.
    ///
    /// # Errors
    ///
    /// This method is infallible with the moka backend and always returns `Ok`.
    pub fn get(&self, query_key: u64, security_hash: u64) -> Result<Option<Arc<Value>>> {
        if !self.enabled {
            return Ok(None);
        }

        let key = (query_key, security_hash);
        if let Some(entry) = self.store.get(&key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Ok(Some(Arc::clone(&entry.response)))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            Ok(None)
        }
    }

    /// Store a response in the cache.
    ///
    /// # Errors
    ///
    /// This method is infallible with the moka backend and always returns `Ok`.
    pub fn put(
        &self,
        query_key: u64,
        security_hash: u64,
        response: Arc<Value>,
        accessed_views: Vec<String>,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let key = (query_key, security_hash);

        // Update view → key reverse index before inserting into the store,
        // so invalidate_views() called concurrently won't miss the key.
        for view in &accessed_views {
            self.view_index.entry(view.clone()).or_default().insert(key);
        }

        let entry = Arc::new(ResponseEntry {
            response,
            accessed_views: accessed_views.into_boxed_slice(),
        });

        self.store.insert(key, entry);
        Ok(())
    }

    /// Invalidate all entries that access any of the given views.
    ///
    /// Uses the O(k) reverse index — no full-cache scan.
    ///
    /// # Errors
    ///
    /// This method is infallible with the moka backend and always returns `Ok`.
    pub fn invalidate_views(&self, views: &[String]) -> Result<u64> {
        let mut total = 0_u64;

        for view in views {
            if let Some(keys) = self.view_index.get(view) {
                let to_remove: Vec<(u64, u64)> = keys.iter().map(|k| *k).collect();
                drop(keys);
                for key in to_remove {
                    self.store.invalidate(&key);
                    // The eviction listener handles index cleanup.
                    total += 1;
                }
            }
        }

        Ok(total)
    }

    /// Get cache hit/miss counts.
    #[must_use]
    pub fn metrics(&self) -> (u64, u64) {
        (self.hits.load(Ordering::Relaxed), self.misses.load(Ordering::Relaxed))
    }

    /// Flush pending background tasks in the moka store.
    ///
    /// Used in tests to synchronise async eviction/invalidation before assertions.
    #[cfg(test)]
    pub(crate) fn run_pending_tasks(&self) {
        self.store.run_pending_tasks();
    }
}

/// Hash the security context fields that affect response content.
///
/// Fields hashed: `user_id`, roles (sorted), `tenant_id`, scopes (sorted),
/// `attributes` (sorted keys + JSON-serialized values).
///
/// Fields NOT hashed: `request_id`, `ip_address`, `authenticated_at`, `expires_at`,
/// `issuer`, `audience` — these don't affect which data the user can see.
///
/// `attributes` IS hashed because custom RLS policies can key on arbitrary
/// attributes (e.g., "department", "region") to produce different query results
/// for users who otherwise share the same `user_id`/roles/`tenant_id`/scopes.
///
/// Returns `0` when no security context is present (all users share one entry).
#[must_use]
pub fn hash_security_context(ctx: Option<&SecurityContext>) -> u64 {
    use std::hash::{Hash, Hasher};

    let Some(ctx) = ctx else {
        return 0;
    };

    let mut hasher = ahash::AHasher::default();
    ctx.user_id.hash(&mut hasher);

    // Sort roles for determinism (JWT may present them in any order)
    let mut sorted_roles = ctx.roles.clone();
    sorted_roles.sort();
    for role in &sorted_roles {
        role.hash(&mut hasher);
    }

    ctx.tenant_id.hash(&mut hasher);

    let mut sorted_scopes = ctx.scopes.clone();
    sorted_scopes.sort();
    for scope in &sorted_scopes {
        scope.hash(&mut hasher);
    }

    // Hash attributes (custom RLS policies can key on these)
    if !ctx.attributes.is_empty() {
        let mut attr_keys: Vec<&String> = ctx.attributes.keys().collect();
        attr_keys.sort();
        for key in attr_keys {
            key.hash(&mut hasher);
            // Use JSON serialization for deterministic Value hashing
            serde_json::to_string(&ctx.attributes[key])
                .unwrap_or_default()
                .hash(&mut hasher);
        }
    }

    hasher.finish()
}
