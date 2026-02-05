//! Redis-backed action result caching.
//!
//! Provides high-performance caching of action results using Redis with
//! automatic TTL-based expiration.

use redis::aio::ConnectionManager;

use super::{CacheBackend, CachedActionResult};
use crate::error::Result;

/// Redis-backed cache backend.
///
/// Stores action results in Redis with configurable TTL.
/// Supports fast retrieval (<1ms) for cached results.
#[derive(Clone)]
pub struct RedisCacheBackend {
    conn:        ConnectionManager,
    ttl_seconds: u64,
}

impl RedisCacheBackend {
    /// Create a new Redis cache backend.
    ///
    /// # Arguments
    ///
    /// * `conn` - Redis connection manager
    /// * `ttl_seconds` - Time-to-live for cached results in seconds
    #[must_use]
    pub const fn new(conn: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { conn, ttl_seconds }
    }

    /// Generate cache key for consistent naming.
    fn cache_key(key: &str) -> String {
        format!("cache:v1:{key}")
    }
}

#[async_trait::async_trait]
impl CacheBackend for RedisCacheBackend {
    async fn get(&self, cache_key: &str) -> Result<Option<CachedActionResult>> {
        let key = Self::cache_key(cache_key);

        let value: Option<String> =
            redis::cmd("GET").arg(&key).query_async(&mut self.conn.clone()).await?;

        match value {
            Some(json) => {
                let result = serde_json::from_str(&json)
                    .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;
                Ok(Some(result))
            },
            None => Ok(None),
        }
    }

    async fn set(&self, cache_key: &str, result: &CachedActionResult) -> Result<()> {
        let key = Self::cache_key(cache_key);
        let json = serde_json::to_string(result)
            .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;

        redis::cmd("SETEX")
            .arg(&key)
            .arg(self.ttl_seconds as i64)
            .arg(&json)
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds
    }

    fn set_ttl_seconds(&mut self, seconds: u64) {
        self.ttl_seconds = seconds;
    }

    async fn invalidate(&self, cache_key: &str) -> Result<()> {
        let key = Self::cache_key(cache_key);

        redis::cmd("DEL").arg(&key).query_async::<_, ()>(&mut self.conn.clone()).await?;

        Ok(())
    }

    async fn clear_all(&self) -> Result<()> {
        // Use SCAN to find all cache keys and delete them
        let pattern = "cache:v1:*";

        let mut scan_cursor = 0u64;
        loop {
            let (cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(scan_cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.conn.clone())
                .await?;

            if !keys.is_empty() {
                redis::cmd("DEL")
                    .arg(&keys)
                    .query_async::<_, ()>(&mut self.conn.clone())
                    .await?;
            }

            scan_cursor = cursor;
            if scan_cursor == 0 {
                break;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let key = RedisCacheBackend::cache_key("email_action:order:123");
        assert_eq!(key, "cache:v1:email_action:order:123");
    }

    #[test]
    fn test_redis_cache_backend_clone() {
        // Ensure RedisCacheBackend is Clone-able
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisCacheBackend>();
        // Note: This test verifies the struct is Clone
        // Actual Redis tests require a Redis server
    }
}
