//! Redis-backed rate limiter with atomic sliding window.

use async_trait::async_trait;
use redis::AsyncCommands;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{RateLimit, RateLimitResult, RateLimitState, RateLimiter};
use fraiseql_error::RuntimeError;

pub struct RedisRateLimiter {
    client: redis::Client,
    key_prefix: String,
}

impl RedisRateLimiter {
    /// Create a new Redis rate limiter
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis connection cannot be established
    pub async fn new(redis_url: &str) -> Result<Self, RuntimeError> {
        let client = redis::Client::open(redis_url).map_err(|e| {
            RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis: {e}"),
            })
        })?;

        // Test connection
        let mut conn = client.get_async_connection().await.map_err(|e| {
            RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis: {e}"),
            })
        })?;

        let _: String = redis::cmd("PING").query_async(&mut conn).await.map_err(|e| {
            RuntimeError::Integration(fraiseql_error::IntegrationError::ConnectionFailed {
                service: format!("redis ping: {e}"),
            })
        })?;

        Ok(Self {
            client,
            key_prefix: "fraiseql:ratelimit".to_string(),
        })
    }

    fn make_key(&self, key: &str) -> String {
        format!("{}:{}", self.key_prefix, key)
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, key: &str, limit: &RateLimit) -> RateLimitResult {
        let redis_key = self.make_key(key);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let window_start_ms = now_ms - limit.window.as_millis() as i64;

        let mut conn = match self.client.get_async_connection().await {
            Ok(c) => c,
            Err(_) => return RateLimitResult::Overloaded, // Fail open or closed based on policy
        };

        // Lua script for atomic sliding window check
        let script = redis::Script::new(
            r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local window_start = tonumber(ARGV[2])
            local limit = tonumber(ARGV[3])
            local window_ms = tonumber(ARGV[4])

            -- Remove old entries
            redis.call('ZREMRANGEBYSCORE', key, '-inf', window_start)

            -- Count current entries
            local count = redis.call('ZCARD', key)

            if count < limit then
                -- Add new entry
                redis.call('ZADD', key, now, now)
                redis.call('PEXPIRE', key, window_ms)
                return {1, limit - count - 1, 0}
            else
                -- Get oldest entry for reset time
                local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
                local reset_at = 0
                if #oldest > 0 then
                    reset_at = oldest[2] + window_ms
                end
                return {0, 0, reset_at}
            end
        "#,
        );

        let result: Vec<i64> = match script
            .key(&redis_key)
            .arg(now_ms)
            .arg(window_start_ms)
            .arg(limit.requests as i64)
            .arg(limit.window.as_millis() as i64)
            .invoke_async(&mut conn)
            .await
        {
            Ok(r) => r,
            Err(_) => return RateLimitResult::Overloaded,
        };

        if result[0] == 1 {
            RateLimitResult::Allowed {
                remaining: result[1] as u32,
                limit: limit.requests,
                reset_at: SystemTime::now() + limit.window,
            }
        } else {
            let reset_at_ms = result[2];
            let retry_after = if reset_at_ms > now_ms {
                Duration::from_millis((reset_at_ms - now_ms) as u64)
            } else {
                limit.window
            };

            RateLimitResult::Limited {
                retry_after,
                limit: limit.requests,
            }
        }
    }

    async fn record(&self, _key: &str, _limit: &RateLimit) {
        // Recording is done atomically in check() for Redis
    }

    async fn get_state(&self, key: &str) -> Option<RateLimitState> {
        let redis_key = self.make_key(key);
        let mut conn: redis::aio::Connection = self.client.get_async_connection().await.ok()?;
        let count: i64 = redis::cmd("ZCARD")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await
            .ok()?;

        Some(RateLimitState {
            current_count: count as u32,
            window_start: SystemTime::now(),
            queue_depth: 0, // Redis doesn't track queue depth
        })
    }
}
