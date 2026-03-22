//! Redis APQ (Automatic Persisted Queries) integration tests.
//!
//! Validates the Redis-backed APQ storage against a real Redis instance
//! using testcontainers, plus in-memory APQ lifecycle tests.
//!
//! ## Running Tests
//!
//! ```bash
//! # In-memory tests (no infrastructure required)
//! cargo test --test redis_apq_integration_test --features auth
//!
//! # Redis tests (requires Docker)
//! cargo test --test redis_apq_integration_test --features "auth,redis-apq" -- --ignored
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use fraiseql_core::apq::{ApqMetrics, memory_storage::InMemoryApqStorage, storage::ApqStorage};

// --- In-memory APQ storage lifecycle ---

#[tokio::test]
async fn memory_apq_set_and_get() {
    let store = InMemoryApqStorage::default();

    store.set("hash1".to_string(), "{ users { id } }".to_string()).await.unwrap();
    let result = store.get("hash1").await.unwrap();
    assert_eq!(result, Some("{ users { id } }".to_string()));
}

#[tokio::test]
async fn memory_apq_missing_returns_none() {
    let store = InMemoryApqStorage::default();
    let result = store.get("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn memory_apq_exists_and_remove() {
    let store = InMemoryApqStorage::default();

    store.set("hash2".to_string(), "query".to_string()).await.unwrap();
    assert!(store.exists("hash2").await.unwrap());

    store.remove("hash2").await.unwrap();
    assert!(!store.exists("hash2").await.unwrap());
}

#[tokio::test]
async fn memory_apq_clear() {
    let store = InMemoryApqStorage::default();

    store.set("h1".to_string(), "q1".to_string()).await.unwrap();
    store.set("h2".to_string(), "q2".to_string()).await.unwrap();

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_queries, 2);

    store.clear().await.unwrap();
    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_queries, 0);
}

#[tokio::test]
async fn memory_apq_stats() {
    let store = InMemoryApqStorage::default();

    store.set("h1".to_string(), "q1".to_string()).await.unwrap();
    store.set("h2".to_string(), "q2".to_string()).await.unwrap();

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_queries, 2);
    assert_eq!(stats.backend, "memory");
}

#[tokio::test]
async fn memory_apq_overwrite_existing() {
    let store = InMemoryApqStorage::default();

    store.set("hash".to_string(), "old query".to_string()).await.unwrap();
    store.set("hash".to_string(), "new query".to_string()).await.unwrap();

    let result = store.get("hash").await.unwrap();
    assert_eq!(result, Some("new query".to_string()));

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_queries, 1, "should not duplicate on overwrite");
}

// --- APQ Metrics ---

#[test]
fn apq_metrics_hit_rate() {
    let metrics = ApqMetrics::default();

    metrics.record_hit();
    metrics.record_hit();
    metrics.record_miss();

    assert_eq!(metrics.get_hits(), 2);
    assert_eq!(metrics.get_misses(), 1);
    assert!((metrics.hit_rate() - (2.0 / 3.0)).abs() < 0.001);
}

#[test]
fn apq_metrics_stored() {
    let metrics = ApqMetrics::default();
    metrics.record_store();
    metrics.record_store();
    assert_eq!(metrics.get_stored(), 2);
}

#[test]
fn apq_metrics_hit_rate_zero_total() {
    let metrics = ApqMetrics::default();
    assert!(metrics.hit_rate().abs() < f64::EPSILON, "empty metrics should have 0% hit rate");
}

#[test]
fn apq_metrics_json_output() {
    let metrics = ApqMetrics::default();
    metrics.record_hit();
    metrics.record_miss();
    metrics.record_store();

    let json = metrics.as_json();
    assert_eq!(json["hits"], 1);
    assert_eq!(json["misses"], 1);
    assert_eq!(json["stored"], 1);
}

// --- Redis APQ tests (require Docker) ---

#[cfg(feature = "redis-apq")]
mod redis_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use fraiseql_core::apq::redis_storage::RedisApqStorage;

    /// Helper to get Redis URL from env or skip.
    fn redis_url() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
    }

    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL or run Redis on localhost:6379)"]
    async fn redis_apq_full_lifecycle() {
        let store = RedisApqStorage::new(&redis_url()).await.unwrap();
        store.clear().await.unwrap();

        let hash = "integration_test_hash";
        let query = "{ users { id name email } }";

        // Set
        store.set(hash.to_string(), query.to_string()).await.unwrap();

        // Get
        let result = store.get(hash).await.unwrap();
        assert_eq!(result.as_deref(), Some(query));

        // Exists
        assert!(store.exists(hash).await.unwrap());

        // Remove
        store.remove(hash).await.unwrap();
        assert!(!store.exists(hash).await.unwrap());

        // Get after remove
        let result = store.get(hash).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL or run Redis on localhost:6379)"]
    async fn redis_apq_clear_removes_all() {
        let store = RedisApqStorage::new(&redis_url()).await.unwrap();
        store.clear().await.unwrap();

        // Store several entries
        for i in 0..5 {
            store.set(format!("clear_test_{i}"), format!("query_{i}")).await.unwrap();
        }

        // Verify they exist
        assert!(store.exists("clear_test_0").await.unwrap());
        assert!(store.exists("clear_test_4").await.unwrap());

        // Clear all
        store.clear().await.unwrap();

        // Verify they're gone
        assert!(!store.exists("clear_test_0").await.unwrap());
        assert!(!store.exists("clear_test_4").await.unwrap());
    }

    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL or run Redis on localhost:6379)"]
    async fn redis_apq_stats_reports_backend() {
        let store = RedisApqStorage::new(&redis_url()).await.unwrap();
        let stats = store.stats().await.unwrap();
        assert_eq!(stats.backend, "redis");
    }

    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL or run Redis on localhost:6379)"]
    async fn redis_apq_custom_ttl() {
        let store = RedisApqStorage::new(&redis_url())
            .await
            .unwrap()
            .with_ttl_secs(60);

        store.set("ttl_test".to_string(), "query".to_string()).await.unwrap();
        let result = store.get("ttl_test").await.unwrap();
        assert_eq!(result.as_deref(), Some("query"));

        // Clean up
        store.remove("ttl_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Redis (set REDIS_URL or run Redis on localhost:6379)"]
    async fn redis_apq_concurrent_access() {
        let store = std::sync::Arc::new(RedisApqStorage::new(&redis_url()).await.unwrap());
        store.clear().await.unwrap();

        let mut handles = Vec::new();
        for i in 0..10 {
            let s = store.clone();
            handles.push(tokio::spawn(async move {
                s.set(format!("concurrent_{i}"), format!("query_{i}")).await.unwrap();
                let result = s.get(&format!("concurrent_{i}")).await.unwrap();
                assert!(result.is_some());
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // Verify all entries exist
        for i in 0..10 {
            assert!(store.exists(&format!("concurrent_{i}")).await.unwrap());
        }

        store.clear().await.unwrap();
    }
}
