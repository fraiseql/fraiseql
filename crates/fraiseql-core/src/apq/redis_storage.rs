//! Redis-backed APQ storage backend.
//!
//! Provides distributed, persistent query storage using Redis `SET`/`GET`
//! with automatic TTL expiry. Fail-open: Redis errors are logged and
//! treated as cache misses, since APQ is an optimisation, not a
//! correctness requirement.

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use redis::AsyncCommands;
use serde_json::json;
use tracing::warn;

use super::storage::{ApqError, ApqStats, ApqStorage};

/// Redis key prefix for APQ entries.
const KEY_PREFIX: &str = "fraiseql:apq:";

/// Default TTL for stored queries (1 hour).
const DEFAULT_TTL_SECS: u64 = 3600;

/// Counter of Redis errors encountered (for metrics / diagnostics).
static REDIS_APQ_ERRORS: AtomicU64 = AtomicU64::new(0);

/// Return the cumulative count of Redis errors since process start.
#[must_use]
pub fn redis_apq_error_count_total() -> u64 {
    REDIS_APQ_ERRORS.load(Ordering::Relaxed)
}

/// Redis-backed APQ storage.
///
/// Each persisted query is stored as a Redis string at key
/// `fraiseql:apq:{sha256_hash}` with the configured TTL.
///
/// **Fail-open**: any Redis error is logged, counted, and surfaced as
/// `Ok(None)` / `Ok(())` so that the request pipeline falls back to
/// requiring the full query body.
pub struct RedisApqStorage {
    pool:     redis::aio::ConnectionManager,
    ttl_secs: u64,
}

impl RedisApqStorage {
    /// Connect to Redis and prepare the APQ store.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or the initial connection fails.
    pub async fn new(redis_url: &str) -> Result<Self, ApqError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ApqError::StorageError(format!("invalid Redis URL: {e}")))?;
        let pool = client
            .get_connection_manager()
            .await
            .map_err(|e| ApqError::StorageError(format!("Redis connection failed: {e}")))?;
        Ok(Self {
            pool,
            ttl_secs: DEFAULT_TTL_SECS,
        })
    }

    /// Set the TTL for stored queries (in seconds).
    #[must_use]
    pub const fn with_ttl_secs(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    /// Build the full Redis key for a given query hash.
    fn key(hash: &str) -> String {
        format!("{KEY_PREFIX}{hash}")
    }

    /// Record a Redis error and return a fail-open result.
    fn fail_open<T: Default>(err: &redis::RedisError, operation: &str) -> Result<T, ApqError> {
        REDIS_APQ_ERRORS.fetch_add(1, Ordering::Relaxed);
        warn!(operation, error = %err, "Redis APQ: fail-open on error");
        Ok(T::default())
    }
}

// Reason: ApqStorage is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl ApqStorage for RedisApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let mut conn = self.pool.clone();
        match conn.get::<_, Option<String>>(Self::key(hash)).await {
            Ok(result) => Ok(result),
            Err(e) => Self::fail_open(&e, "GET"),
        }
    }

    async fn set(&self, hash: String, query: String) -> Result<(), ApqError> {
        let mut conn = self.pool.clone();
        match conn.set_ex::<_, _, ()>(Self::key(&hash), &query, self.ttl_secs).await {
            Ok(()) => Ok(()),
            Err(e) => Self::fail_open(&e, "SET"),
        }
    }

    async fn exists(&self, hash: &str) -> Result<bool, ApqError> {
        let mut conn = self.pool.clone();
        match conn.exists::<_, bool>(Self::key(hash)).await {
            Ok(result) => Ok(result),
            Err(e) => Self::fail_open(&e, "EXISTS"),
        }
    }

    async fn remove(&self, hash: &str) -> Result<(), ApqError> {
        let mut conn = self.pool.clone();
        match conn.del::<_, ()>(Self::key(hash)).await {
            Ok(()) => Ok(()),
            Err(e) => Self::fail_open(&e, "DEL"),
        }
    }

    async fn stats(&self) -> Result<ApqStats, ApqError> {
        // Redis doesn't efficiently expose "how many keys match a prefix", so
        // we report what we can: the backend name and the error counter.
        Ok(ApqStats::with_extra(
            0, // unknown without SCAN
            "redis".to_string(),
            json!({ "redis_errors_total": redis_apq_error_count_total() }),
        ))
    }

    async fn clear(&self) -> Result<(), ApqError> {
        // Use cursor-based SCAN to avoid blocking the Redis server with O(N) KEYS.
        let mut conn = self.pool.clone();
        let pattern = format!("{KEY_PREFIX}*");
        let mut cursor: u64 = 0;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = match redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100u64)
                .query_async(&mut conn)
                .await
            {
                Ok(result) => result,
                Err(e) => return Self::fail_open(&e, "SCAN"),
            };

            if !keys.is_empty() {
                if let Err(e) = conn.del::<_, ()>(&keys[..]).await {
                    return Self::fail_open(&e, "DEL (clear)");
                }
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(())
    }
}
