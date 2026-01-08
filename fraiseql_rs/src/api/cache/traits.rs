//! Cache backend traits and interfaces
//!
//! Defines the contract that all cache backends must implement.

use super::CacheError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Metadata about a cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Cache key
    pub key: String,

    /// Cached value
    pub value: serde_json::Value,

    /// Time-to-live in seconds
    pub ttl_seconds: u64,

    /// When this entry was created
    pub created_at: SystemTime,
}

/// Cache backend trait - all cache implementations must implement this
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Get a value from the cache
    ///
    /// # Arguments
    /// * `key` - The cache key
    ///
    /// # Returns
    /// * `Ok(Some(value))` - Value found and not expired
    /// * `Ok(None)` - Key not found or expired
    /// * `Err(CacheError)` - If cache operation fails
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, CacheError>;

    /// Set a value in the cache with a TTL
    ///
    /// # Arguments
    /// * `key` - The cache key
    /// * `value` - The value to cache
    /// * `ttl_seconds` - Time-to-live in seconds
    ///
    /// # Returns
    /// * `Ok(())` - Value stored
    /// * `Err(CacheError)` - If storage fails
    async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        ttl_seconds: u64,
    ) -> Result<(), CacheError>;

    /// Delete a value from the cache
    ///
    /// # Arguments
    /// * `key` - The cache key to delete
    ///
    /// # Returns
    /// * `Ok(())` - Key deleted (or didn't exist)
    /// * `Err(CacheError)` - If deletion fails
    async fn delete(&self, key: &str) -> Result<(), CacheError>;

    /// Delete multiple keys from the cache
    ///
    /// # Arguments
    /// * `keys` - Keys to delete
    ///
    /// # Returns
    /// * `Ok(())` - Keys deleted
    /// * `Err(CacheError)` - If deletion fails
    async fn delete_many(&self, keys: &[String]) -> Result<(), CacheError>;

    /// Clear the entire cache
    ///
    /// # Returns
    /// * `Ok(())` - Cache cleared
    /// * `Err(CacheError)` - If clear fails
    async fn clear(&self) -> Result<(), CacheError>;

    /// Check if the cache backend is healthy
    ///
    /// # Returns
    /// * `Ok(())` - Cache is healthy
    /// * `Err(CacheError)` - If health check fails
    async fn health_check(&self) -> Result<(), CacheError>;

    /// Get cache backend name (for logging/debugging)
    fn backend_name(&self) -> &str;

    /// Get the current cache size (if available)
    /// Returns None if backend doesn't track size
    async fn size(&self) -> Result<Option<usize>, CacheError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_serialization() {
        let now = SystemTime::now();
        let entry = CacheEntry {
            key: "test_key".to_string(),
            value: serde_json::json!({"id": 1, "name": "test"}),
            ttl_seconds: 3600,
            created_at: now,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: CacheEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.key, "test_key");
        assert_eq!(deserialized.ttl_seconds, 3600);
    }
}
