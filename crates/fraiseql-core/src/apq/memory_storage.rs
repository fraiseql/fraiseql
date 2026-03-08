//! In-memory APQ storage backend with LRU eviction and TTL support.

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::json;

use super::storage::{ApqError, ApqStats, ApqStorage};

/// Default TTL for stored queries (1 hour).
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Default maximum number of stored queries.
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// A stored query with metadata for TTL and LRU tracking.
struct StoredQuery {
    body: String,
    stored_at: Instant,
    ttl: Duration,
    last_accessed: Instant,
}

impl StoredQuery {
    fn is_expired(&self) -> bool {
        self.stored_at.elapsed() >= self.ttl
    }
}

/// In-memory APQ storage with LRU eviction and TTL expiry.
///
/// Stores persisted queries in a `HashMap` with configurable capacity
/// and time-to-live. When capacity is reached, expired entries are
/// purged first, then the least-recently-accessed entry is evicted.
pub struct InMemoryApqStorage {
    entries: tokio::sync::Mutex<HashMap<String, StoredQuery>>,
    max_entries: usize,
    ttl: Duration,
}

impl InMemoryApqStorage {
    /// Create a new store with the given maximum entry count and default TTL.
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: tokio::sync::Mutex::new(HashMap::new()),
            max_entries,
            ttl: DEFAULT_TTL,
        }
    }

    /// Create a new store with custom capacity and TTL.
    #[must_use]
    pub fn with_ttl(max_entries: usize, ttl: Duration) -> Self {
        Self {
            entries: tokio::sync::Mutex::new(HashMap::new()),
            max_entries,
            ttl,
        }
    }
}

impl Default for InMemoryApqStorage {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }
}

// Reason: ApqStorage is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl ApqStorage for InMemoryApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let mut map = self.entries.lock().await;

        // Check if the entry exists and is not expired.
        if let Some(entry) = map.get_mut(hash) {
            if entry.is_expired() {
                map.remove(hash);
                return Ok(None);
            }
            entry.last_accessed = Instant::now();
            return Ok(Some(entry.body.clone()));
        }

        Ok(None)
    }

    async fn set(&self, hash: String, query: String) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;

        let now = Instant::now();

        // Purge expired entries first.
        map.retain(|_, v| !v.is_expired());

        // If still at capacity, evict the least-recently-accessed entry.
        if map.len() >= self.max_entries && !map.contains_key(&hash) {
            if let Some(lru_key) = map
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone())
            {
                map.remove(&lru_key);
            }
        }

        map.insert(
            hash,
            StoredQuery {
                body: query,
                stored_at: now,
                ttl: self.ttl,
                last_accessed: now,
            },
        );

        Ok(())
    }

    async fn exists(&self, hash: &str) -> Result<bool, ApqError> {
        let mut map = self.entries.lock().await;

        if let Some(entry) = map.get(hash) {
            if entry.is_expired() {
                map.remove(hash);
                return Ok(false);
            }
            return Ok(true);
        }

        Ok(false)
    }

    async fn remove(&self, hash: &str) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;
        map.remove(hash);
        Ok(())
    }

    async fn stats(&self) -> Result<ApqStats, ApqError> {
        let map = self.entries.lock().await;

        let total = map.len();
        let expired = map.values().filter(|v| v.is_expired()).count();

        Ok(ApqStats::with_extra(
            total,
            "memory".to_string(),
            json!({
                "max_entries": self.max_entries,
                "ttl_secs": self.ttl.as_secs(),
                "expired_pending_cleanup": expired,
            }),
        ))
    }

    async fn clear(&self) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;
        map.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[tokio::test]
    async fn set_and_get() {
        let store = InMemoryApqStorage::default();
        store
            .set("abc123".to_string(), "{ users { id } }".to_string())
            .await
            .unwrap();

        let result = store.get("abc123").await.unwrap();
        assert_eq!(result, Some("{ users { id } }".to_string()));
    }

    #[tokio::test]
    async fn missing_hash_returns_none() {
        let store = InMemoryApqStorage::default();
        let result = store.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn ttl_expiry() {
        let store = InMemoryApqStorage::with_ttl(100, Duration::from_millis(50));
        store
            .set("h1".to_string(), "query1".to_string())
            .await
            .unwrap();

        // Should be present immediately.
        assert!(store.get("h1").await.unwrap().is_some());

        // Wait for TTL to expire.
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should be gone.
        assert!(store.get("h1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn max_entries_evicts_lru() {
        let store = InMemoryApqStorage::new(2);

        store
            .set("h1".to_string(), "q1".to_string())
            .await
            .unwrap();
        // Touch h1 first, then add h2 — h1 is older in access time.
        tokio::time::sleep(Duration::from_millis(5)).await;
        store
            .set("h2".to_string(), "q2".to_string())
            .await
            .unwrap();

        // Access h1 to make it more recently used.
        store.get("h1").await.unwrap();

        // Adding h3 should evict h2 (least recently accessed).
        store
            .set("h3".to_string(), "q3".to_string())
            .await
            .unwrap();

        assert!(store.get("h1").await.unwrap().is_some());
        assert!(store.get("h2").await.unwrap().is_none());
        assert!(store.get("h3").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn stats() {
        let store = InMemoryApqStorage::new(10);
        store
            .set("h1".to_string(), "q1".to_string())
            .await
            .unwrap();
        store
            .set("h2".to_string(), "q2".to_string())
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.total_queries, 2);
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.extra["max_entries"], 10);
    }

    #[tokio::test]
    async fn exists_and_remove() {
        let store = InMemoryApqStorage::default();
        store
            .set("h1".to_string(), "q1".to_string())
            .await
            .unwrap();

        assert!(store.exists("h1").await.unwrap());
        assert!(!store.exists("h2").await.unwrap());

        store.remove("h1").await.unwrap();
        assert!(!store.exists("h1").await.unwrap());
    }

    #[tokio::test]
    async fn clear() {
        let store = InMemoryApqStorage::default();
        store
            .set("h1".to_string(), "q1".to_string())
            .await
            .unwrap();
        store
            .set("h2".to_string(), "q2".to_string())
            .await
            .unwrap();

        assert_eq!(store.stats().await.unwrap().total_queries, 2);

        store.clear().await.unwrap();
        assert_eq!(store.stats().await.unwrap().total_queries, 0);
    }
}
