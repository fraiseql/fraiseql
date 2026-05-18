//! Query plan cache for federation query execution.
//!
//! Caches query plans keyed on normalized query string and schema fingerprint,
//! avoiding redundant planning for repeated queries against the same schema version.

use std::{num::NonZeroUsize, sync::Mutex};

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
#[derive(Debug, Clone, serde::Serialize)]
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
    #[must_use] 
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
#[must_use] 
pub fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Generate a schema fingerprint from federation metadata.
///
/// The fingerprint changes when types, keys, or directives change,
/// ensuring stale plans are not reused after schema updates.
#[must_use] 
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
mod tests;
