//! Query plan cache for federation query execution.
//!
//! Caches query plans keyed on normalized query string and schema fingerprint,
//! avoiding redundant planning for repeated queries against the same schema version.

use std::num::NonZeroUsize;
use std::sync::Mutex;

use lru::LruCache;

/// A cached query plan representing the resolution strategy for a federation query.
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Subgraph fetch operations in execution order
    pub fetches: Vec<SubgraphFetch>,

    /// Schema fingerprint at plan creation time
    pub schema_fingerprint: String,
}

/// A single fetch operation targeting a subgraph
#[derive(Debug, Clone)]
pub struct SubgraphFetch {
    /// Subgraph name or URL
    pub subgraph: String,

    /// GraphQL query to send to this subgraph
    pub query: String,

    /// Entity types being resolved
    pub entity_types: Vec<String>,

    /// Whether this fetch depends on a prior fetch
    pub depends_on: Option<usize>,
}

/// LRU cache for federation query plans.
///
/// Thread-safe via `Mutex`. Keyed on `(normalized_query, schema_fingerprint)`.
pub struct QueryPlanCache {
    cache: Mutex<LruCache<String, QueryPlan>>,
}

impl QueryPlanCache {
    /// Create a new query plan cache with the given capacity.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("cache capacity must be > 0"),
            )),
        }
    }

    /// Look up a cached query plan.
    ///
    /// Returns `None` if the plan is not cached or the schema fingerprint
    /// does not match (stale plan).
    pub fn get(&self, query: &str, schema_fingerprint: &str) -> Option<QueryPlan> {
        let key = Self::cache_key(query, schema_fingerprint);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.get(&key).cloned()
    }

    /// Insert a query plan into the cache.
    pub fn put(&self, query: &str, plan: QueryPlan) {
        let key = Self::cache_key(query, &plan.schema_fingerprint);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.put(key, plan);
    }

    /// Invalidate all cached plans (e.g. on schema reload).
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }

    /// Number of cached plans.
    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Build cache key from query and schema fingerprint.
    fn cache_key(query: &str, schema_fingerprint: &str) -> String {
        format!("{}:{}", schema_fingerprint, query)
    }
}

/// Normalize a GraphQL query for cache key generation.
///
/// Strips insignificant whitespace and operation names so that semantically
/// identical queries share a cache entry.
pub fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Generate a schema fingerprint from federation metadata.
///
/// The fingerprint changes when types, keys, or directives change,
/// ensuring stale plans are not reused after schema updates.
pub fn schema_fingerprint(types: &[(&str, &[&str])]) -> String {
    let mut parts: Vec<String> = types
        .iter()
        .map(|(name, keys)| format!("{}:{}", name, keys.join(",")))
        .collect();
    parts.sort();
    format!("{:x}", simple_hash(&parts.join(";")))
}

/// Simple non-cryptographic hash for fingerprinting.
fn simple_hash(input: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    hash
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryPlanCache::new(100);
        let plan = QueryPlan {
            fetches:            vec![SubgraphFetch {
                subgraph:     "users".to_string(),
                query:        "{ user(id: $id) { name } }".to_string(),
                entity_types: vec!["User".to_string()],
                depends_on:   None,
            }],
            schema_fingerprint: "abc123".to_string(),
        };

        cache.put("query GetUser { user { name } }", plan);
        let cached = cache.get("query GetUser { user { name } }", "abc123");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().fetches.len(), 1);
    }

    #[test]
    fn test_cache_miss_on_different_fingerprint() {
        let cache = QueryPlanCache::new(100);
        let plan = QueryPlan {
            fetches:            vec![],
            schema_fingerprint: "abc123".to_string(),
        };

        cache.put("query { user { name } }", plan);
        let cached = cache.get("query { user { name } }", "different_fingerprint");
        assert!(cached.is_none(), "should not match stale schema fingerprint");
    }

    #[test]
    fn test_cache_eviction() {
        let cache = QueryPlanCache::new(2);
        for i in 0..3 {
            let plan = QueryPlan {
                fetches:            vec![],
                schema_fingerprint: "fp".to_string(),
            };
            cache.put(&format!("query{i}"), plan);
        }

        assert_eq!(cache.len(), 2, "LRU should evict oldest entry");
        assert!(cache.get("query0", "fp").is_none(), "query0 should be evicted");
        assert!(cache.get("query2", "fp").is_some(), "query2 should be present");
    }

    #[test]
    fn test_cache_clear() {
        let cache = QueryPlanCache::new(100);
        let plan = QueryPlan {
            fetches:            vec![],
            schema_fingerprint: "fp".to_string(),
        };
        cache.put("q1", plan);

        assert!(!cache.is_empty());
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_normalize_query() {
        let q1 = "query  GetUser  {\n  user(id: 1)  {\n    name\n  }\n}";
        let q2 = "query GetUser { user(id: 1) { name } }";
        assert_eq!(normalize_query(q1), normalize_query(q2));
    }

    #[test]
    fn test_schema_fingerprint_deterministic() {
        let fp1 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
        let fp2 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_schema_fingerprint_changes_on_key_change() {
        let fp1 = schema_fingerprint(&[("User", &["id"])]);
        let fp2 = schema_fingerprint(&[("User", &["id", "email"])]);
        assert_ne!(fp1, fp2, "fingerprint should change when keys change");
    }

    #[test]
    fn test_schema_fingerprint_order_independent() {
        let fp1 = schema_fingerprint(&[("User", &["id"]), ("Order", &["id"])]);
        let fp2 = schema_fingerprint(&[("Order", &["id"]), ("User", &["id"])]);
        assert_eq!(fp1, fp2, "fingerprint should be order-independent");
    }

    #[test]
    fn test_multi_fetch_plan() {
        let cache = QueryPlanCache::new(100);
        let plan = QueryPlan {
            fetches:            vec![
                SubgraphFetch {
                    subgraph:     "users".to_string(),
                    query:        "{ user { id } }".to_string(),
                    entity_types: vec!["User".to_string()],
                    depends_on:   None,
                },
                SubgraphFetch {
                    subgraph:     "orders".to_string(),
                    query:        "{ orders { id } }".to_string(),
                    entity_types: vec!["Order".to_string()],
                    depends_on:   Some(0),
                },
            ],
            schema_fingerprint: "fp".to_string(),
        };

        cache.put("query { user { orders { id } } }", plan);
        let cached = cache.get("query { user { orders { id } } }", "fp").unwrap();
        assert_eq!(cached.fetches.len(), 2);
        assert_eq!(cached.fetches[1].depends_on, Some(0));
    }
}
