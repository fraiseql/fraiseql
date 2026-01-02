//! In-memory LRU cache backend for APQ
//!
//! Provides fast, single-instance APQ storage using an LRU cache.
//! Best for single-instance deployments or development.

use async_trait::async_trait;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::apq::storage::{ApqError, ApqStats, ApqStorage};

/// In-memory LRU cache backend for APQ
///
/// Provides fast, thread-safe query caching using an LRU eviction policy.
/// Suitable for single-instance deployments or when `PostgreSQL` backend is unavailable.
#[derive(Debug)]
pub struct MemoryApqStorage {
    /// LRU cache of queries
    cache: Arc<RwLock<LruCache<String, String>>>,

    /// Cache hit counter
    hits: Arc<AtomicU64>,

    /// Cache miss counter
    misses: Arc<AtomicU64>,
}

impl MemoryApqStorage {
    /// Create new memory storage with capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of queries to cache (default: 1000)
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1000).expect("1000 > 0"));

        Self {
            cache: Arc::new(RwLock::new(LruCache::new(cap))),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get cache hit rate
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);

        if hits + misses == 0 {
            0.0
        } else {
            hits as f64 / (hits + misses) as f64
        }
    }

    /// Get current cache size
    pub async fn size(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Get cache capacity
    pub async fn capacity(&self) -> usize {
        self.cache.read().await.cap().get()
    }
}

#[async_trait]
impl ApqStorage for MemoryApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let mut cache = self.cache.write().await;

        cache.get(hash).map_or_else(
            || {
                self.misses.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            },
            |query| {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Ok(Some(query.clone()))
            },
        )
    }

    async fn set(&self, hash: String, query: String) -> Result<(), ApqError> {
        let mut cache = self.cache.write().await;
        cache.put(hash, query);
        Ok(())
    }

    async fn exists(&self, hash: &str) -> Result<bool, ApqError> {
        let cache = self.cache.read().await;
        Ok(cache.contains(hash))
    }

    async fn remove(&self, hash: &str) -> Result<(), ApqError> {
        let mut cache = self.cache.write().await;
        cache.pop(hash);
        Ok(())
    }

    async fn stats(&self) -> Result<ApqStats, ApqError> {
        let cache = self.cache.read().await;
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let hit_rate = if hits + misses > 0 {
            hits as f64 / (hits + misses) as f64
        } else {
            0.0
        };

        Ok(ApqStats::with_extra(
            cache.len(),
            "memory".to_string(),
            serde_json::json!({
                "hits": hits,
                "misses": misses,
                "hit_rate": hit_rate,
                "capacity": cache.cap().get(),
            }),
        ))
    }

    async fn clear(&self) -> Result<(), ApqError> {
        let mut cache = self.cache.write().await;
        cache.clear();
        Ok(())
    }
}

impl Default for MemoryApqStorage {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_creation() {
        let storage = MemoryApqStorage::new(100);
        assert_eq!(storage.size().await, 0);
        assert_eq!(storage.capacity().await, 100);
    }

    #[tokio::test]
    async fn test_memory_storage_default() {
        let storage = MemoryApqStorage::default();
        assert_eq!(storage.capacity().await, 1000);
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query.clone()).await.unwrap();

        let retrieved = storage.get(&hash).await.unwrap();
        assert_eq!(retrieved, Some(query));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let storage = MemoryApqStorage::new(100);
        let result = storage.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_exists() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query).await.unwrap();

        assert!(storage.exists(&hash).await.unwrap());
        assert!(!storage.exists("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query).await.unwrap();
        assert!(storage.exists(&hash).await.unwrap());

        storage.remove(&hash).await.unwrap();
        assert!(!storage.exists(&hash).await.unwrap());
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    async fn test_hit_rate_tracking() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query).await.unwrap();

        // Get hit
        let _ = storage.get(&hash).await;
        assert_eq!(storage.hit_rate(), 1.0);

        // Get miss
        let _ = storage.get("nonexistent").await;
        assert!((storage.hit_rate() - 0.5).abs() < 0.0001); // 1 hit / 2 total
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let storage = MemoryApqStorage::new(2); // Small capacity

        storage
            .set("hash1".to_string(), "query1".to_string())
            .await
            .unwrap();
        storage
            .set("hash2".to_string(), "query2".to_string())
            .await
            .unwrap();
        assert_eq!(storage.size().await, 2);

        // Third entry should evict oldest (hash1)
        storage
            .set("hash3".to_string(), "query3".to_string())
            .await
            .unwrap();
        assert_eq!(storage.size().await, 2);

        assert_eq!(storage.get("hash1").await.unwrap(), None);
        assert_eq!(
            storage.get("hash2").await.unwrap(),
            Some("query2".to_string())
        );
        assert_eq!(
            storage.get("hash3").await.unwrap(),
            Some("query3".to_string())
        );
    }

    #[tokio::test]
    async fn test_clear() {
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
    async fn test_stats() {
        let storage = MemoryApqStorage::new(100);
        let hash = "abc123".to_string();
        let query = "{ users { id } }".to_string();

        storage.set(hash.clone(), query).await.unwrap();
        let _ = storage.get(&hash).await; // Hit
        let _ = storage.get("nonexistent").await; // Miss

        let stats = storage.stats().await.unwrap();
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.backend, "memory");
        assert_eq!(stats.extra["hits"], 1);
        assert_eq!(stats.extra["misses"], 1);
    }
}
