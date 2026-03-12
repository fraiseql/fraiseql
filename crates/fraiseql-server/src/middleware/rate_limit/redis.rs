//! Redis-backed distributed rate limiter backend.

#[cfg(feature = "redis-rate-limiting")]
use tracing::{debug, warn};

#[cfg(feature = "redis-rate-limiting")]
use super::config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig};
#[cfg(feature = "redis-rate-limiting")]
use super::key::{PathRateLimit, path_matches_rule};

/// Convert a Redis Lua `retry_after_ms` value to a `Retry-After` header value
/// in whole seconds.
///
/// Rounds up so clients never retry before the bucket has refilled.
/// Clamps to a minimum of 1 second.
#[cfg(feature = "redis-rate-limiting")]
fn retry_after_ms_to_secs(ms: u64) -> u32 {
    ms.div_ceil(1000).max(1) as u32
}

// ─── Redis backend ────────────────────────────────────────────────────────────

/// Cumulative count of Redis rate limiter errors (fail-open events).
///
/// Exposed via `/metrics` as `fraiseql_rate_limit_redis_errors_total`.
#[cfg(feature = "redis-rate-limiting")]
pub static REDIS_RATE_LIMIT_ERRORS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Return the total number of Redis rate-limit fail-open events observed so far.
#[cfg(feature = "redis-rate-limiting")]
pub fn redis_error_count_total() -> u64 {
    REDIS_RATE_LIMIT_ERRORS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Internal result returned by the Redis Lua token-bucket script.
#[cfg(feature = "redis-rate-limiting")]
struct RedisRateLimitResult {
    allowed:          bool,
    /// Remaining tokens after this request (in whole tokens, not milli-tokens).
    remaining_tokens: f64,
    /// Milliseconds until the next token becomes available; 0 when allowed.
    retry_after_ms:   u64,
}

/// Atomic token-bucket Lua script for Redis.
///
/// Tokens are stored as **milli-tokens** (integer × 1000) so that sub-1-rps
/// rates (e.g. 10 req/60 s = 0.1667 req/s = 167 milli-tokens/s) are handled
/// with integer arithmetic without floating-point in Lua.
///
/// Arguments:
/// - `KEYS[1]`  — bucket key (e.g. `fraiseql:rl:ip:1.2.3.4`)
/// - `ARGV[1]`  — capacity  (burst_size × 1000, milli-tokens)
/// - `ARGV[2]`  — refill rate (tokens_per_sec × 1000, milli-tokens per second)
/// - `ARGV[3]`  — now (Unix timestamp in **milliseconds**)
///
/// Returns `[allowed (0|1), remaining_milli_tokens, retry_after_ms]`.
#[cfg(feature = "redis-rate-limiting")]
const RATE_LIMIT_LUA: &str = r"
local key      = KEYS[1]
local capacity = tonumber(ARGV[1])
local rate     = tonumber(ARGV[2])
local now      = tonumber(ARGV[3])

local bucket      = redis.call('HMGET', key, 'tokens', 'last_refill')
local tokens      = tonumber(bucket[1]) or capacity
local last_refill = tonumber(bucket[2]) or now

local elapsed = math.max(0, now - last_refill) / 1000.0
local refill  = math.floor(elapsed * rate)
tokens = math.min(capacity, tokens + refill)

if tokens >= 1000 then
    tokens = tokens - 1000
    redis.call('HSET', key, 'tokens', tokens, 'last_refill', now)
    redis.call('PEXPIRE', key, math.ceil(capacity / rate * 1000))
    return {1, tokens, 0}
else
    local retry_ms = math.ceil((1000 - tokens) / rate * 1000)
    return {0, 0, retry_ms}
end
";

/// Rate limiter backed by Redis for distributed, multi-instance deployments.
///
/// Uses an atomic Lua token-bucket script (`EVALSHA`) to prevent race
/// conditions when multiple server replicas share a rate limit.
///
/// **Fail-open**: on Redis errors requests are allowed and a warning is
/// logged. The cumulative error count is tracked in [`REDIS_RATE_LIMIT_ERRORS`]
/// and exposed in the `/metrics` endpoint.
#[cfg(feature = "redis-rate-limiting")]
pub struct RedisRateLimiter {
    pool:       redis::aio::ConnectionManager,
    config:     RateLimitConfig,
    path_rules: Vec<PathRateLimit>,
    /// Cached SHA of the loaded Lua script.  Cleared on `NOSCRIPT` errors so
    /// the script is transparently reloaded (e.g. after a Redis restart).
    script_sha: tokio::sync::RwLock<Option<String>>,
}

#[cfg(feature = "redis-rate-limiting")]
impl RedisRateLimiter {
    /// Connect to Redis and prepare the rate limiter.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or the connection fails.
    pub(super) async fn new(url: &str, config: RateLimitConfig) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let pool = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self {
            pool,
            config,
            path_rules: Vec::new(),
            script_sha: tokio::sync::RwLock::new(None),
        })
    }

    /// Attach per-path rules from `[security.rate_limiting]` auth endpoint fields.
    #[must_use]
    pub(super) fn with_path_rules_from_security(mut self, sec: &RateLimitingSecurityConfig) -> Self {
        let mut rules = Vec::new();

        if sec.auth_start_max_requests > 0 && sec.auth_start_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/start".to_string(),
                tokens_per_sec: f64::from(sec.auth_start_max_requests)
                    / sec.auth_start_window_secs as f64,
                burst:          f64::from(sec.auth_start_max_requests),
            });
        }
        if sec.auth_callback_max_requests > 0 && sec.auth_callback_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/callback".to_string(),
                tokens_per_sec: f64::from(sec.auth_callback_max_requests)
                    / sec.auth_callback_window_secs as f64,
                burst:          f64::from(sec.auth_callback_max_requests),
            });
        }
        if sec.auth_refresh_max_requests > 0 && sec.auth_refresh_window_secs > 0 {
            rules.push(PathRateLimit {
                path_prefix:    "/auth/refresh".to_string(),
                tokens_per_sec: f64::from(sec.auth_refresh_max_requests)
                    / sec.auth_refresh_window_secs as f64,
                burst:          f64::from(sec.auth_refresh_max_requests),
            });
        }

        self.path_rules = rules;
        self
    }

    /// Return the active rate limit configuration.
    pub(super) const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Number of per-path rate limit rules registered.
    pub(super) const fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    pub(super) fn retry_after_for_path(&self, path: &str) -> u32 {
        if let Some(rule) =
            self.path_rules.iter().find(|r| path.starts_with(r.path_prefix.as_str()))
        {
            if rule.tokens_per_sec > 0.0 {
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }

    /// Load the Lua script into Redis and cache its SHA for subsequent calls.
    async fn load_script(&self) -> Result<String, redis::RedisError> {
        let cached_sha = self.script_sha.read().await.as_ref().cloned();
        if let Some(sha) = cached_sha {
            return Ok(sha);
        }
        let mut conn = self.pool.clone();
        let sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(RATE_LIMIT_LUA)
            .query_async(&mut conn)
            .await?;
        *self.script_sha.write().await = Some(sha.clone());
        Ok(sha)
    }

    /// Execute the Lua token-bucket script atomically.
    ///
    /// Tokens and capacity are passed as **milli-tokens** (× 1000) so that
    /// sub-1-rps rates are represented without truncation.  The Lua script
    /// stores milli-tokens internally; `remaining_tokens` in the returned
    /// result is converted back to whole tokens by dividing by 1 000.
    ///
    /// Falls back from `EVALSHA` to a fresh reload on `NOSCRIPT` errors
    /// (script cache cleared after a Redis restart).
    async fn check_and_decrement(
        &self,
        key: &str,
        capacity: u32,
        rate_per_sec: f64,
    ) -> Result<RedisRateLimitResult, redis::RedisError> {
        let sha = self.load_script().await?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut conn = self.pool.clone();

        // Convert to milli-token precision so fractional rates (e.g. 0.167 req/s)
        // are not truncated.  Minimum rate_millis = 1 (0.001 req/s) to avoid /0.
        let capacity_millis = u64::from(capacity) * 1000;
        let rate_millis = ((rate_per_sec * 1000.0) as u64).max(1);

        let do_evalsha = |sha: &str| {
            redis::cmd("EVALSHA")
                .arg(sha)
                .arg(1)
                .arg(key)
                .arg(capacity_millis)
                .arg(rate_millis)
                .arg(now_ms)
                .to_owned()
        };

        let result: Vec<i64> = match do_evalsha(&sha).query_async(&mut conn).await {
            Ok(r) => r,
            Err(e) if e.kind() == redis::ErrorKind::NoScriptError => {
                // Script cache was cleared (e.g. Redis restart) — reload and retry.
                *self.script_sha.write().await = None;
                let sha2 = self.load_script().await?;
                do_evalsha(&sha2).query_async(&mut conn).await?
            },
            Err(e) => return Err(e),
        };

        Ok(RedisRateLimitResult {
            allowed:          result[0] == 1,
            // Convert milli-tokens back to whole tokens for the header.
            remaining_tokens: result[1] as f64 / 1000.0,
            retry_after_ms:   result[2] as u64,
        })
    }

    /// Check a key against the token bucket, failing open on Redis error.
    ///
    /// Returns `(allowed, remaining_tokens, retry_after_ms)`.
    async fn check_key(
        &self,
        key: &str,
        capacity: u32,
        rate_per_sec: f64,
    ) -> (bool, f64, u64) {
        if !self.config.enabled {
            return (true, f64::from(self.config.burst_size), 0);
        }
        match self.check_and_decrement(key, capacity, rate_per_sec).await {
            Ok(r) => (r.allowed, r.remaining_tokens, r.retry_after_ms),
            Err(e) => {
                REDIS_RATE_LIMIT_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(error = %e, "Redis rate limiter error — failing open");
                (true, f64::from(self.config.burst_size), 0)
            },
        }
    }

    /// Check IP limit using the Redis token bucket.
    pub(super) async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        let key = format!("fraiseql:rl:ip:{ip}");
        let (allowed, remaining, retry_after_ms) =
            self.check_key(&key, self.config.burst_size, f64::from(self.config.rps_per_ip))
                .await;
        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, "Redis rate limit exceeded for IP");
            CheckResult::deny(retry_after_ms_to_secs(retry_after_ms))
        }
    }

    /// Check user limit using the Redis token bucket.
    pub(super) async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        let key = format!("fraiseql:rl:user:{user_id}");
        let (allowed, remaining, retry_after_ms) =
            self.check_key(&key, self.config.burst_size, f64::from(self.config.rps_per_user))
                .await;
        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(user_id = user_id, "Redis rate limit exceeded for user");
            CheckResult::deny(retry_after_ms_to_secs(retry_after_ms))
        }
    }

    /// Check per-path limit for `ip` on `path`.
    ///
    /// Returns an allowed [`CheckResult`] when no rule governs the path.
    pub(super) async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }
        let rule = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix));
        let Some(rule) = rule else {
            return CheckResult::allow(f64::from(self.config.burst_size));
        };
        let key = format!("fraiseql:rl:path:{}:{ip}", rule.path_prefix);
        // Capacity must be ≥ 1 (milli-token precision handles sub-1 rates).
        let capacity = (rule.burst as u32).max(1);
        let (allowed, remaining, retry_after_ms) =
            self.check_key(&key, capacity, rule.tokens_per_sec).await;
        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, path = path, "Redis per-path rate limit exceeded");
            CheckResult::deny(retry_after_ms_to_secs(retry_after_ms))
        }
    }
}
