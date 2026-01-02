//! Comprehensive APQ (Automatic Persisted Queries) test suite
//!
//! Tests covering:
//! - Query hashing (SHA-256)
//! - Memory backend with LRU eviction
//! - APQ request handling
//! - Metrics tracking
//! - Edge cases and error handling

#[cfg(test)]
mod apq_hasher_tests {
    use fraiseql_rs::apq::hasher::{hash_query, verify_hash};

    #[test]
    fn test_hash_query_simple() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_query_deterministic() {
        let query = "{ posts { id title } }";
        let hash1 = hash_query(query);
        let hash2 = hash_query(query);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_query_empty() {
        let hash = hash_query("");
        assert_eq!(hash.len(), 64);
        // Empty string SHA-256
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_hash_query_whitespace_sensitive() {
        let query1 = "{ users { id } }";
        let query2 = "{users{id}}";

        let hash1 = hash_query(query1);
        let hash2 = hash_query(query2);

        // Different whitespace should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_query_large() {
        let large_query = "query GetUserAndPosts($id: ID!) {
            user(id: $id) {
                id
                name
                email
                address {
                    street
                    city
                    state
                    zip
                }
                posts {
                    id
                    title
                    content
                    published
                    comments {
                        id
                        text
                        author { id name }
                    }
                }
            }
        }";

        let hash = hash_query(large_query);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_verify_hash_valid() {
        let query = "{ users { id } }";
        let hash = hash_query(query);

        assert!(verify_hash(query, &hash));
    }

    #[test]
    fn test_verify_hash_invalid() {
        let query = "{ users { id } }";
        assert!(!verify_hash(query, "invalid_hash"));
    }

    #[test]
    fn test_verify_hash_wrong_query() {
        let query1 = "{ users { id } }";
        let query2 = "{ posts { id } }";
        let hash1 = hash_query(query1);

        assert!(!verify_hash(query2, &hash1));
    }

    #[test]
    fn test_verify_hash_case_sensitive() {
        let query = "{ Users { id } }";
        let hash = hash_query(query);

        assert!(!verify_hash("{ users { id } }", &hash));
    }
}

#[cfg(test)]
mod apq_memory_backend_tests {
    use fraiseql_rs::apq::backends::MemoryApqStorage;
    use fraiseql_rs::apq::storage::ApqStorage;

    #[tokio::test]
    async fn test_memory_storage_set_and_get() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query.clone()).await.unwrap();
        let retrieved = storage.get(&hash).await.unwrap();

        assert_eq!(retrieved, Some(query));
    }

    #[tokio::test]
    async fn test_memory_storage_miss() {
        let storage = MemoryApqStorage::new(100);
        let result = storage.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_memory_storage_overwrite() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();

        storage
            .set(hash.clone(), "query1".to_string())
            .await
            .unwrap();
        storage
            .set(hash.clone(), "query2".to_string())
            .await
            .unwrap();

        let retrieved = storage.get(&hash).await.unwrap();
        assert_eq!(retrieved, Some("query2".to_string()));
    }

    #[tokio::test]
    async fn test_memory_storage_exists() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();

        storage
            .set(hash.clone(), "{ users { id } }".to_string())
            .await
            .unwrap();

        assert!(storage.exists(&hash).await.unwrap());
        assert!(!storage.exists("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_storage_remove() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();

        storage
            .set(hash.clone(), "{ users { id } }".to_string())
            .await
            .unwrap();
        assert!(storage.exists(&hash).await.unwrap());

        storage.remove(&hash).await.unwrap();
        assert!(!storage.exists(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_storage_lru_eviction() {
        let storage = MemoryApqStorage::new(3); // Small capacity

        // Fill cache
        storage
            .set("hash1".to_string(), "query1".to_string())
            .await
            .unwrap();
        storage
            .set("hash2".to_string(), "query2".to_string())
            .await
            .unwrap();
        storage
            .set("hash3".to_string(), "query3".to_string())
            .await
            .unwrap();

        // Size should be full
        assert_eq!(storage.size().await, 3);

        // Add new entry, should evict oldest (hash1)
        storage
            .set("hash4".to_string(), "query4".to_string())
            .await
            .unwrap();

        assert_eq!(storage.size().await, 3);
        assert!(storage.get("hash1").await.unwrap().is_none());
        assert!(storage.get("hash2").await.unwrap().is_some());
        assert!(storage.get("hash3").await.unwrap().is_some());
        assert!(storage.get("hash4").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_memory_storage_lru_access_updates() {
        let storage = MemoryApqStorage::new(2);

        // Fill cache
        storage
            .set("hash1".to_string(), "query1".to_string())
            .await
            .unwrap();
        storage
            .set("hash2".to_string(), "query2".to_string())
            .await
            .unwrap();

        // Access hash1, making it recently used
        let _ = storage.get("hash1").await;

        // Add new entry, should evict hash2 (least recently used)
        storage
            .set("hash3".to_string(), "query3".to_string())
            .await
            .unwrap();

        assert!(storage.get("hash1").await.unwrap().is_some());
        assert!(storage.get("hash2").await.unwrap().is_none());
        assert!(storage.get("hash3").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_memory_storage_clear() {
        let storage = MemoryApqStorage::new(100);

        storage
            .set("hash1".to_string(), "query1".to_string())
            .await
            .unwrap();
        storage
            .set("hash2".to_string(), "query2".to_string())
            .await
            .unwrap();
        assert_eq!(storage.size().await, 2);

        storage.clear().await.unwrap();
        assert_eq!(storage.size().await, 0);
    }

    #[tokio::test]
    async fn test_memory_storage_stats() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();

        storage
            .set(hash.clone(), "{ users { id } }".to_string())
            .await
            .unwrap();

        // Record hit
        let _ = storage.get(&hash).await;
        // Record miss
        let _ = storage.get("nonexistent").await;

        let stats = storage.stats().await.unwrap();
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.extra["hits"], 1);
        assert_eq!(stats.extra["misses"], 1);
    }

    #[tokio::test]
    async fn test_memory_storage_capacity() {
        let storage = MemoryApqStorage::new(500);
        assert_eq!(storage.capacity().await, 500);
    }
}

#[cfg(test)]
mod apq_metrics_tests {
    use fraiseql_rs::apq::metrics::ApqMetrics;

    #[test]
    fn test_metrics_initialization() {
        let metrics = ApqMetrics::default();

        assert_eq!(metrics.get_hits(), 0);
        assert_eq!(metrics.get_misses(), 0);
        assert_eq!(metrics.get_stored(), 0);
        assert_eq!(metrics.get_errors(), 0);
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_metrics_record_hit() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        metrics.record_hit();

        assert_eq!(metrics.get_hits(), 2);
    }

    #[test]
    fn test_metrics_record_miss() {
        let metrics = ApqMetrics::default();
        metrics.record_miss();

        assert_eq!(metrics.get_misses(), 1);
    }

    #[test]
    fn test_metrics_record_store() {
        let metrics = ApqMetrics::default();
        metrics.record_store();
        metrics.record_store();

        assert_eq!(metrics.get_stored(), 2);
    }

    #[test]
    fn test_metrics_record_error() {
        let metrics = ApqMetrics::default();
        metrics.record_error();

        assert_eq!(metrics.get_errors(), 1);
    }

    #[test]
    fn test_metrics_hit_rate_perfect() {
        let metrics = ApqMetrics::default();
        for _ in 0..100 {
            metrics.record_hit();
        }

        assert_eq!(metrics.hit_rate(), 1.0);
    }

    #[test]
    fn test_metrics_hit_rate_zero() {
        let metrics = ApqMetrics::default();
        for _ in 0..100 {
            metrics.record_miss();
        }

        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_metrics_hit_rate_mixed() {
        let metrics = ApqMetrics::default();
        for _ in 0..90 {
            metrics.record_hit();
        }
        for _ in 0..10 {
            metrics.record_miss();
        }

        assert!((metrics.hit_rate() - 0.9).abs() < 0.0001);
    }

    #[test]
    fn test_metrics_as_json() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();
        metrics.record_store();
        metrics.record_error();

        let json = metrics.as_json();

        assert_eq!(json["hits"], 3);
        assert_eq!(json["misses"], 1);
        assert_eq!(json["stored"], 1);
        assert_eq!(json["errors"], 1);
        assert!((json["hit_rate"].as_f64().unwrap() - 0.75).abs() < 0.0001);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();
        metrics.record_store();

        assert!(metrics.get_hits() > 0);

        metrics.reset();

        assert_eq!(metrics.get_hits(), 0);
        assert_eq!(metrics.get_misses(), 0);
        assert_eq!(metrics.get_stored(), 0);
        assert_eq!(metrics.get_errors(), 0);
    }
}

#[cfg(test)]
mod apq_handler_tests {
    use fraiseql_rs::apq::backends::MemoryApqStorage;
    use fraiseql_rs::apq::hasher::hash_query;
    use fraiseql_rs::apq::{ApqExtensions, ApqHandler, ApqResponse, PersistedQuery};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_apq_handler_query_not_in_cache() {
        let storage = Arc::new(MemoryApqStorage::new(100));
        let handler = ApqHandler::new(storage);

        let query = "{ users { id } }".to_string();
        let hash = hash_query(&query);

        let extensions = ApqExtensions {
            persisted_query: Some(PersistedQuery {
                version: 1,
                sha256_hash: hash.clone(),
            }),
        };

        // Query not in cache, client provides full query
        let response = handler
            .handle_request(Some(extensions), Some(query.clone()))
            .await
            .unwrap();

        // Should store and return query
        match response {
            ApqResponse::QueryFound(q) => assert_eq!(q, query),
            _ => panic!("Expected QueryFound"),
        }

        // Verify metrics
        assert_eq!(handler.metrics().get_misses(), 1);
        assert_eq!(handler.metrics().get_stored(), 1);
    }

    #[tokio::test]
    async fn test_apq_handler_query_in_cache() {
        let storage = Arc::new(MemoryApqStorage::new(100));
        let handler = ApqHandler::new(storage.clone());

        let query = "{ users { id } }".to_string();
        let hash = hash_query(&query);

        // Pre-populate cache
        storage.set(hash.clone(), query.clone()).await.unwrap();

        let extensions = ApqExtensions {
            persisted_query: Some(PersistedQuery {
                version: 1,
                sha256_hash: hash,
            }),
        };

        // Query in cache, client doesn't send full query
        let response = handler
            .handle_request(Some(extensions), None)
            .await
            .unwrap();

        // Should return cached query
        match response {
            ApqResponse::QueryFound(q) => assert_eq!(q, query),
            _ => panic!("Expected QueryFound"),
        }

        // Verify metrics
        assert_eq!(handler.metrics().get_hits(), 1);
    }

    #[tokio::test]
    async fn test_apq_handler_no_extensions() {
        let storage = Arc::new(MemoryApqStorage::new(100));
        let handler = ApqHandler::new(storage);

        let query = "{ users { id } }".to_string();

        // No APQ extensions
        let response = handler
            .handle_request(None, Some(query.clone()))
            .await
            .unwrap();

        match response {
            ApqResponse::QueryFound(q) => assert_eq!(q, query),
            _ => panic!("Expected QueryFound"),
        }
    }

    #[tokio::test]
    async fn test_apq_handler_metrics() {
        let storage = Arc::new(MemoryApqStorage::new(100));
        let handler = ApqHandler::new(storage);

        // Simulate hits and misses
        for _ in 0..3 {
            let _ = handler
                .handle_request(None, Some("query".to_string()))
                .await;
        }

        let metrics_json = handler.metrics().as_json();
        assert!(metrics_json["hit_rate"].as_f64().is_some());
    }
}

#[cfg(test)]
mod apq_integration_tests {
    use fraiseql_rs::apq::backends::MemoryApqStorage;
    use fraiseql_rs::apq::hasher::{hash_query, verify_hash};
    use fraiseql_rs::apq::storage::ApqStorage;

    #[tokio::test]
    async fn test_full_apq_workflow() {
        let storage = MemoryApqStorage::new(100);

        // Step 1: Client sends full query
        let query1 = "query GetUsers { users { id name email } }";
        let hash1 = hash_query(query1);

        // Store query
        storage
            .set(hash1.clone(), query1.to_string())
            .await
            .unwrap();

        // Verify hash
        assert!(verify_hash(query1, &hash1));

        // Step 2: Client sends only hash (APQ hit)
        let retrieved = storage.get(&hash1).await.unwrap();
        assert_eq!(retrieved, Some(query1.to_string()));

        // Step 3: Query another without APQ
        let query2 = "query GetPosts { posts { id title } }";
        assert!(storage.get(&hash_query(query2)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_apq_bandwidth_comparison() {
        let query = "query GetUserWithAllRelations($userId: ID!) {
            user(id: $userId) {
                id
                name
                email
                phone
                address { street city state zip }
                posts { id title content }
                comments { id text }
            }
        }";

        let hash = hash_query(query);

        // Original: full query sent
        let full_size = query.len();

        // APQ: only hash sent
        let apq_size = hash.len();

        // APQ should be significantly smaller
        let reduction = (1.0 - (apq_size as f64 / full_size as f64)) * 100.0;
        assert!(
            reduction > 90.0,
            "APQ bandwidth reduction: {:.1}%",
            reduction
        );
    }

    #[tokio::test]
    async fn test_apq_concurrent_access() {
        let storage = std::sync::Arc::new(MemoryApqStorage::new(1000));

        let mut handles = vec![];

        // Spawn multiple concurrent writers
        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                for j in 0..10 {
                    let hash = format!("hash_{}_{}", i, j);
                    let query = format!("query_{}", j);
                    storage_clone.set(hash, query).await.unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify total size
        assert_eq!(storage.size().await, 100);
    }
}
