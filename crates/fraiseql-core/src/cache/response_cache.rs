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
            enabled:     false, // opt-in
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
    hits:    AtomicU64,
    misses:  AtomicU64,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> ResponseCacheConfig {
        ResponseCacheConfig {
            enabled:     true,
            max_entries: 100,
            ttl_seconds: 3600,
        }
    }

    #[test]
    fn test_put_and_get() {
        let cache = ResponseCache::new(enabled_config());
        let response = Arc::new(serde_json::json!({"data": {"users": []}}));

        cache
            .put(1, 0, response.clone(), vec!["v_user".to_string()])
            .expect("put should succeed");
        let result = cache.get(1, 0).expect("get should succeed");
        assert!(result.is_some());
        assert_eq!(*result.expect("should be Some"), *response);
    }

    #[test]
    fn test_different_security_contexts_different_entries() {
        let cache = ResponseCache::new(enabled_config());

        let admin_response =
            Arc::new(serde_json::json!({"data": {"users": [{"id": "1", "role": "admin"}]}}));
        let user_response = Arc::new(serde_json::json!({"data": {"users": [{"id": "1"}]}}));

        // Same query key (1), different security hashes
        cache
            .put(1, 100, admin_response.clone(), vec!["v_user".to_string()])
            .expect("put admin");
        cache
            .put(1, 200, user_response.clone(), vec!["v_user".to_string()])
            .expect("put user");

        let admin_result = cache.get(1, 100).expect("get admin").expect("admin hit");
        let user_result = cache.get(1, 200).expect("get user").expect("user hit");

        assert_ne!(*admin_result, *user_result);
        assert_eq!(*admin_result, *admin_response);
        assert_eq!(*user_result, *user_response);
    }

    #[test]
    fn test_invalidate_views() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("r1")), vec!["v_user".to_string()])
            .expect("put 1");
        cache
            .put(2, 0, Arc::new(serde_json::json!("r2")), vec!["v_post".to_string()])
            .expect("put 2");

        // Flush pending moka writes before invalidation
        cache.store.run_pending_tasks();

        let invalidated = cache.invalidate_views(&["v_user".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 1);

        // Flush invalidations
        cache.store.run_pending_tasks();

        assert!(cache.get(1, 0).expect("get 1").is_none());
        assert!(cache.get(2, 0).expect("get 2").is_some());
    }

    #[test]
    fn test_disabled_cache_returns_none() {
        let cache = ResponseCache::new(ResponseCacheConfig::default());
        assert!(!cache.is_enabled());

        cache.put(1, 0, Arc::new(serde_json::json!("r")), vec![]).expect("put disabled");
        assert!(cache.get(1, 0).expect("get disabled").is_none());
    }

    #[test]
    fn test_metrics() {
        let cache = ResponseCache::new(enabled_config());

        cache.put(1, 0, Arc::new(serde_json::json!("r")), vec![]).expect("put");
        cache.store.run_pending_tasks();
        let _ = cache.get(1, 0); // hit
        let _ = cache.get(2, 0); // miss

        let (hits, misses) = cache.metrics();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    // ========================================================================
    // Security Context Hash Tests
    // ========================================================================

    #[test]
    fn test_hash_security_context_none_returns_zero() {
        assert_eq!(hash_security_context(None), 0);
    }

    #[test]
    fn test_hash_security_context_same_context_same_hash() {
        let ctx = make_security_context("alice", &["admin"], Some("tenant-1"), &["read:user"]);
        let hash1 = hash_security_context(Some(&ctx));
        let hash2 = hash_security_context(Some(&ctx));
        assert_eq!(hash1, hash2, "Same context must produce same hash");
    }

    #[test]
    fn test_hash_security_context_different_user_different_hash() {
        let alice = make_security_context("alice", &["admin"], Some("tenant-1"), &[]);
        let bob = make_security_context("bob", &["admin"], Some("tenant-1"), &[]);

        assert_ne!(
            hash_security_context(Some(&alice)),
            hash_security_context(Some(&bob)),
            "Different user_id must produce different hash"
        );
    }

    #[test]
    fn test_hash_security_context_different_roles_different_hash() {
        let admin = make_security_context("alice", &["admin"], None, &[]);
        let viewer = make_security_context("alice", &["viewer"], None, &[]);

        assert_ne!(
            hash_security_context(Some(&admin)),
            hash_security_context(Some(&viewer)),
            "Different roles must produce different hash"
        );
    }

    #[test]
    fn test_hash_security_context_role_order_independent() {
        let ctx1 = make_security_context("alice", &["admin", "viewer"], None, &[]);
        let ctx2 = make_security_context("alice", &["viewer", "admin"], None, &[]);

        assert_eq!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Role order must not affect hash (sorted internally)"
        );
    }

    #[test]
    fn test_hash_security_context_different_tenant_different_hash() {
        let t1 = make_security_context("alice", &[], Some("tenant-1"), &[]);
        let t2 = make_security_context("alice", &[], Some("tenant-2"), &[]);
        let none = make_security_context("alice", &[], None, &[]);

        assert_ne!(hash_security_context(Some(&t1)), hash_security_context(Some(&t2)),);
        assert_ne!(hash_security_context(Some(&t1)), hash_security_context(Some(&none)),);
    }

    #[test]
    fn test_hash_security_context_different_scopes_different_hash() {
        let read = make_security_context("alice", &[], None, &["read:user"]);
        let write = make_security_context("alice", &[], None, &["write:user"]);
        let both = make_security_context("alice", &[], None, &["read:user", "write:user"]);

        assert_ne!(hash_security_context(Some(&read)), hash_security_context(Some(&write)),);
        assert_ne!(hash_security_context(Some(&read)), hash_security_context(Some(&both)),);
    }

    #[test]
    fn test_hash_security_context_scope_order_independent() {
        let ctx1 = make_security_context("alice", &[], None, &["read:user", "write:post"]);
        let ctx2 = make_security_context("alice", &[], None, &["write:post", "read:user"]);

        assert_eq!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Scope order must not affect hash (sorted internally)"
        );
    }

    #[test]
    fn test_hash_security_context_different_attributes_different_hash() {
        let mut ctx1 = make_security_context("alice", &["admin"], None, &[]);
        ctx1.attributes
            .insert("department".to_string(), serde_json::json!("engineering"));

        let mut ctx2 = make_security_context("alice", &["admin"], None, &[]);
        ctx2.attributes.insert("department".to_string(), serde_json::json!("sales"));

        let ctx_no_attrs = make_security_context("alice", &["admin"], None, &[]);

        assert_ne!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Different attribute values must produce different hashes"
        );
        assert_ne!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx_no_attrs)),
            "Attributes vs no attributes must produce different hashes"
        );
    }

    // ========================================================================
    // Invalidation Edge Cases
    // ========================================================================

    #[test]
    fn test_invalidate_empty_views_is_noop() {
        let cache = ResponseCache::new(enabled_config());
        cache
            .put(1, 0, Arc::new(serde_json::json!("r")), vec!["v_user".to_string()])
            .expect("put");
        cache.store.run_pending_tasks();

        let invalidated = cache.invalidate_views(&[]).expect("invalidate empty");
        assert_eq!(invalidated, 0);
        assert!(cache.get(1, 0).expect("still cached").is_some());
    }

    #[test]
    fn test_invalidate_nonexistent_view_is_noop() {
        let cache = ResponseCache::new(enabled_config());
        cache
            .put(1, 0, Arc::new(serde_json::json!("r")), vec!["v_user".to_string()])
            .expect("put");
        cache.store.run_pending_tasks();

        let invalidated = cache
            .invalidate_views(&["v_nonexistent".to_string()])
            .expect("invalidate nonexistent");
        assert_eq!(invalidated, 0);
        assert!(cache.get(1, 0).expect("still cached").is_some());
    }

    #[test]
    fn test_invalidate_clears_all_security_contexts_for_view() {
        let cache = ResponseCache::new(enabled_config());

        // Same query, different users, same view
        cache
            .put(1, 100, Arc::new(serde_json::json!("admin")), vec!["v_user".to_string()])
            .expect("put admin");
        cache
            .put(1, 200, Arc::new(serde_json::json!("user")), vec!["v_user".to_string()])
            .expect("put user");
        cache
            .put(1, 0, Arc::new(serde_json::json!("anon")), vec!["v_user".to_string()])
            .expect("put anon");
        cache.store.run_pending_tasks();

        let invalidated = cache.invalidate_views(&["v_user".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 3, "All entries for the view must be invalidated");

        cache.store.run_pending_tasks();

        assert!(cache.get(1, 100).expect("admin gone").is_none());
        assert!(cache.get(1, 200).expect("user gone").is_none());
        assert!(cache.get(1, 0).expect("anon gone").is_none());
    }

    #[test]
    fn test_invalidate_multiple_views_at_once() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("users")), vec!["v_user".to_string()])
            .expect("put users");
        cache
            .put(2, 0, Arc::new(serde_json::json!("posts")), vec!["v_post".to_string()])
            .expect("put posts");
        cache
            .put(3, 0, Arc::new(serde_json::json!("tags")), vec!["v_tag".to_string()])
            .expect("put tags");
        cache.store.run_pending_tasks();

        let invalidated = cache
            .invalidate_views(&["v_user".to_string(), "v_post".to_string()])
            .expect("invalidate");
        assert_eq!(invalidated, 2);

        cache.store.run_pending_tasks();

        assert!(cache.get(1, 0).expect("users gone").is_none());
        assert!(cache.get(2, 0).expect("posts gone").is_none());
        assert!(cache.get(3, 0).expect("tags alive").is_some());
    }

    #[test]
    fn test_entry_with_multiple_views_invalidated_by_any() {
        let cache = ResponseCache::new(enabled_config());

        // Query reads from both v_user and v_post (e.g., a join)
        cache
            .put(
                1,
                0,
                Arc::new(serde_json::json!("joined")),
                vec!["v_user".to_string(), "v_post".to_string()],
            )
            .expect("put");
        cache.store.run_pending_tasks();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&["v_post".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 1);

        cache.store.run_pending_tasks();
        assert!(cache.get(1, 0).expect("gone").is_none());
    }

    // ========================================================================
    // Response Cache Key Collision Avoidance
    // ========================================================================

    #[test]
    fn test_different_query_keys_no_collision() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("response_1")), vec![])
            .expect("put q1");
        cache
            .put(2, 0, Arc::new(serde_json::json!("response_2")), vec![])
            .expect("put q2");
        cache.store.run_pending_tasks();

        let r1 = cache.get(1, 0).expect("get q1").expect("q1 hit");
        let r2 = cache.get(2, 0).expect("get q2").expect("q2 hit");

        assert_eq!(*r1, serde_json::json!("response_1"));
        assert_eq!(*r2, serde_json::json!("response_2"));
    }

    #[test]
    fn test_same_query_key_different_security_no_collision() {
        let cache = ResponseCache::new(enabled_config());

        for sec_hash in 0_u64..10 {
            cache
                .put(
                    42,
                    sec_hash,
                    Arc::new(serde_json::json!(format!("response_for_user_{sec_hash}"))),
                    vec![],
                )
                .expect("put");
        }
        cache.store.run_pending_tasks();

        for sec_hash in 0_u64..10 {
            let r = cache.get(42, sec_hash).expect("get").expect("should be cached");
            assert_eq!(*r, serde_json::json!(format!("response_for_user_{sec_hash}")));
        }
    }

    // ========================================================================
    // Helper: SecurityContext builder for tests
    // ========================================================================

    fn make_security_context(
        user_id: &str,
        roles: &[&str],
        tenant_id: Option<&str>,
        scopes: &[&str],
    ) -> SecurityContext {
        use chrono::Utc;
        SecurityContext {
            user_id:          user_id.to_string(),
            roles:            roles.iter().map(|s| (*s).to_string()).collect(),
            tenant_id:        tenant_id.map(str::to_string),
            scopes:           scopes.iter().map(|s| (*s).to_string()).collect(),
            attributes:       std::collections::HashMap::new(),
            request_id:       "test-request".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
    }
}
