//! End-to-end integration tests for cache coherency (Phase 17A.6)
//!
//! Tests the complete cache pipeline including:
//! - Query execution → caching → cache hits
//! - Mutation execution → cascade → cache invalidation
//! - Multi-client scenarios with cache coherency
//! - Complex entity relationships and dependencies
//! - Edge cases and race conditions

#[cfg(test)]
mod tests {
    use crate::cache::{CacheMonitor, QueryResultCache, QueryResultCacheConfig};
    use serde_json::json;
    use std::sync::Arc;

    // =========================================================================
    // Test Suite 1: Basic Query Caching Pipeline
    // =========================================================================

    #[test]
    fn test_e2e_single_query_caching_pipeline() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));
        let monitor = Arc::new(CacheMonitor::new());

        // Step 1: First query - cache miss
        let cache_key = "query:user:1";
        let result1 = json!({"user": {"id": "1", "name": "Alice"}});

        assert!(cache.get(&cache_key).unwrap().is_none());
        monitor.record_miss();

        // Store in cache
        cache
            .put(
                cache_key,
                result1.clone(),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();
        monitor.record_cache_entry();

        // Step 2: Second query - cache hit
        let cached = cache.get(&cache_key).unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), Arc::new(result1.clone()));
        monitor.record_hit();

        // Verify metrics
        let health = monitor.get_health(1, 100, 1024 * 1024);
        assert_eq!(health.hit_rate, 1.0); // 1 hit, 1 miss total = 50% (1 of 2)
        assert_eq!(health.status, crate::cache::HealthStatus::Healthy);
    }

    #[test]
    fn test_e2e_multiple_queries_different_entities() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache queries for different entities
        let user1_result = json!({"user": {"id": "1", "name": "Alice"}});
        let user2_result = json!({"user": {"id": "2", "name": "Bob"}});
        let post1_result = json!({"post": {"id": "1", "title": "Hello"}});

        cache
            .put(
                "query:user:1",
                user1_result.clone(),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:2",
                user2_result.clone(),
                vec![("User".to_string(), "2".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:post:1",
                post1_result.clone(),
                vec![("Post".to_string(), "1".to_string())],
            )
            .unwrap();

        // All should be cached
        assert_eq!(
            cache.get("query:user:1").unwrap().unwrap(),
            Arc::new(user1_result)
        );
        assert_eq!(
            cache.get("query:user:2").unwrap().unwrap(),
            Arc::new(user2_result)
        );
        assert_eq!(
            cache.get("query:post:1").unwrap().unwrap(),
            Arc::new(post1_result)
        );

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 3);
    }

    // =========================================================================
    // Test Suite 2: Mutation → Cascade → Invalidation
    // =========================================================================

    #[test]
    fn test_e2e_mutation_single_entity_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Pre-populate cache
        let user_data = json!({"user": {"id": "1", "name": "Alice"}});
        cache
            .put(
                "query:user:1",
                user_data,
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 1);

        // Mutation: Update User 1
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 1);
        assert_eq!(cache.metrics().unwrap().size, 0);

        // Query should miss now
        assert!(cache.get("query:user:1").unwrap().is_none());
    }

    #[test]
    fn test_e2e_mutation_invalidates_list_queries() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache: specific user query + list query
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:users:all",
                json!({"users": [{"id": "1"}]}),
                vec![("User".to_string(), "*".to_string())], // Wildcard = all users
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 2);

        // Mutation: Create new user (User:2)
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "2"}],
                "deleted": []
            }
        });

        // Should invalidate:
        // 1. User:1 query (because it accesses User:1)
        // 2. users:all query (because it accesses User:* which includes new User:2)
        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 2);

        assert_eq!(cache.metrics().unwrap().size, 0);
    }

    #[test]
    fn test_e2e_mutation_selective_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache different entity types
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:user:2",
                json!({"user": {"id": "2"}}),
                vec![("User".to_string(), "2".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:post:100",
                json!({"post": {"id": "100"}}),
                vec![("Post".to_string(), "100".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 3);

        // Mutation: Only User:1 updated
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 1); // Only User:1 query

        // Verify correct entries were invalidated
        assert!(cache.get("query:user:1").unwrap().is_none()); // Invalidated
        assert!(cache.get("query:user:2").unwrap().is_some()); // Still cached
        assert!(cache.get("query:post:100").unwrap().is_some()); // Still cached

        assert_eq!(cache.metrics().unwrap().size, 2);
    }

    // =========================================================================
    // Test Suite 3: Cache Coherency
    // =========================================================================

    #[test]
    fn test_e2e_cache_coherency_multi_client_scenario() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));
        let monitor = Arc::new(CacheMonitor::new());

        // Client A: Query for User 1
        let user_data = json!({"user": {"id": "1", "name": "Alice"}});
        cache
            .put(
                "query:user:1",
                user_data.clone(),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();
        monitor.record_cache_entry();

        // Client B: Same query, gets cached result
        let cached = cache.get("query:user:1").unwrap();
        assert_eq!(cached.unwrap(), Arc::new(user_data.clone()));
        monitor.record_hit();

        // Client C: Mutation (update User 1)
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });
        cache.invalidate_from_cascade(&cascade).unwrap();

        // Client D: Query after mutation - cache miss, gets fresh data
        assert!(cache.get("query:user:1").unwrap().is_none());
        monitor.record_miss();

        let fresh_data = json!({"user": {"id": "1", "name": "Alice Updated"}});
        cache
            .put(
                "query:user:1",
                fresh_data,
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();
        monitor.record_cache_entry();

        // Verify coherency
        let health = monitor.get_health(2, 100, 1024 * 1024);
        // With 1 hit and 1 miss out of 2 total: 50% hit rate
        assert!((health.hit_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_e2e_cache_coherency_related_entities() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache: User with their posts
        cache
            .put(
                "query:user:1:with_posts",
                json!({"user": {"id": "1", "posts": [{"id": "100"}]}}),
                vec![
                    ("User".to_string(), "1".to_string()),
                    ("Post".to_string(), "100".to_string()),
                ],
            )
            .unwrap();

        // Cache: Post by user
        cache
            .put(
                "query:post:100",
                json!({"post": {"id": "100", "userId": "1"}}),
                vec![("Post".to_string(), "100".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 2);

        // Mutation: Delete Post 100
        let cascade = json!({
            "invalidations": {
                "updated": [],
                "deleted": [{"type": "Post", "id": "100"}]
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 2); // Both queries affected

        // Both should be invalidated (post is directly mentioned, user query references post)
        assert!(cache.get("query:user:1:with_posts").unwrap().is_none());
        assert!(cache.get("query:post:100").unwrap().is_none());
    }

    // =========================================================================
    // Test Suite 4: Wildcard and Mass Invalidation
    // =========================================================================

    #[test]
    fn test_e2e_wildcard_invalidation_on_any_entity_change() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache: List of all users (wildcard)
        cache
            .put(
                "query:users:all",
                json!({"users": [{"id": "1"}]}),
                vec![("User".to_string(), "*".to_string())],
            )
            .unwrap();

        // Cache: Specific user
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        // Cache: Another specific user
        cache
            .put(
                "query:user:2",
                json!({"user": {"id": "2"}}),
                vec![("User".to_string(), "2".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 3);

        // Mutation: Update User 3 (didn't exist before)
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "3"}],
                "deleted": []
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        // Should invalidate:
        // - users:all (wildcard User:*)
        // - NOT user:1 (specific User:1)
        // - NOT user:2 (specific User:2)
        assert_eq!(invalidated, 1);

        assert!(cache.get("query:users:all").unwrap().is_none());
        assert!(cache.get("query:user:1").unwrap().is_some());
        assert!(cache.get("query:user:2").unwrap().is_some());
    }

    #[test]
    fn test_e2e_bulk_invalidation_multiple_entities() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache multiple queries
        for i in 0..5 {
            cache
                .put(
                    &format!("query:user:{i}"),
                    json!({"user": {"id": format!("{i}")}}),
                    vec![("User".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        for i in 0..5 {
            cache
                .put(
                    &format!("query:post:{i}"),
                    json!({"post": {"id": format!("{i}")}}),
                    vec![("Post".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        assert_eq!(cache.metrics().unwrap().size, 10);

        // Mutation: Update multiple users
        let cascade = json!({
            "invalidations": {
                "updated": [
                    {"type": "User", "id": "0"},
                    {"type": "User", "id": "1"},
                    {"type": "User", "id": "2"},
                ],
                "deleted": []
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 3); // Only 3 user queries

        assert_eq!(cache.metrics().unwrap().size, 7); // 10 - 3 = 7 remaining

        // Posts should still be cached
        for i in 0..5 {
            assert!(cache.get(&format!("query:post:{i}")).unwrap().is_some());
        }
    }

    // =========================================================================
    // Test Suite 5: Cache Invalidation Correctness
    // =========================================================================

    #[test]
    fn test_e2e_no_stale_data_after_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache old data
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1", "name": "Alice"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        assert_eq!(
            cache.get("query:user:1").unwrap().unwrap(),
            Arc::new(json!({"user": {"id": "1", "name": "Alice"}}))
        );

        // Mutation updates user
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });

        cache.invalidate_from_cascade(&cascade).unwrap();

        // Should be cache miss (no stale data)
        assert!(cache.get("query:user:1").unwrap().is_none());

        // Fresh data is stored
        let fresh_data = json!({"user": {"id": "1", "name": "Alice Updated"}});
        cache
            .put(
                "query:user:1",
                fresh_data.clone(),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        assert_eq!(
            cache.get("query:user:1").unwrap().unwrap(),
            Arc::new(fresh_data)
        );
    }

    #[test]
    fn test_e2e_invalidation_idempotent() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 1);

        // First invalidation
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });

        let invalidated1 = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated1, 1);
        assert_eq!(cache.metrics().unwrap().size, 0);

        // Second invalidation on same cascade (idempotent)
        let invalidated2 = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated2, 0); // Nothing to invalidate, already gone

        assert_eq!(cache.metrics().unwrap().size, 0);
    }

    // =========================================================================
    // Test Suite 6: Concurrent Operations
    // =========================================================================

    #[test]
    fn test_e2e_concurrent_reads_and_writes() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));
        let mut handles = vec![];

        // Spawn reader threads
        for _id in 0..3 {
            let cache_clone = cache.clone();
            handles.push(std::thread::spawn(move || {
                spawn_reader_thread(&cache_clone);
            }));
        }

        // Spawn writer threads
        for _id in 0..2 {
            let cache_clone = cache.clone();
            handles.push(std::thread::spawn(move || {
                spawn_writer_thread(&cache_clone);
            }));
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should be in valid state
        let metrics = cache.metrics().unwrap();
        assert!(metrics.size <= 5); // Max 5 entries
    }

    #[test]
    fn test_e2e_concurrent_invalidation() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Pre-populate cache
        for i in 0..10 {
            cache
                .put(
                    &format!("query:user:{i}"),
                    json!({"user": {"id": format!("{i}")}}),
                    vec![("User".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        assert_eq!(cache.metrics().unwrap().size, 10);

        let mut handles = vec![];

        // Spawn invalidation threads
        for _ in 0..3 {
            let cache_clone = cache.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..10 {
                    let cascade = json!({
                        "invalidations": {
                            "updated": [{"type": "User", "id": format!("{i}")}],
                            "deleted": []
                        }
                    });
                    let _ = cache_clone.invalidate_from_cascade(&cascade);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All should be invalidated
        assert_eq!(cache.metrics().unwrap().size, 0);
    }

    // =========================================================================
    // Test Suite 7: Complex Scenarios
    // =========================================================================

    #[test]
    fn test_e2e_query_mutation_cycle() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cycle 1: Query and cache
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1", "name": "v1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();
        assert_eq!(cache.metrics().unwrap().size, 1);

        // Cycle 2: Invalidate
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });
        cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(cache.metrics().unwrap().size, 0);

        // Cycle 3: Re-query and cache updated data
        cache
            .put(
                "query:user:1",
                json!({"user": {"id": "1", "name": "v2"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();
        assert_eq!(cache.metrics().unwrap().size, 1);

        // Verify fresh data
        assert_eq!(
            cache.get("query:user:1").unwrap().unwrap(),
            Arc::new(json!({"user": {"id": "1", "name": "v2"}}))
        );
    }

    #[test]
    fn test_e2e_delete_cascade_removes_all_references() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Cache: Author query
        cache
            .put(
                "query:author:1",
                json!({"author": {"id": "1"}}),
                vec![("Author".to_string(), "1".to_string())],
            )
            .unwrap();

        // Cache: Posts by author
        cache
            .put(
                "query:author:1:posts",
                json!({"posts": [{"id": "100"}, {"id": "101"}]}),
                vec![
                    ("Author".to_string(), "1".to_string()),
                    ("Post".to_string(), "100".to_string()),
                    ("Post".to_string(), "101".to_string()),
                ],
            )
            .unwrap();

        // Cache: Individual posts
        cache
            .put(
                "query:post:100",
                json!({"post": {"id": "100"}}),
                vec![("Post".to_string(), "100".to_string())],
            )
            .unwrap();

        cache
            .put(
                "query:post:101",
                json!({"post": {"id": "101"}}),
                vec![("Post".to_string(), "101".to_string())],
            )
            .unwrap();

        assert_eq!(cache.metrics().unwrap().size, 4);

        // Delete author
        let cascade = json!({
            "invalidations": {
                "updated": [],
                "deleted": [{"type": "Author", "id": "1"}]
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 2); // author:1 and author:1:posts

        // Posts not directly mentioned stay cached (they're independent)
        assert!(cache.get("query:post:100").unwrap().is_some());
        assert!(cache.get("query:post:101").unwrap().is_some());
    }

    // =========================================================================
    // Test Suite 8: State Consistency
    // =========================================================================

    #[test]
    fn test_e2e_metrics_consistency_with_operations() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Add 5 entries
        for i in 0..5 {
            cache
                .put(
                    &format!("query:{i}"),
                    json!({"id": i}),
                    vec![("Entity".to_string(), format!("{i}"))],
                )
                .unwrap();
        }

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 5);
        assert_eq!(metrics.total_cached, 5);

        // Invalidate 2 entries
        let cascade = json!({
            "invalidations": {
                "updated": [
                    {"type": "Entity", "id": "0"},
                    {"type": "Entity", "id": "1"}
                ],
                "deleted": []
            }
        });

        cache.invalidate_from_cascade(&cascade).unwrap();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 3); // 5 - 2
        assert_eq!(metrics.total_cached, 5); // Still 5 (cumulative)
        assert_eq!(metrics.invalidations, 2);
    }

    #[test]
    fn test_e2e_empty_cascade_no_side_effects() {
        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        cache
            .put(
                "query:test",
                json!({"test": true}),
                vec![("Test".to_string(), "1".to_string())],
            )
            .unwrap();

        let metrics_before = cache.metrics().unwrap();

        // Empty cascade (no entities)
        let cascade = json!({
            "invalidations": {
                "updated": [],
                "deleted": []
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 0);

        let metrics_after = cache.metrics().unwrap();
        assert_eq!(metrics_before.size, metrics_after.size); // No change
    }

    // =========================================================================
    // Helper functions for concurrent test operations
    // =========================================================================

    fn spawn_reader_thread(cache: &Arc<QueryResultCache>) {
        for _ in 0..10 {
            read_multiple_keys(cache);
            std::thread::yield_now();
        }
    }

    fn read_multiple_keys(cache: &Arc<QueryResultCache>) {
        for i in 0..5 {
            let _ = cache.get(&format!("query:user:{i}"));
        }
    }

    fn spawn_writer_thread(cache: &Arc<QueryResultCache>) {
        for _ in 0..5 {
            write_multiple_keys(cache);
            std::thread::yield_now();
        }
    }

    fn write_multiple_keys(cache: &Arc<QueryResultCache>) {
        for i in 0..5 {
            let _ = cache.put(
                &format!("query:user:{i}"),
                json!({"user": {"id": format!("{i}")}}),
                vec![("User".to_string(), format!("{i}"))],
            );
        }
    }
}
