//! Integration tests for mutation cache invalidation (Phase 17A.3)
//!
//! Tests cascade-driven invalidation of query cache when mutations complete

#[cfg(test)]
mod tests {
    use crate::cache::{
        extract_cascade_from_response, invalidate_cache_on_mutation, QueryResultCache,
        QueryResultCacheConfig,
    };
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_mutation_invalidates_single_entity_queries() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // User 1 is queried and cached
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        let metrics_before = cache.metrics().unwrap();
        assert_eq!(metrics_before.size, 1);

        // User 1 is updated
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

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 1);

        let metrics_after = cache.metrics().unwrap();
        assert_eq!(metrics_after.size, 0);
    }

    #[test]
    fn test_mutation_invalidates_all_entities_queries() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // "list all users" is cached
        cache
            .put(
                "query:users:all".to_string(),
                json!({"users": [{"id": "1", "name": "Alice"}]}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        // User 1 is created
        let mutation_response = json!({
            "data": {
                "createUser": {
                    "__typename": "CreateUserSuccess",
                    "user": {"id": "2", "name": "Bob"},
                    "cascade": {
                        "invalidations": {
                            "updated": [{"type": "User", "id": "2"}],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();

        // "users:all" should be invalidated (accesses all users, new user created)
        // Actually, cascade shows User:2, and users:all accesses User:*, so it's invalidated
        assert_eq!(invalidated, 1);
    }

    #[test]
    fn test_mutation_selective_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache queries for different users
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:2".to_string(),
                json!({"user": {"id": "2"}}),
                vec![("User".to_string(), "2".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:post:100".to_string(),
                json!({"post": {"id": "100"}}),
                vec![("Post".to_string(), "100".to_string())],
            )
            .unwrap();

        // Only User 1 is updated
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

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 1); // Only query:user:1

        // Check what's still cached
        assert!(cache.get("query:user:1").unwrap().is_none()); // Invalidated
        assert!(cache.get("query:user:2").unwrap().is_some()); // Still cached
        assert!(cache.get("query:post:100").unwrap().is_some()); // Still cached
    }

    #[test]
    fn test_mutation_multiple_invalidations() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache multiple queries
        cache
            .put(
                "query:user:1".to_string(),
                json!({}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:2".to_string(),
                json!({}),
                vec![("User".to_string(), "2".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:users:all".to_string(),
                json!({}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        // Cascade: User 1 and 2 updated
        let mutation_response = json!({
            "data": {
                "bulkUpdateUsers": {
                    "__typename": "BulkUpdateSuccess",
                    "updatedCount": 2,
                    "cascade": {
                        "invalidations": {
                            "updated": [
                                {"type": "User", "id": "1"},
                                {"type": "User", "id": "2"}
                            ],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        // Should invalidate: user:1, user:2, and users:all (accesses all)
        // users:all gets invalidated for wildcard User:*
        assert_eq!(invalidated, 3);

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0); // All cleared
    }

    #[test]
    fn test_mutation_delete_cascade() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache queries
        cache
            .put(
                "query:user:1".to_string(),
                json!({}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:users:all".to_string(),
                json!({}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        // User 1 is deleted
        let mutation_response = json!({
            "data": {
                "deleteUser": {
                    "__typename": "DeleteUserSuccess",
                    "user": null,
                    "cascade": {
                        "invalidations": {
                            "updated": [],
                            "deleted": [{"type": "User", "id": "1"}]
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 2);

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);
    }

    #[test]
    fn test_mutation_no_cascade_no_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache a query
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        // Mutation response without cascade (should not happen in Phase 17A, but handle gracefully)
        let mutation_response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Updated"}
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 0); // No invalidation

        // Cache entry still exists (no cascade = no invalidation)
        assert!(cache.get("query:user:1").unwrap().is_some());
    }

    #[test]
    fn test_extract_cascade_from_nested_response() {
        let response = json!({
            "data": {
                "createPost": {
                    "__typename": "CreatePostSuccess",
                    "post": {"id": "123", "title": "New Post"},
                    "cascade": {
                        "invalidations": {
                            "updated": [
                                {"type": "Post", "id": "123"},
                                {"type": "User", "id": "1"}
                            ],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let cascade = extract_cascade_from_response(&response).unwrap();
        assert!(cascade.is_some());

        let c = cascade.unwrap();
        let updated = c
            .get("invalidations")
            .unwrap()
            .get("updated")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(updated.len(), 2);
    }

    #[test]
    fn test_cascade_extraction_handles_error_responses() {
        let error_response = json!({
            "errors": [{"message": "Unauthorized"}]
        });

        let cascade = extract_cascade_from_response(&error_response).unwrap();
        assert!(cascade.is_none()); // No cascade in error response
    }

    #[test]
    fn test_mutation_workflow_end_to_end() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // User 1 is queried
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        // Verify cache hit
        let cached = cache.get("query:user:1").unwrap();
        assert!(cached.is_some());
        assert_eq!(
            cached.unwrap().get("user").unwrap().get("name"),
            Some(&"Alice".into())
        );

        // User 1 is updated (mutation)
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

        // Invalidate cache based on cascade
        let invalidated = invalidate_cache_on_mutation(&cache, &mutation_response).unwrap();
        assert_eq!(invalidated, 1);

        // Cache miss after invalidation
        let cached_after = cache.get("query:user:1").unwrap();
        assert!(cached_after.is_none());

        // Next query would hit database and refresh cache
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1", "name": "Alice Updated"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        // Cache hit with fresh data
        let cached_fresh = cache.get("query:user:1").unwrap();
        assert!(cached_fresh.is_some());
        assert_eq!(
            cached_fresh.unwrap().get("user").unwrap().get("name"),
            Some(&"Alice Updated".into())
        );
    }
}
