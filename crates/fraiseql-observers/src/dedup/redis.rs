//! Redis-backed event deduplication store.
//!
//! Provides time-window based deduplication using Redis keys with TTL.
//! Events are considered duplicates if their dedup key exists in Redis.

use super::DeduplicationStore;
use crate::error::Result;
use redis::aio::ConnectionManager;

/// Redis-backed deduplication store.
///
/// Uses Redis keys with TTL for efficient deduplication. Each event gets a unique key,
/// and if the key exists, the event is a duplicate. TTL automatically expires keys.
#[derive(Clone)]
pub struct RedisDeduplicationStore {
    conn: ConnectionManager,
    window_seconds: u64,
}

impl RedisDeduplicationStore {
    /// Create a new Redis deduplication store.
    ///
    /// # Arguments
    ///
    /// * `conn` - Redis connection manager
    /// * `window_seconds` - Deduplication time window in seconds (default 300)
    #[must_use] 
    pub const fn new(conn: ConnectionManager, window_seconds: u64) -> Self {
        Self {
            conn,
            window_seconds,
        }
    }

    /// Generate deduplication key for an event.
    ///
    /// # Arguments
    ///
    /// * `event_key` - The event identifier
    ///
    /// # Returns
    ///
    /// Redis key with dedup prefix
    fn dedup_key(event_key: &str) -> String {
        format!("dedup:v1:{event_key}")
    }
}

#[async_trait::async_trait]
impl DeduplicationStore for RedisDeduplicationStore {
    async fn is_duplicate(&self, event_key: &str) -> Result<bool> {
        let key = Self::dedup_key(event_key);
        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(exists)
    }

    async fn mark_processed(&self, event_key: &str) -> Result<()> {
        let key = Self::dedup_key(event_key);

        redis::cmd("SETEX")
            .arg(&key)
            .arg(self.window_seconds as i64)
            .arg("1")
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    fn window_seconds(&self) -> u64 {
        self.window_seconds
    }

    fn set_window_seconds(&mut self, seconds: u64) {
        self.window_seconds = seconds;
    }

    async fn remove(&self, event_key: &str) -> Result<()> {
        let key = Self::dedup_key(event_key);

        redis::cmd("DEL")
            .arg(&key)
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_key_generation() {
        let key = RedisDeduplicationStore::dedup_key("order:123:created");
        assert_eq!(key, "dedup:v1:order:123:created");
    }

    #[test]
    fn test_redis_dedup_store_clone() {
        // Ensure RedisDeduplicationStore is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<RedisDeduplicationStore>();
        // Note: This test only verifies the struct is Clone-able
        // Actual Redis tests require a Redis server
    }
}
