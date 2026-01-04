//! Query caching module.
//!
//! Contains two types of caches:
//! 1. QueryPlanCache: Caches GraphQL query execution plans (existing)
//! 2. QueryResultCache: Caches GraphQL query results with entity tracking (Phase 17A)

pub mod cache_key;
pub mod executor;
pub mod http_integration;
pub mod monitoring;
pub mod mutation_invalidator;
pub mod query_result;
pub mod signature;

#[cfg(test)]
mod tests_monitoring;

// Re-export key types and functions for convenience
pub use cache_key::QueryCacheKey;
pub use executor::{execute_query_with_cache, invalidate_cache_from_cascade};
pub use http_integration::{
    clear_cache, execute_cached_query, get_cache_metrics, invalidate_cached_queries, CacheConfig,
};
pub use monitoring::{CacheHealthThresholds, CacheMonitor, HealthReport, HealthStatus};
pub use mutation_invalidator::{extract_cascade_from_response, invalidate_cache_on_mutation};
pub use query_result::{CacheMetrics, CachedResult, QueryResultCache, QueryResultCacheConfig};

use anyhow::{anyhow, Result};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Cached GraphQL query execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedQueryPlan {
    /// Query signature (hash of query structure)
    pub signature: String,
    /// SQL template with parameter placeholders
    pub sql_template: String,
    /// Parameter information
    pub parameters: Vec<ParamInfo>,
    /// Unix timestamp when cache entry was created
    pub created_at: u64,
    /// Number of cache hits
    pub hit_count: u64,
}

/// Parameter metadata for cached query plans
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamInfo {
    /// Parameter name
    pub name: String,
    /// Parameter position in query
    pub position: usize,
    /// Expected parameter type: "string", "int", "float", "bool", "json"
    pub expected_type: String,
}

/// Thread-safe query plan cache.
#[derive(Debug)]
pub struct QueryPlanCache {
    cache: Arc<Mutex<LruCache<String, CachedQueryPlan>>>,
    max_size: usize,
    hits: Arc<Mutex<u64>>,
    misses: Arc<Mutex<u64>>,
}

/// Query plan cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Hit rate (hits / total requests)
    pub hit_rate: f64,
    /// Current number of entries in cache
    pub size: usize,
    /// Maximum cache capacity
    pub max_size: usize,
}

impl QueryPlanCache {
    /// Create a new query plan cache.
    ///
    /// # Panics
    ///
    /// Panics if `max_size` is 0.
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(max_size).expect("cache size must be non-zero"),
            ))),
            max_size,
            hits: Arc::new(Mutex::new(0)),
            misses: Arc::new(Mutex::new(0)),
        }
    }

    /// Get a cached query plan by signature.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache mutex is poisoned.
    ///
    /// # Panics
    ///
    /// Panics if the hits/misses counter mutex is poisoned.
    pub fn get(&self, signature: &str) -> Result<Option<CachedQueryPlan>> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|e| anyhow!("Cache lock error: {e}"))?;

        if let Some(plan) = cache.get_mut(signature) {
            plan.hit_count += 1;
            *self.hits.lock().expect("hits counter mutex poisoned") += 1;
            Ok(Some(plan.clone()))
        } else {
            *self.misses.lock().expect("misses counter mutex poisoned") += 1;
            Ok(None)
        }
    }

    /// Store a query plan in the cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache mutex is poisoned.
    pub fn put(&self, signature: String, plan: CachedQueryPlan) -> Result<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|e| anyhow!("Cache lock error: {e}"))?;
        cache.put(signature, plan);
        Ok(())
    }

    /// Clear all cached plans and reset statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache mutex is poisoned.
    ///
    /// # Panics
    ///
    /// Panics if the hits/misses counter mutex is poisoned.
    pub fn clear(&self) -> Result<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|e| anyhow!("Cache lock error: {e}"))?;
        cache.clear();

        // Reset counters
        *self.hits.lock().expect("hits counter mutex poisoned") = 0;
        *self.misses.lock().expect("misses counter mutex poisoned") = 0;

        Ok(())
    }

    /// Get cache statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache mutex is poisoned.
    ///
    /// # Panics
    ///
    /// Panics if the hits/misses counter mutex is poisoned.
    pub fn stats(&self) -> Result<CacheStats> {
        let hits = *self.hits.lock().expect("hits counter mutex poisoned");
        let misses = *self.misses.lock().expect("misses counter mutex poisoned");
        let size = self
            .cache
            .lock()
            .map_err(|e| anyhow!("Cache lock error: {e}"))?
            .len();

        Ok(CacheStats {
            hits,
            misses,
            hit_rate: if hits + misses > 0 {
                hits as f64 / (hits + misses) as f64
            } else {
                0.0
            },
            size,
            max_size: self.max_size,
        })
    }
}

impl Default for QueryPlanCache {
    fn default() -> Self {
        Self::new(5000) // 5000 cached plans by default
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Tests can use unwrap for simplicity
mod tests {
    use super::*;

    #[test]
    fn test_cache_put_get() {
        let cache = QueryPlanCache::new(100);
        let plan = CachedQueryPlan {
            signature: "test_query".to_string(),
            sql_template: "SELECT * FROM users".to_string(),
            parameters: vec![],
            created_at: 0,
            hit_count: 0,
        };

        cache.put("test_query".to_string(), plan).unwrap();
        let retrieved = cache.get("test_query").unwrap().unwrap();

        assert_eq!(retrieved.signature, "test_query");
    }

    #[test]
    fn test_cache_hit_counting() {
        let cache = QueryPlanCache::new(100);
        let plan = CachedQueryPlan {
            signature: "test".to_string(),
            sql_template: "SELECT *".to_string(),
            parameters: vec![],
            created_at: 0,
            hit_count: 0,
        };

        cache.put("test".to_string(), plan).unwrap();

        // Access 5 times
        for _ in 0..5 {
            cache.get("test").unwrap();
        }

        let stats = cache.stats().unwrap();
        assert_eq!(stats.hits, 5);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = QueryPlanCache::new(3);

        for i in 0..5 {
            let plan = CachedQueryPlan {
                signature: format!("query_{i}"),
                sql_template: "SELECT *".to_string(),
                parameters: vec![],
                created_at: 0,
                hit_count: 0,
            };
            cache.put(format!("query_{i}"), plan).unwrap();
        }

        let stats = cache.stats().unwrap();
        assert_eq!(stats.size, 3); // Only 3 entries (LRU eviction)
    }
}
