//! Redis-backed action result caching.
//!
//! Provides high-performance caching of action results using Redis with
//! automatic TTL-based expiration.

use redis::aio::ConnectionManager;

use super::{CacheBackend, CacheBackendDyn, CachedActionResult, glob};
use crate::{
    config::RedisConfig,
    error::{ObserverError, Result},
    event::EntityEvent,
};

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
    pub(crate) fn cache_key(key: &str) -> String {
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
            .arg(self.ttl_seconds.cast_signed()) // Reason: TTL seconds won't exceed i64::MAX
            .arg(&json)
            .query_async::<()>(&mut self.conn.clone())
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

        redis::cmd("DEL").arg(&key).query_async::<()>(&mut self.conn.clone()).await?;

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
                redis::cmd("DEL").arg(&keys).query_async::<()>(&mut self.conn.clone()).await?;
            }

            scan_cursor = cursor;
            if scan_cursor == 0 {
                break;
            }
        }

        Ok(())
    }
}

/// Real Redis transport for the `cache` observer action (#428).
///
/// Unlike [`RedisCacheBackend`] — which namespaces the observer's *own*
/// action-result cache under a `cache:v1:` prefix — this invalidator operates on
/// the **raw application keyspace**: an entity change fires an `invalidate` action
/// that deletes the application's cached read keys so downstream readers see fresh
/// data. It shares Redis with the result cache but is a separate concern, so it
/// holds its own connection and never prepends a prefix.
///
/// `key_pattern` is a `{{ field }}` template (see
/// [`invalidate`](RedisCacheInvalidator::invalidate)). After the event values are
/// glob-escaped and substituted, a pattern that still contains a glob operator is
/// matched with `SCAN MATCH`; otherwise a single literal key is targeted directly.
/// Deletion uses `UNLINK` (asynchronous reclaim) on both paths to avoid the
/// O(N) latency spike a synchronous `DEL` of a wide match would cause on a hot
/// keyspace.
#[derive(Clone)]
pub struct RedisCacheInvalidator {
    conn: ConnectionManager,
}

impl RedisCacheInvalidator {
    /// Connect to Redis using the observer runtime's [`RedisConfig`].
    ///
    /// This is the TOML → runtime bridge: the same `[observers.runtime.redis]`
    /// config that feeds dedup/result-cache also feeds cache invalidation. A
    /// connection is established eagerly so an unreachable Redis fails loud at
    /// startup rather than silently dropping invalidations later.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if the URL is malformed or the
    /// initial connection cannot be established.
    pub async fn connect(config: &RedisConfig) -> Result<Self> {
        let client =
            redis::Client::open(config.url.as_str()).map_err(|e| ObserverError::InvalidConfig {
                message: format!("cache invalidation: invalid Redis URL {:?}: {e}", config.url),
            })?;
        let conn =
            ConnectionManager::new(client).await.map_err(|e| ObserverError::InvalidConfig {
                message: format!(
                    "cache invalidation: cannot connect to Redis at {:?}: {e}",
                    config.url
                ),
            })?;
        Ok(Self { conn })
    }

    /// Build an invalidator from an existing connection manager.
    ///
    /// Useful for callers that already hold a Redis connection (e.g. sharing one
    /// with the result-cache backend) and for tests.
    #[must_use]
    pub const fn from_connection(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Invalidate the cache key(s) described by `key_pattern` for this event.
    ///
    /// `key_pattern` is a `{{ field }}` template rendered against `event.data`.
    /// Event values are glob-escaped *before* substitution (the escape-then-
    /// substitute helpers in the `glob` module), so only globs the config author
    /// wrote in the template itself are active. The rendered pattern is then
    /// dispatched as:
    ///
    /// - **single literal key** (no surviving glob) → one `UNLINK`;
    /// - **glob** (e.g. `app:user:{{ id }}:*`) → `SCAN MATCH` + batched `UNLINK`.
    ///
    /// Returns the number of keys removed.
    ///
    /// `SCAN`+`UNLINK` is intentionally **non-atomic**: a key written
    /// concurrently with the scan can be missed. For cache invalidation this is
    /// acceptable — a key written *after* the entity change carries post-change
    /// data worth keeping, and a genuinely-concurrent miss is covered by the
    /// retryable error class (the action retries and the next pass clears it).
    /// The cost of that retryable classification is a brief stale read until the
    /// retry lands.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] (the retryable class) if any
    /// Redis command fails — a dropped invalidation is a correctness problem
    /// (stale read), not a lost message, so it must surface loudly and retry,
    /// never report a fabricated success.
    pub async fn invalidate(&self, key_pattern: &str, event: &EntityEvent) -> Result<u64> {
        let escaped = glob::render_key_pattern(key_pattern, &event.data, true);
        if glob::has_unescaped_glob(&escaped) {
            self.scan_unlink(&escaped).await
        } else {
            let literal = glob::render_key_pattern(key_pattern, &event.data, false);
            self.unlink_keys(&[literal]).await
        }
    }

    /// `UNLINK` the given keys (asynchronous reclaim). Returns the count removed.
    async fn unlink_keys(&self, keys: &[String]) -> Result<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        let removed: u64 =
            redis::cmd("UNLINK").arg(keys).query_async(&mut self.conn.clone()).await?;
        Ok(removed)
    }

    /// `SCAN MATCH pattern` in cursor batches, `UNLINK`-ing each batch. Never uses
    /// the blocking `KEYS` command. Returns the total count removed.
    async fn scan_unlink(&self, pattern: &str) -> Result<u64> {
        let mut cursor = 0u64;
        let mut total = 0u64;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.conn.clone())
                .await?;

            if !keys.is_empty() {
                total = total.saturating_add(self.unlink_keys(&keys).await?);
            }

            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        Ok(total)
    }
}

#[async_trait::async_trait]
impl CacheBackendDyn for RedisCacheBackend {
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
            .arg(self.ttl_seconds.cast_signed()) // Reason: TTL seconds won't exceed i64::MAX
            .arg(&json)
            .query_async::<()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds
    }
}
