//! Redis-backed idempotency store for multi-replica deployments.
//!
//! Uses MessagePack for compact binary serialization and Redis `SET EX` for
//! automatic TTL-based expiry.

use std::time::Duration;

use redis::AsyncCommands;
use tracing::warn;

use super::{IdempotencyCheck, IdempotencyStore, StoredResponse};

/// Serializable Redis entry combining body hash + stored response.
#[derive(serde::Serialize, serde::Deserialize)]
struct RedisEntry {
    body_hash: u64,
    response: StoredResponse,
}

/// Redis-backed idempotency store.
///
/// Each key is stored as `fraiseql:idempotency:{key}` with TTL set via `SET EX`.
/// Value is MessagePack-encoded `RedisEntry`.
pub struct RedisIdempotencyStore {
    pool: redis::aio::ConnectionManager,
    ttl: Duration,
    prefix: String,
}

impl RedisIdempotencyStore {
    /// Create a new Redis-backed idempotency store.
    #[must_use]
    pub fn new(pool: redis::aio::ConnectionManager, ttl: Duration) -> Self {
        Self {
            pool,
            ttl,
            prefix: "fraiseql:idempotency:".to_string(),
        }
    }

    fn redis_key(&self, key: &str) -> String {
        format!("{}{key}", self.prefix)
    }
}

impl IdempotencyStore for RedisIdempotencyStore {
    fn check(
        &self,
        key: &str,
        body_hash: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = IdempotencyCheck> + Send + '_>> {
        let redis_key = self.redis_key(key);
        let mut conn = self.pool.clone();

        Box::pin(async move {
            let raw: Option<Vec<u8>> = match conn.get(&redis_key).await {
                Ok(val) => val,
                Err(e) => {
                    warn!("Redis idempotency check failed, treating as new: {e}");
                    return IdempotencyCheck::New;
                }
            };

            match raw {
                None => IdempotencyCheck::New,
                Some(data) => {
                    let entry: RedisEntry = match rmp_serde::from_slice(&data) {
                        Ok(e) => e,
                        Err(e) => {
                            warn!("Failed to deserialize idempotency entry: {e}");
                            return IdempotencyCheck::New;
                        }
                    };
                    if entry.body_hash == body_hash {
                        IdempotencyCheck::Replay(entry.response)
                    } else {
                        IdempotencyCheck::Conflict
                    }
                }
            }
        })
    }

    fn store(
        &self,
        key: String,
        body_hash: u64,
        response: StoredResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let redis_key = self.redis_key(&key);
        let entry = RedisEntry {
            body_hash,
            response,
        };

        Box::pin(async move {
            let data = match rmp_serde::to_vec(&entry) {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to serialize idempotency entry: {e}");
                    return;
                }
            };

            let mut conn = self.pool.clone();
            let ttl_secs = self.ttl.as_secs();
            if let Err(e) = conn.set_ex::<_, _, ()>(&redis_key, data, ttl_secs).await {
                warn!("Redis idempotency store failed: {e}");
            }
        })
    }
}
