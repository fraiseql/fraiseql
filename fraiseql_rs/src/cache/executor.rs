//! Query execution with result caching (Phase 17A.2)
//!
//! Integrates `QueryResultCache` into the GraphQL pipeline to cache and retrieve
//! query results based on cascade-driven entity tracking.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::cache::{QueryCacheKey, QueryResultCache};
use crate::graphql::types::ParsedQuery;

/// Execute a GraphQL query with result caching
///
/// This function:
/// 1. Generates cache key from query + variables
/// 2. Checks cache for hit
/// 3. If miss, executes query and stores result
/// 4. Returns cached/fresh result
///
/// # Arguments
///
/// * `cache` - Shared query result cache
/// * `query` - Parsed GraphQL query
/// * `variables` - Query variables
/// * `execute_fn` - Closure that executes the query and returns JSON result
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
/// let result = execute_query_with_cache(
///     &cache,
///     &parsed_query,
///     &variables,
///     |q, v| {
///         // Your query execution logic here
///         let sql = compose_sql(q)?;
///         execute_database_query(&sql)
///     }
/// )?;
/// ```
pub fn execute_query_with_cache<F, S: ::std::hash::BuildHasher>(
    cache: &Arc<QueryResultCache>,
    query: &ParsedQuery,
    variables: &HashMap<String, Value, S>,
    execute_fn: F,
) -> Result<Value>
where
    F: Fn(&ParsedQuery, &HashMap<String, Value, S>) -> Result<Value>,
{
    // Generate cache key - skip caching if key generation returns None
    let Some(cache_key) = QueryCacheKey::from_query(query, variables) else {
        // Don't cache (mutation, dynamic directives, etc.)
        return execute_fn(query, variables);
    };

    // Check cache first
    if let Ok(Some(cached_result)) = cache.get(&cache_key.key) {
        // Cache hit - return immediately
        return Ok((*cached_result).clone());
    }

    // Cache miss - execute query
    let result = execute_fn(query, variables)?;

    // Store in cache for next time
    if let Err(e) = cache.put(&cache_key.key, result.clone(), cache_key.accessed_entities) {
        // Log cache error but don't fail the query
        eprintln!("Cache store error: {e}");
    }

    Ok(result)
}

/// Invalidate query cache based on mutation cascade metadata
///
/// Called after a mutation completes to invalidate queries that accessed
/// the changed entities. This ensures cache coherency.
///
/// # Arguments
///
/// * `cache` - Shared query result cache
/// * `cascade` - Cascade metadata from mutation response
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
/// let mutation_result = execute_mutation(query)?;
/// if let Some(cascade) = &mutation_result.cascade {
///     invalidate_cache_from_cascade(&cache, cascade)?;
/// }
/// ```
pub fn invalidate_cache_from_cascade(
    cache: &Arc<QueryResultCache>,
    cascade: &Value,
) -> Result<u64> {
    cache.invalidate_from_cascade(cascade)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::QueryResultCacheConfig;
    use serde_json::json;

    #[test]
    #[ignore = "requires async test refactoring"]
    fn test_execute_with_cache_hit() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "users".to_string(),
            selections: vec![],
            variables: vec![],
            fragments: vec![],
            source: "query { users { id } }".to_string(),
        };

        let variables = HashMap::new();

        // First call executes the function
        let result1 = execute_query_with_cache(&cache, &query, &variables, |_, _| {
            Ok(json!({"users": [{"id": "1"}]}))
        });

        assert!(result1.is_ok());

        // Second call hits cache
        let result2 = execute_query_with_cache(&cache, &query, &variables, |_, _| {
            Ok(json!({"users": [{"id": "2"}]}))
        });

        assert!(result2.is_ok());
        // Results should be the same (cached)
        assert_eq!(result1.unwrap(), result2.unwrap());
    }

    #[test]
    #[ignore = "requires async test refactoring"]
    fn test_execute_mutation_bypasses_cache() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        let query = ParsedQuery {
            operation_type: "mutation".to_string(),
            operation_name: None,
            root_field: "updateUser".to_string(),
            selections: vec![],
            variables: vec![],
            fragments: vec![],
            source: "mutation { updateUser(id: \"1\") { id } }".to_string(),
        };

        let variables = HashMap::new();

        // Mutations always execute (no caching)
        let result1 = execute_query_with_cache(&cache, &query, &variables, |_, _| {
            Ok(json!({"user": {"id": "1"}}))
        });

        assert!(result1.is_ok());

        // Second mutation call also executes (mutations not cached)
        let result2 = execute_query_with_cache(&cache, &query, &variables, |_, _| {
            Ok(json!({"user": {"id": "1"}}))
        });

        assert!(result2.is_ok());
    }

    #[test]
    fn test_invalidate_cache_on_cascade() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Pre-populate cache with queries about User:1
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:users:all".to_string(),
                json!({"users": [{"id": "1", "name": "Alice"}]}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        let metrics_before = cache.metrics().unwrap();
        assert_eq!(metrics_before.size, 2);

        // User 1 is updated
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}]
            }
        });

        let invalidated = invalidate_cache_from_cascade(&cache, &cascade).unwrap();
        assert_eq!(invalidated, 2);

        let metrics_after = cache.metrics().unwrap();
        assert_eq!(metrics_after.size, 0);
    }
}
