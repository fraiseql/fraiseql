//! HTTP server integration for query result cache (Phase 17A.4)
//!
//! Integrates `QueryResultCache` into the HTTP server's `AppState` and provides
//! hooks for query execution and mutation response handling.

use std::sync::Arc;

use crate::cache::{
    execute_query_with_cache, invalidate_cache_on_mutation, QueryResultCache,
    QueryResultCacheConfig,
};
use crate::graphql::types::ParsedQuery;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// HTTP server application state with query result cache
///
/// This extends the base `AppState` to include the query result cache.
/// The cache is shared across all HTTP handlers via Arc.
///
/// # Example
///
/// ```ignore
/// let config = QueryResultCacheConfig {
///     max_entries: 10000,
///     ttl_seconds: 86400,
///     cache_list_queries: true,
/// };
///
/// let cache = Arc::new(QueryResultCache::new(config));
/// let app_state = AppStateWithCache {
///     cache,
///     // ... other fields
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Query result cache
    pub cache: Arc<QueryResultCache>,
}

impl CacheConfig {
    /// Create a new cache configuration with default settings
    ///
    /// Default:
    /// - Max 10,000 cached queries
    /// - 24 hour TTL safety net
    /// - Caches list queries
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Arc::new(QueryResultCache::new(QueryResultCacheConfig::default())),
        }
    }

    /// Create a new cache configuration with custom settings
    #[must_use]
    pub fn with_config(config: QueryResultCacheConfig) -> Self {
        Self {
            cache: Arc::new(QueryResultCache::new(config)),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a GraphQL query with cache integration
///
/// This is the integration point for the HTTP handler to use caching.
/// It wraps the cache check/store logic around the actual query execution.
///
/// # Arguments
///
/// * `cache` - Shared query result cache
/// * `query` - Parsed GraphQL query
/// * `variables` - Query variables as `HashMap`
/// * `execute_fn` - Closure that executes the query and returns result
///
/// # Returns
///
/// Query result as JSON value
///
/// # Errors
/// Returns error if query execution fails
///
/// # Example
///
/// ```ignore
/// // In HTTP handler:
/// let result = execute_cached_query(
///     &app_state.cache.cache,
///     &parsed_query,
///     &variables,
///     |q, v| {
///         // Your query execution logic (call unified pipeline)
///         state.pipeline.execute_query_async(q, v).await
///     }
/// ).await?;
/// ```
#[allow(clippy::future_not_send)]
pub async fn execute_cached_query<F, S: ::std::hash::BuildHasher>(
    cache: &Arc<QueryResultCache>,
    query: &ParsedQuery,
    variables: &HashMap<String, Value, S>,
    execute_fn: F,
) -> Result<Value>
where
    F: Fn(&ParsedQuery, &HashMap<String, Value, S>) -> Result<Value>,
{
    // Use the cache executor
    execute_query_with_cache(cache, query, variables, execute_fn)
}

/// Invalidate query cache after a mutation completes
///
/// This is the integration point called after a successful mutation response
/// is built. It extracts cascade metadata and invalidates affected queries.
///
/// # Arguments
///
/// * `cache` - Shared query result cache
/// * `mutation_response` - Complete GraphQL mutation response
///
/// # Returns
///
/// Number of cache entries invalidated
///
/// # Errors
/// Returns error if invalidation fails
///
/// # Example
///
/// ```ignore
/// // After building mutation response:
/// let invalidated = invalidate_cached_queries(
///     &app_state.cache.cache,
///     &mutation_response
/// )?;
///
/// eprintln!("Cache invalidated {} entries", invalidated);
/// ```
pub fn invalidate_cached_queries(
    cache: &Arc<QueryResultCache>,
    mutation_response: &Value,
) -> Result<u64> {
    invalidate_cache_on_mutation(cache, mutation_response)
}

/// Get cache metrics for monitoring
///
/// Returns cache statistics like hit rate, size, memory usage.
/// Suitable for /metrics endpoint or observability dashboards.
///
/// # Errors
/// Returns error if metrics retrieval fails
///
/// # Example
///
/// ```ignore
/// let metrics = get_cache_metrics(&app_state.cache.cache)?;
/// println!("Cache hit rate: {}%", metrics.hit_rate * 100.0);
/// println!("Cache size: {} entries", metrics.size);
/// ```
pub fn get_cache_metrics(cache: &Arc<QueryResultCache>) -> Result<crate::cache::CacheMetrics> {
    cache.metrics()
}

/// Clear all cache entries (for manual invalidation or testing)
///
/// # Errors
/// Returns error if cache clear fails
///
/// # Example
///
/// ```ignore
/// clear_cache(&app_state.cache.cache)?;
/// eprintln!("Cache cleared");
/// ```
pub fn clear_cache(cache: &Arc<QueryResultCache>) -> Result<()> {
    cache.clear()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert!(Arc::strong_count(&config.cache) >= 1);
    }

    #[test]
    fn test_cache_config_custom() {
        let custom = QueryResultCacheConfig {
            max_entries: 5000,
            ttl_seconds: 3600,
            cache_list_queries: false,
        };

        let config = CacheConfig::with_config(custom);
        assert!(Arc::strong_count(&config.cache) >= 1);
    }

    #[test]
    fn test_get_cache_metrics() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        let metrics = get_cache_metrics(&cache).unwrap();
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.size, 0);
    }

    #[test]
    fn test_clear_cache() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Add entry
        cache
            .put(
                "query:test".to_string(),
                serde_json::json!({"test": "value"}),
                vec![("Test".to_string(), "1".to_string())],
            )
            .unwrap();

        let metrics_before = cache.metrics().unwrap();
        assert_eq!(metrics_before.size, 1);

        // Clear cache
        clear_cache(&cache).unwrap();

        let metrics_after = cache.metrics().unwrap();
        assert_eq!(metrics_after.size, 0);
    }
}
