//! Integration tests for query result caching (Phase 17A.2)
//!
//! Tests cache key generation and query caching scenarios

#[cfg(test)]
mod tests {
    use crate::cache::{QueryCacheKey, QueryResultCache, QueryResultCacheConfig};
    use crate::graphql::types::{FieldSelection, ParsedQuery};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_query_cache_key_for_list_query() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: Some("ListUsers".to_string()),
            root_field: "users".to_string(),
            selections: vec![FieldSelection {
                name: "users".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![],
                directives: vec![],
            }],
            variables: vec![],
            fragments: vec![],
            source: "query { users { id name } }".to_string(),
        };

        let variables = HashMap::new();
        let cache_key = QueryCacheKey::from_query(&query, &variables);

        assert!(cache_key.is_some());
        let key = cache_key.unwrap();
        assert!(key.key.contains("users"));
        // Should track that it accesses all User entities
        assert_eq!(
            key.accessed_entities,
            vec![("User".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn test_query_cache_key_includes_variables() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "user".to_string(),
            selections: vec![FieldSelection {
                name: "user".to_string(),
                alias: None,
                arguments: vec![],
                nested_fields: vec![],
                directives: vec![],
            }],
            variables: vec![],
            fragments: vec![],
            source: "query($id: ID!) { user(id: $id) { id name } }".to_string(),
        };

        let mut variables = HashMap::new();
        variables.insert("id".to_string(), json!("123"));

        let cache_key1 = QueryCacheKey::from_query(&query, &variables);
        assert!(cache_key1.is_some());

        // Different variable value = different cache key
        let mut variables2 = HashMap::new();
        variables2.insert("id".to_string(), json!("456"));
        let cache_key2 = QueryCacheKey::from_query(&query, &variables2);
        assert!(cache_key2.is_some());

        assert_ne!(cache_key1.unwrap().key, cache_key2.unwrap().key);
    }

    #[test]
    fn test_query_cache_key_not_for_mutations() {
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
        let cache_key = QueryCacheKey::from_query(&query, &variables);

        // Mutations should not be cached
        assert!(cache_key.is_none());
    }

    #[test]
    fn test_cache_stores_and_retrieves_query_result() {
        let cache = QueryResultCache::new(QueryResultCacheConfig::default());

        let result = json!({
            "data": {
                "users": [
                    {"id": "1", "name": "Alice"},
                    {"id": "2", "name": "Bob"}
                ]
            }
        });

        let cache_key = "query:users:no-vars";
        let accessed_entities = vec![("User".to_string(), "*".to_string())];

        // Store in cache
        cache
            .put(cache_key.to_string(), result.clone(), accessed_entities)
            .unwrap();

        // Retrieve from cache
        let cached = cache.get(cache_key).unwrap();
        assert!(cached.is_some());
        assert_eq!(
            cached.unwrap().get("data"),
            Some(&result.get("data").unwrap().clone())
        );
    }

    #[test]
    fn test_cache_hit_miss_tracking_on_query() {
        let cache = QueryResultCache::new(QueryResultCacheConfig::default());

        // Miss on first query
        let miss1 = cache.get("nonexistent").unwrap();
        assert!(miss1.is_none());

        // Store something
        let result = json!({"data": {"users": []}});
        cache
            .put(
                "query:users:all".to_string(),
                result,
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        // Hit on second query
        let hit1 = cache.get("query:users:all").unwrap();
        assert!(hit1.is_some());

        // Another hit
        let hit2 = cache.get("query:users:all").unwrap();
        assert!(hit2.is_some());

        // Check metrics
        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 2);
        assert_eq!(metrics.misses, 1);
    }

    #[test]
    fn test_query_cache_invalidation_on_entity_update() {
        let cache = QueryResultCache::new(QueryResultCacheConfig::default());

        // Cache two queries: one for all users, one for a specific user
        cache
            .put(
                "query:users:all".to_string(),
                json!({"users": [{"id": "1", "name": "Alice"}]}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        let metrics_before = cache.metrics().unwrap();
        assert_eq!(metrics_before.size, 2);

        // User 1 is updated
        let cascade = json!({
            "invalidations": {
                "updated": [
                    {"type": "User", "id": "1"}
                ]
            }
        });

        let invalidated_count = cache.invalidate_from_cascade(&cascade).unwrap();

        // Both queries should be invalidated (all users accesses User:1, specific user accesses User:1)
        assert_eq!(invalidated_count, 2);

        let metrics_after = cache.metrics().unwrap();
        assert_eq!(metrics_after.size, 0);
    }

    #[test]
    fn test_query_cache_selective_invalidation() {
        let cache = QueryResultCache::new(QueryResultCacheConfig::default());

        // Cache three queries
        cache
            .put(
                "query:users:all".to_string(),
                json!({"users": []}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:posts:all".to_string(),
                json!({"posts": []}),
                vec![("Post".to_string(), "*".to_string())],
            )
            .unwrap();

        // Only User 1 is updated
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}]
            }
        });

        let invalidated_count = cache.invalidate_from_cascade(&cascade).unwrap();

        // user:all and user:1 should be invalidated (both access User:1)
        // posts:all should NOT be invalidated
        assert_eq!(invalidated_count, 2);

        // posts:all should still be cached
        let posts_cached = cache.get("query:posts:all").unwrap();
        assert!(posts_cached.is_some());
    }

    #[test]
    fn test_query_cache_lru_eviction() {
        let mut config = QueryResultCacheConfig::default();
        config.max_entries = 2;

        let cache = QueryResultCache::new(config);

        // Add 3 entries to cache with max 2
        for i in 0..3 {
            cache
                .put(
                    format!("query:user:{i}"),
                    json!({"user": {"id": format!("{i}")}}),
                    vec![("User".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        let metrics = cache.metrics().unwrap();
        // Should only have 2 entries (LRU evicted the oldest)
        assert_eq!(metrics.size, 2);
    }
}
