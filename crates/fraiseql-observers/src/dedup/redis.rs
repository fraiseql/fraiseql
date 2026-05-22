//! Redis-backed event deduplication store.
//!
//! Provides time-window based deduplication using Redis keys with TTL.
//! Events are considered duplicates if their dedup key exists in Redis.

use redis::aio::ConnectionManager;

use super::DeduplicationStore;
use crate::error::Result;

/// Redis-backed deduplication store.
///
/// Uses Redis keys with TTL for efficient deduplication. Each event gets a unique key,
/// and if the key exists, the event is a duplicate. TTL automatically expires keys.
#[derive(Clone)]
pub struct RedisDeduplicationStore {
    conn:           ConnectionManager,
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
    pub(crate) fn dedup_key(event_key: &str) -> String {
        format!("dedup:v1:{event_key}")
    }
}

#[async_trait::async_trait]
impl DeduplicationStore for RedisDeduplicationStore {
    /// Atomically claim the event via `SET key "1" NX EX window_secs`.
    ///
    /// Redis `SET NX` is atomic: only one concurrent caller will receive `OK`;
    /// all others receive `nil` and must skip the event.
    async fn claim_event(&self, event_key: &str) -> Result<bool> {
        let key = Self::dedup_key(event_key);
        // SET key "1" NX EX <ttl> → "OK" if we claimed it, nil if already claimed.
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(self.window_seconds.cast_signed())
            .query_async(&mut self.conn.clone())
            .await?;
        Ok(result.is_some())
    }

    async fn is_duplicate(&self, event_key: &str) -> Result<bool> {
        let key = Self::dedup_key(event_key);
        let exists: bool =
            redis::cmd("EXISTS").arg(&key).query_async(&mut self.conn.clone()).await?;

        Ok(exists)
    }

    async fn mark_processed(&self, event_key: &str) -> Result<()> {
        let key = Self::dedup_key(event_key);

        redis::cmd("SETEX")
            .arg(&key)
            .arg(self.window_seconds.cast_signed())
            .arg("1")
            .query_async::<()>(&mut self.conn.clone())
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

        redis::cmd("DEL").arg(&key).query_async::<()>(&mut self.conn.clone()).await?;

        Ok(())
    }
}
