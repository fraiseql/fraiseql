//! In-memory cache backend implementation
//!
//! Provides a simple, fast in-memory cache using DashMap for concurrent access.
//! Perfect for development and single-instance deployments.

use super::traits::CacheBackend;
use super::CacheError;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// In-memory cache backend using DashMap for concurrent access
pub struct MemoryCache {
    /// Concurrent HashMap storing cache entries with their expiration times
    data: Arc<DashMap<String, CacheEntry>>,
}

/// Internal cache entry with expiration tracking
struct CacheEntry {
    value: serde_json::Value,
    expires_at: SystemTime,
}

impl MemoryCache {
    /// Create a new in-memory cache
    pub fn new() -> Self {
        MemoryCache {
            data: Arc::new(DashMap::new()),
        }
    }

    /// Get the number of entries currently in cache
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all entries from cache (synchronous helper)
    fn sync_clear(&self) {
        self.data.clear();
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, CacheError> {
        // Check if key exists
        if let Some(entry) = self.data.get(key) {
            // Check if entry has expired
            if SystemTime::now() < entry.expires_at {
                return Ok(Some(entry.value.clone()));
            }
            // Entry expired, remove it
            drop(entry);
            self.data.remove(key);
        }

        Ok(None)
    }

    async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<(), CacheError> {
        let expires_at = SystemTime::now() + std::time::Duration::from_secs(ttl_seconds);

        self.data
            .insert(key.to_string(), CacheEntry { value, expires_at });

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.data.remove(key);
        Ok(())
    }

    async fn delete_many(&self, keys: &[String]) -> Result<(), CacheError> {
        for key in keys {
            self.data.remove(key);
        }
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.sync_clear();
        Ok(())
    }

    async fn health_check(&self) -> Result<(), CacheError> {
        // In-memory cache is always healthy
        Ok(())
    }

    fn backend_name(&self) -> &str {
        "memory"
    }

    async fn size(&self) -> Result<Option<usize>, CacheError> {
        Ok(Some(self.data.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_memory_cache_set_get() {
        let cache = MemoryCache::new();
        let value = json!({"id": 1, "name": "test"});

        cache.set("key1", value.clone(), 3600).await.unwrap();
        let retrieved = cache.get("key1").await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }

    #[tokio::test]
    async fn test_memory_cache_get_missing_key() {
        let cache = MemoryCache::new();
        let retrieved = cache.get("missing_key").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_delete() {
        let cache = MemoryCache::new();
        cache.set("key1", json!({"id": 1}), 3600).await.unwrap();

        cache.delete("key1").await.unwrap();
        let retrieved = cache.get("key1").await.unwrap();

        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_delete_many() {
        let cache = MemoryCache::new();
        cache.set("key1", json!({"id": 1}), 3600).await.unwrap();
        cache.set("key2", json!({"id": 2}), 3600).await.unwrap();
        cache.set("key3", json!({"id": 3}), 3600).await.unwrap();

        cache
            .delete_many(&["key1".to_string(), "key2".to_string()])
            .await
            .unwrap();

        assert!(cache.get("key1").await.unwrap().is_none());
        assert!(cache.get("key2").await.unwrap().is_none());
        assert!(cache.get("key3").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_memory_cache_clear() {
        let cache = MemoryCache::new();
        cache.set("key1", json!({"id": 1}), 3600).await.unwrap();
        cache.set("key2", json!({"id": 2}), 3600).await.unwrap();

        cache.clear().await.unwrap();

        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_memory_cache_expiration() {
        let cache = MemoryCache::new();

        // Set with 1 second TTL
        cache.set("short_ttl", json!({"id": 1}), 1).await.unwrap();

        // Should be available immediately
        assert!(cache.get("short_ttl").await.unwrap().is_some());

        // Wait for expiration
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        // Should be expired now
        assert!(cache.get("short_ttl").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_backend_name() {
        let cache = MemoryCache::new();
        assert_eq!(cache.backend_name(), "memory");
    }

    #[tokio::test]
    async fn test_memory_cache_health_check() {
        let cache = MemoryCache::new();
        assert!(cache.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_memory_cache_size() {
        let cache = MemoryCache::new();
        cache.set("key1", json!({"id": 1}), 3600).await.unwrap();
        cache.set("key2", json!({"id": 2}), 3600).await.unwrap();

        let size = cache.size().await.unwrap();
        assert_eq!(size, Some(2));
    }

    #[test]
    fn test_memory_cache_len() {
        let cache = MemoryCache::new();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_memory_cache_is_empty() {
        let cache = MemoryCache::new();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_memory_cache_default() {
        let _cache = MemoryCache::default();
        // Should create successfully
    }
}
