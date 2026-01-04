//! Integration tests for HTTP server cache integration (Phase 17A.4)
//!
//! Tests the integration of query result cache into HTTP handlers

#[cfg(test)]
mod tests {
    use crate::cache::{
        clear_cache, execute_cached_query, get_cache_metrics, invalidate_cached_queries,
        CacheConfig, QueryResultCacheConfig,
    };
    use crate::graphql::types::ParsedQuery;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_cache_config_default_creation() {
        let config = CacheConfig::default();

        // Should create a functional cache
        let metrics = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics.size, 0);
        assert_eq!(metrics.hits, 0);
    }

    #[test]
    fn test_cache_config_custom_settings() {
        let custom = QueryResultCacheConfig {
            max_entries: 5000,
            ttl_seconds: 3600,
            cache_list_queries: false,
        };

        let config = CacheConfig::with_config(custom);
        let metrics = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics.size, 0);
    }

    #[test]
    fn test_execute_cached_query_hit() {
        let config = CacheConfig::default();
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
        let mut execution_count = 0;

        // First execution - cache miss
        let result1 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            execution_count += 1;
            Ok(json!({"users": [{"id": "1"}]}))
        });

        assert!(result1.is_ok());
        assert_eq!(execution_count, 1);

        // Second execution - cache hit
        let result2 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            execution_count += 1;
            Ok(json!({"users": [{"id": "2"}]}))
        });

        assert!(result2.is_ok());
        assert_eq!(execution_count, 1); // Executor not called again

        // Results should be the same (cached)
        assert_eq!(result1.unwrap(), result2.unwrap());
    }

    #[test]
    fn test_execute_cached_mutation_no_cache() {
        let config = CacheConfig::default();
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
        let mut execution_count = 0;

        // Mutations are never cached
        let result1 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            execution_count += 1;
            Ok(json!({"user": {"id": "1"}}))
        });

        assert!(result1.is_ok());
        assert_eq!(execution_count, 1);

        // Second execution also executes (no caching)
        let result2 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            execution_count += 1;
            Ok(json!({"user": {"id": "1"}}))
        });

        assert!(result2.is_ok());
        assert_eq!(execution_count, 2); // Both executed
    }

    #[test]
    fn test_invalidate_cached_queries_from_mutation() {
        let config = CacheConfig::default();

        // Pre-cache a query
        config
            .cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        let metrics_before = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics_before.size, 1);

        // Mutation response with cascade
        let mutation_response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Updated"},
                    "cascade": {
                        "invalidations": {
                            "updated": [{"type": "User", "id": "1"}],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cached_queries(&config.cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 1);

        let metrics_after = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics_after.size, 0);
    }

    #[test]
    fn test_clear_cache_operation() {
        let config = CacheConfig::default();

        // Add multiple entries
        for i in 0..5 {
            config
                .cache
                .put(
                    format!("query:item:{i}"),
                    json!({"item": {"id": format!("{i}")}}),
                    vec![("Item".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        let metrics_before = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics_before.size, 5);

        // Clear all
        clear_cache(&config.cache).unwrap();

        let metrics_after = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics_after.size, 0);
    }

    #[test]
    fn test_cache_metrics_tracking() {
        let config = CacheConfig::default();

        // Initial state
        let metrics1 = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics1.hits, 0);
        assert_eq!(metrics1.misses, 0);
        assert_eq!(metrics1.size, 0);

        // Miss
        let _ = config.cache.get("nonexistent").unwrap();

        // Hit
        config
            .cache
            .put(
                "query:test".to_string(),
                json!({}),
                vec![("Test".to_string(), "1".to_string())],
            )
            .unwrap();
        let _ = config.cache.get("query:test").unwrap();

        let metrics2 = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics2.misses, 1);
        assert_eq!(metrics2.hits, 1);
        assert_eq!(metrics2.size, 1);
    }

    #[test]
    fn test_multiple_cache_instances() {
        let config1 = CacheConfig::default();
        let config2 = CacheConfig::default();

        // Add to config1
        config1
            .cache
            .put(
                "query:a".to_string(),
                json!({"a": 1}),
                vec![("A".to_string(), "1".to_string())],
            )
            .unwrap();

        // config2 should be separate
        let metrics1 = get_cache_metrics(&config1.cache).unwrap();
        let metrics2 = get_cache_metrics(&config2.cache).unwrap();

        assert_eq!(metrics1.size, 1);
        assert_eq!(metrics2.size, 0);
    }

    #[test]
    fn test_cache_with_variables() {
        let config = CacheConfig::default();
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "user".to_string(),
            selections: vec![],
            variables: vec![],
            fragments: vec![],
            source: "query($id: ID!) { user(id: $id) { id name } }".to_string(),
        };

        let mut variables1 = HashMap::new();
        variables1.insert("id".to_string(), json!("123"));

        let mut variables2 = HashMap::new();
        variables2.insert("id".to_string(), json!("456"));

        let mut execution_count = 0;

        // Query with id=123
        let result1 = execute_cached_query(&config.cache, &query, &variables1, |_, _| {
            execution_count += 1;
            Ok(json!({"user": {"id": "123", "name": "Alice"}}))
        });

        assert_eq!(execution_count, 1);

        // Query with same id=123 - should hit cache
        let result1_again = execute_cached_query(&config.cache, &query, &variables1, |_, _| {
            execution_count += 1;
            Ok(json!({"user": {"id": "123", "name": "Different"}}))
        });

        assert_eq!(execution_count, 1); // Cache hit
        assert_eq!(result1.unwrap(), result1_again.unwrap());

        // Query with id=456 - different variable, cache miss
        let result2 = execute_cached_query(&config.cache, &query, &variables2, |_, _| {
            execution_count += 1;
            Ok(json!({"user": {"id": "456", "name": "Bob"}}))
        });

        assert_eq!(execution_count, 2); // New execution

        // Different results
        assert_ne!(result1.unwrap(), result2.unwrap());
    }

    #[test]
    fn test_cache_workflow_query_then_mutation() {
        let config = CacheConfig::default();
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "user".to_string(),
            selections: vec![],
            variables: vec![],
            fragments: vec![],
            source: "query { user(id: \"1\") { id name } }".to_string(),
        };

        let variables = HashMap::new();

        // Step 1: Query executes and caches
        let result1 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            Ok(json!({"user": {"id": "1", "name": "Alice"}}))
        });

        assert!(result1.is_ok());
        let metrics1 = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics1.size, 1);

        // Step 2: Same query hits cache
        let result1_cached = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            panic!("Should not execute - cached!")
        });

        assert!(result1_cached.is_ok());
        assert_eq!(result1.unwrap(), result1_cached.unwrap());

        // Step 3: Mutation occurs and invalidates cache
        let mutation_response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Alice Updated"},
                    "cascade": {
                        "invalidations": {
                            "updated": [{"type": "User", "id": "1"}],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cached_queries(&config.cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 1);

        let metrics2 = get_cache_metrics(&config.cache).unwrap();
        assert_eq!(metrics2.size, 0);

        // Step 4: Query executes again with fresh data
        let result2 = execute_cached_query(&config.cache, &query, &variables, |_, _| {
            Ok(json!({"user": {"id": "1", "name": "Alice Updated"}}))
        });

        assert!(result2.is_ok());
        assert_ne!(result1.unwrap(), result2.unwrap()); // Different data
    }
}
