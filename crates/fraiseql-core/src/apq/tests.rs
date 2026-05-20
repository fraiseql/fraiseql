//! Tests for `apq/` modules.
//! Re-export items not in `crate::apq::*` so submodules reach them via `use super::*`.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience

pub use std::time::Duration;

pub use serde_json::json;

mod hasher_tests {

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_hash_query_deterministic() {
        let query = "{ users { id name } }";
        let hash1 = hash_query(query);
        let hash2 = hash_query(query);

        // Hash should be deterministic
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_query_length() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        // SHA-256 hex is 64 characters
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_hash_query_hex_format() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        // Should only contain hex characters
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_verify_hash_valid() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        assert!(verify_hash(query, &hash));
    }

    #[test]
    fn test_verify_hash_invalid() {
        let query = "{ users { id name } }";
        assert!(!verify_hash(query, "invalid_hash"));
    }

    #[test]
    fn test_verify_hash_rejects_wrong_length_hash() {
        // A hash that is not exactly 64 hex chars must be rejected immediately.
        let query = "{ users { id } }";
        assert!(!verify_hash(query, ""), "empty string rejected");
        assert!(!verify_hash(query, "abc123"), "short string rejected");
        // 65 chars — one too long
        let too_long = "a".repeat(65);
        assert!(!verify_hash(query, &too_long), "65-char string rejected");
    }

    #[test]
    fn test_verify_hash_with_variables_rejects_wrong_length_hash() {
        use serde_json::json;
        let query = "{ users { id } }";
        let vars = json!({"limit": 10});
        assert!(!verify_hash_with_variables(query, &vars, "tooshort"));
        let too_long = "b".repeat(65);
        assert!(!verify_hash_with_variables(query, &vars, &too_long));
    }

    #[test]
    fn test_different_queries_different_hashes() {
        let query1 = "{ users { id } }";
        let query2 = "{ users { name } }";

        let hash1 = hash_query(query1);
        let hash2 = hash_query(query2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_whitespace_affects_hash() {
        let query1 = "{ users { id } }";
        let query2 = "{users{id}}"; // No whitespace

        let hash1 = hash_query(query1);
        let hash2 = hash_query(query2);

        // Different whitespace = different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_empty_query() {
        let hash = hash_query("");
        assert_eq!(hash.len(), 64);
        // Empty string has a well-known SHA-256 hash
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_hash_large_query() {
        let large_query =
            "{ users { id name email address { street city state zip } posts { id title } } }";
        let hash = hash_query(large_query);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // =================================================================
    // SECURITY CRITICAL TESTS: Variable-aware hashing to prevent data leakage
    // =================================================================

    #[test]
    fn test_hash_query_with_variables_deterministic() {
        use serde_json::json;

        let query = "query getUser($id: ID!) { user(id: $id) { name } }";
        let vars = json!({"id": "123"});

        let hash1 = hash_query_with_variables(query, &vars);
        let hash2 = hash_query_with_variables(query, &vars);

        assert_eq!(hash1, hash2, "Same variables must produce same hash");
    }

    #[test]
    fn test_hash_query_with_variables_different_values_produce_different_hashes() {
        use serde_json::json;

        let query = "query getUser($id: ID!) { user(id: $id) { name } }";

        let vars1 = json!({"id": "user-123"});
        let vars2 = json!({"id": "user-456"});

        let hash1 = hash_query_with_variables(query, &vars1);
        let hash2 = hash_query_with_variables(query, &vars2);

        assert_ne!(
            hash1, hash2,
            "Different variable values MUST produce different hashes (SECURITY)"
        );
    }

    #[test]
    fn test_hash_query_with_variables_different_param_names_different_hashes() {
        use serde_json::json;

        let query = "{ users { id } }";

        let vars1 = json!({"limit": 10});
        let vars2 = json!({"offset": 10});

        let hash1 = hash_query_with_variables(query, &vars1);
        let hash2 = hash_query_with_variables(query, &vars2);

        assert_ne!(hash1, hash2, "Different parameter names must produce different hashes");
    }

    #[test]
    fn test_hash_query_with_empty_variables_uses_query_hash_only() {
        use serde_json::json;

        let query = "{ users { id } }";
        let empty_vars = json!({});

        let hash_with_empty = hash_query_with_variables(query, &empty_vars);
        let hash_query_only = hash_query(query);

        assert_eq!(hash_with_empty, hash_query_only, "Empty variables should use query hash only");
    }

    #[test]
    fn test_hash_query_with_null_variables_uses_query_hash_only() {
        use serde_json::Value;

        let query = "{ users { id } }";
        let null_vars = Value::Null;

        let hash_with_null = hash_query_with_variables(query, &null_vars);
        let hash_query_only = hash_query(query);

        assert_eq!(hash_with_null, hash_query_only, "Null variables should use query hash only");
    }

    #[test]
    fn test_hash_query_with_variables_multiple_params() {
        use serde_json::json;

        let query =
            "query search($q: String!, $limit: Int!) { search(q: $q, limit: $limit) { id } }";

        let vars = json!({"q": "test", "limit": 50});

        let hash = hash_query_with_variables(query, &vars);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_query_with_variables_complex_nested_variables() {
        use serde_json::json;

        let query = "mutation createUser($input: UserInput!) { createUser(input: $input) { id } }";

        let vars = json!({
            "input": {
                "name": "Alice",
                "email": "alice@example.com",
                "roles": ["admin", "user"],
                "metadata": {
                    "tier": "premium",
                    "verified": true
                }
            }
        });

        let hash = hash_query_with_variables(query, &vars);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_query_with_variables_key_order_independence() {
        use serde_json::json;

        let query = "{ users { id } }";

        // Same variables, different JSON key order
        let vars1 = json!({"a": 1, "b": 2, "c": 3});
        let vars2 = json!({"c": 3, "a": 1, "b": 2});

        let hash1 = hash_query_with_variables(query, &vars1);
        let hash2 = hash_query_with_variables(query, &vars2);

        assert_eq!(hash1, hash2, "Variable key order must not affect hash (JSON normalized)");
    }

    #[test]
    fn test_verify_hash_with_variables_valid() {
        use serde_json::json;

        let query = "{ users { id } }";
        let vars = json!({"limit": 10});

        let hash = hash_query_with_variables(query, &vars);

        assert!(verify_hash_with_variables(query, &vars, &hash));
    }

    #[test]
    fn test_verify_hash_with_variables_invalid() {
        use serde_json::json;

        let query = "{ users { id } }";
        let vars = json!({"limit": 10});

        assert!(!verify_hash_with_variables(query, &vars, "invalid_hash"));
    }

    #[test]
    fn test_verify_hash_with_variables_different_variables_fails() {
        use serde_json::json;

        let query = "query getUser($id: ID!) { user(id: $id) { name } }";

        let vars_original = json!({"id": "123"});
        let vars_different = json!({"id": "456"});

        let hash = hash_query_with_variables(query, &vars_original);

        // Verification fails if variables don't match
        assert!(!verify_hash_with_variables(query, &vars_different, &hash));
    }

    #[test]
    fn test_hash_deterministic_across_runs() {
        use serde_json::json;

        let query = "query test($x: Int!) { test(x: $x) { id } }";
        let vars = json!({"x": 42});

        // Run the hash multiple times
        let hashes: Vec<String> =
            (0..10).map(|_| hash_query_with_variables(query, &vars)).collect();

        // All hashes should be identical
        for i in 1..10 {
            assert_eq!(hashes[0], hashes[i], "Hash must be deterministic across multiple runs");
        }
    }

    #[test]
    fn test_hash_query_with_variables_length() {
        use serde_json::json;

        let query = "{ users { id } }";
        let vars = json!({"limit": 10});

        let hash = hash_query_with_variables(query, &vars);

        // SHA-256 hex is 64 characters
        assert_eq!(hash.len(), 64, "Combined hash must be SHA-256 length (64 hex chars)");
    }

    #[test]
    fn test_hash_query_with_variables_hex_format() {
        use serde_json::json;

        let query = "{ users { id } }";
        let vars = json!({"limit": 10});

        let hash = hash_query_with_variables(query, &vars);

        // Should only contain hex characters
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Combined hash must be valid hexadecimal"
        );
    }

    // SECURITY TEST: Simulates the data leakage vulnerability
    #[test]
    fn test_security_scenario_prevents_data_leakage() {
        use serde_json::json;

        // Scenario: Same query, different user IDs
        let query = "query getUser($userId: ID!) { user(id: $userId) { name email } }";

        // User A's request
        let alice_vars = json!({"userId": "alice-uuid-123"});
        let alice_cache_key = hash_query_with_variables(query, &alice_vars);

        // User B's request with different ID
        let bob_vars = json!({"userId": "bob-uuid-456"});
        let bob_cache_key = hash_query_with_variables(query, &bob_vars);

        // CRITICAL: Different variables MUST produce different cache keys
        assert_ne!(
            alice_cache_key, bob_cache_key,
            "SECURITY: Different user IDs must produce different cache keys to prevent data leakage"
        );

        // Even if cached, verification should fail with wrong variables
        assert!(
            !verify_hash_with_variables(query, &bob_vars, &alice_cache_key),
            "SECURITY: Cache hit should not occur with different variables"
        );
    }
}

mod memory_storage_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    #[tokio::test]
    async fn set_and_get() {
        let store = InMemoryApqStorage::default();
        store.set("abc123".to_string(), "{ users { id } }".to_string()).await.unwrap();

        let result = store.get("abc123").await.unwrap();
        assert_eq!(result, Some("{ users { id } }".to_string()));
    }

    #[tokio::test]
    async fn missing_hash_returns_none() {
        let store = InMemoryApqStorage::default();
        let result = store.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test(start_paused = true)]
    async fn ttl_expiry() {
        let store = InMemoryApqStorage::with_ttl(100, Duration::from_millis(50));
        store.set("h1".to_string(), "query1".to_string()).await.unwrap();

        // Should be present immediately.
        assert!(store.get("h1").await.unwrap().is_some());

        // Advance frozen time past the TTL.
        tokio::time::advance(Duration::from_millis(60)).await;

        // Should be gone.
        assert!(store.get("h1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn max_entries_evicts_lru() {
        let store = InMemoryApqStorage::new(2);

        store.set("h1".to_string(), "q1".to_string()).await.unwrap();
        store.set("h2".to_string(), "q2".to_string()).await.unwrap();

        // Access h1 to make it more recently used than h2.
        store.get("h1").await.unwrap();

        // Adding h3 should evict h2 (least recently accessed).
        store.set("h3".to_string(), "q3".to_string()).await.unwrap();

        assert!(store.get("h1").await.unwrap().is_some());
        assert!(store.get("h2").await.unwrap().is_none());
        assert!(store.get("h3").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn stats() {
        let store = InMemoryApqStorage::new(10);
        store.set("h1".to_string(), "q1".to_string()).await.unwrap();
        store.set("h2".to_string(), "q2".to_string()).await.unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.total_queries, 2);
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.extra["max_entries"], 10);
    }

    #[tokio::test]
    async fn exists_and_remove() {
        let store = InMemoryApqStorage::default();
        store.set("h1".to_string(), "q1".to_string()).await.unwrap();

        assert!(store.exists("h1").await.unwrap());
        assert!(!store.exists("h2").await.unwrap());

        store.remove("h1").await.unwrap();
        assert!(!store.exists("h1").await.unwrap());
    }

    #[tokio::test]
    async fn clear() {
        let store = InMemoryApqStorage::default();
        store.set("h1".to_string(), "q1".to_string()).await.unwrap();
        store.set("h2".to_string(), "q2".to_string()).await.unwrap();

        assert_eq!(store.stats().await.unwrap().total_queries, 2);

        store.clear().await.unwrap();
        assert_eq!(store.stats().await.unwrap().total_queries, 0);
    }
}

mod metrics_tests {

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        let metrics = ApqMetrics::default();
        assert_eq!(metrics.get_hits(), 0);
        assert_eq!(metrics.get_misses(), 0);
        assert_eq!(metrics.get_stored(), 0);
        assert_eq!(metrics.get_errors(), 0);
    }

    #[test]
    fn test_record_hit() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        assert_eq!(metrics.get_hits(), 1);
    }

    #[test]
    fn test_record_multiple_hits() {
        let metrics = ApqMetrics::default();
        for _ in 0..100 {
            metrics.record_hit();
        }
        assert_eq!(metrics.get_hits(), 100);
    }

    #[test]
    fn test_record_miss() {
        let metrics = ApqMetrics::default();
        metrics.record_miss();
        assert_eq!(metrics.get_misses(), 1);
    }

    #[test]
    fn test_record_store() {
        let metrics = ApqMetrics::default();
        metrics.record_store();
        assert_eq!(metrics.get_stored(), 1);
    }

    #[test]
    fn test_record_error() {
        let metrics = ApqMetrics::default();
        metrics.record_error();
        assert_eq!(metrics.get_errors(), 1);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: test assertion comparing exact metric values
    fn test_hit_rate_no_requests() {
        let metrics = ApqMetrics::default();
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: test assertion comparing exact metric values
    fn test_hit_rate_all_hits() {
        let metrics = ApqMetrics::default();
        for _ in 0..100 {
            metrics.record_hit();
        }
        assert_eq!(metrics.hit_rate(), 1.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: test assertion comparing exact metric values
    fn test_hit_rate_all_misses() {
        let metrics = ApqMetrics::default();
        for _ in 0..100 {
            metrics.record_miss();
        }
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_hit_rate_mixed() {
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
    fn test_as_json() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();
        metrics.record_store();

        let json = metrics.as_json();
        assert_eq!(json["hits"], 2);
        assert_eq!(json["misses"], 1);
        assert_eq!(json["stored"], 1);
        assert_eq!(json["errors"], 0);
    }

    #[test]
    fn test_reset() {
        let metrics = ApqMetrics::default();
        metrics.record_hit();
        metrics.record_hit();
        metrics.record_miss();

        assert_eq!(metrics.get_hits(), 2);
        assert_eq!(metrics.get_misses(), 1);

        metrics.reset();

        assert_eq!(metrics.get_hits(), 0);
        assert_eq!(metrics.get_misses(), 0);
        assert_eq!(metrics.get_stored(), 0);
        assert_eq!(metrics.get_errors(), 0);
    }
}

#[cfg(feature = "redis-apq")]
mod redis_storage_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    /// These tests require a running Redis instance at `REDIS_URL`.
    /// Run with: `REDIS_URL=redis://localhost:6379 cargo test -p fraiseql-core --features redis-apq
    /// -- redis_apq --ignored`
    fn redis_url() -> Option<String> {
        std::env::var("REDIS_URL").ok()
    }

    #[tokio::test]
    #[ignore = "requires REDIS_URL"]
    async fn redis_apq_set_and_get() {
        let url = redis_url().expect("REDIS_URL must be set");
        let store = RedisApqStorage::new(&url).await.unwrap();
        store.clear().await.unwrap();

        let hash = "abc123";
        let query = "{ users { id name } }";

        store.set(hash.to_string(), query.to_string()).await.unwrap();
        let result = store.get(hash).await.unwrap();
        assert_eq!(result.as_deref(), Some(query));
    }

    #[tokio::test]
    #[ignore = "requires REDIS_URL"]
    async fn redis_apq_missing_returns_none() {
        let url = redis_url().expect("REDIS_URL must be set");
        let store = RedisApqStorage::new(&url).await.unwrap();
        let result = store.get("nonexistent_hash_xyz").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    #[ignore = "requires REDIS_URL"]
    async fn redis_apq_exists_and_remove() {
        let url = redis_url().expect("REDIS_URL must be set");
        let store = RedisApqStorage::new(&url).await.unwrap();

        let hash = "exists_test_hash";
        store.set(hash.to_string(), "query".to_string()).await.unwrap();

        assert!(store.exists(hash).await.unwrap());
        store.remove(hash).await.unwrap();
        assert!(!store.exists(hash).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "requires REDIS_URL"]
    async fn redis_apq_fail_open_on_bad_url() {
        // ConnectionManager retries on failure, so wrap in a short timeout.
        // Connecting to a port with no listener should fail quickly.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            RedisApqStorage::new("redis://127.0.0.1:59997"),
        )
        .await;
        // Timeout or connection error are both acceptable outcomes.
        match result {
            Ok(Err(_)) | Err(_) => {},
            Ok(Ok(_)) => panic!("should not connect to port with no listener"),
        }
    }
}

mod storage_tests {

    use super::{super::*, *};

    #[test]
    fn test_apq_stats_creation() {
        let stats = ApqStats::new(100, "memory".to_string());
        assert_eq!(stats.total_queries, 100);
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.extra, json!({}));
    }

    #[test]
    fn test_apq_stats_with_extra() {
        let extra = json!({
            "hits": 500,
            "misses": 50,
            "hit_rate": 0.909
        });

        let stats = ApqStats::with_extra(100, "postgresql".to_string(), extra.clone());
        assert_eq!(stats.total_queries, 100);
        assert_eq!(stats.backend, "postgresql");
        assert_eq!(stats.extra, extra);
    }

    #[test]
    fn test_apq_error_display() {
        let err = ApqError::QueryTooLarge;
        assert_eq!(err.to_string(), "Query size exceeds maximum limit (100KB)");

        let err = ApqError::StorageError("connection failed".to_string());
        assert!(err.to_string().contains("connection failed"));
    }
}
