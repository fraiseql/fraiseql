//! Rate limiting middleware for GraphQL requests.
//!
//! Implements request rate limiting with:
//! - Per-IP rate limiting
//! - Per-user rate limiting (if authenticated)
//! - Per-path rate limiting (for auth endpoints)
//! - Token bucket algorithm
//! - Configurable burst capacity
//! - X-RateLimit headers

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Minimal mirror of the `[security.rate_limiting]` TOML section, deserialized
/// from the compiled schema's `security.rate_limiting` JSON key.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RateLimitingSecurityConfig {
    /// Enable rate limiting.
    pub enabled: bool,
    /// Global request rate cap (requests per second, per IP).
    pub requests_per_second: u32,
    /// Burst allowance above the steady-state rate.
    pub burst_size: u32,
    /// Auth initiation endpoint — max requests per window.
    pub auth_start_max_requests: u32,
    /// Auth initiation window in seconds.
    pub auth_start_window_secs: u64,
    /// OAuth callback endpoint — max requests per window.
    pub auth_callback_max_requests: u32,
    /// OAuth callback window in seconds.
    pub auth_callback_window_secs: u64,
    /// Token refresh endpoint — max requests per window.
    pub auth_refresh_max_requests: u32,
    /// Token refresh window in seconds.
    pub auth_refresh_window_secs: u64,
    /// Per-authenticated-user request rate in requests/second.
    /// Defaults to 10× `requests_per_second` if not set.
    #[serde(default)]
    pub requests_per_second_per_user: Option<u32>,
    /// Redis URL for distributed rate limiting (not yet implemented).
    pub redis_url: Option<String>,
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Requests per second per IP
    pub rps_per_ip: u32,

    /// Requests per second per user (if authenticated)
    pub rps_per_user: u32,

    /// Burst capacity (maximum tokens to accumulate)
    pub burst_size: u32,

    /// Cleanup interval in seconds (remove stale entries)
    pub cleanup_interval_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:               true,
            rps_per_ip:            100,  // 100 req/sec per IP
            rps_per_user:          1000, // 1000 req/sec per user
            burst_size:            500,  // Allow bursts up to 500 requests
            cleanup_interval_secs: 300,  // Clean up every 5 minutes
        }
    }
}

impl RateLimitConfig {
    /// Build from the `[security.rate_limiting]` config embedded in the compiled schema.
    ///
    /// Maps `requests_per_second` → `rps_per_ip` and `burst_size` directly.
    /// `rps_per_user` uses the explicit `requests_per_second_per_user` value when set,
    /// or defaults to 10× `requests_per_second`.
    ///
    /// The default 10× multiplier reflects that authenticated users are identifiable
    /// (abuse is traceable) and include service accounts with higher call rates.
    /// Operators can override with `requests_per_second_per_user` in `fraiseql.toml`.
    pub fn from_security_config(sec: &RateLimitingSecurityConfig) -> Self {
        Self {
            enabled:               sec.enabled,
            rps_per_ip:            sec.requests_per_second,
            rps_per_user:          sec
                .requests_per_second_per_user
                .unwrap_or_else(|| sec.requests_per_second.saturating_mul(10)),
            burst_size:            sec.burst_size,
            cleanup_interval_secs: 300,
        }
    }
}

/// A per-path rate limit rule, derived from `[security.rate_limiting]` auth endpoint fields.
#[derive(Debug, Clone)]
struct PathRateLimit {
    /// Path prefix to match (exact prefix, e.g., `/auth/start`).
    path_prefix:    String,
    /// Token refill rate (tokens per second = max_requests / window_secs).
    tokens_per_sec: f64,
    /// Maximum burst (= max_requests).
    burst:          f64,
}

/// Token bucket for rate limiting.
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current token count
    tokens: f64,

    /// Maximum tokens
    capacity: f64,

    /// Refill rate (tokens per second)
    refill_rate: f64,

    /// Last refill timestamp
    last_refill: std::time::Instant,
}

impl TokenBucket {
    /// Create new token bucket.
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: std::time::Instant::now(),
        }
    }

    /// Try to consume tokens. Returns true if allowed, false if rate limited.
    fn try_consume(&mut self, tokens: f64) -> bool {
        // Refill based on elapsed time
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let refilled = elapsed * self.refill_rate;
        self.tokens = (self.tokens + refilled).min(self.capacity);
        self.last_refill = now;

        // Try to consume
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count.
    fn token_count(&self) -> f64 {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let refilled = elapsed * self.refill_rate;
        (self.tokens + refilled).min(self.capacity)
    }
}

/// In-memory token-bucket rate limiter.
pub struct InMemoryRateLimiter {
    config:          RateLimitConfig,
    // IP -> TokenBucket (global limit)
    ip_buckets:      Arc<RwLock<HashMap<String, TokenBucket>>>,
    // User ID -> TokenBucket
    user_buckets:    Arc<RwLock<HashMap<String, TokenBucket>>>,
    // Per-path rules (from [security.rate_limiting] auth endpoint fields)
    path_rules:      Vec<PathRateLimit>,
    // (path_prefix, ip) -> TokenBucket
    path_ip_buckets: Arc<RwLock<HashMap<(String, String), TokenBucket>>>,
}

impl InMemoryRateLimiter {
    /// Create new in-memory rate limiter.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_buckets:      Arc::new(RwLock::new(HashMap::new())),
            user_buckets:    Arc::new(RwLock::new(HashMap::new())),
            path_rules:      Vec::new(),
            path_ip_buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Attach per-path rules derived from `[security.rate_limiting]` auth endpoint fields.
    ///
    /// Converts max-requests-per-window into token-per-second refill rates.
    #[must_use]
    pub fn with_path_rules_from_security(mut self, sec: &RateLimitingSecurityConfig) -> Self {
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

    /// Check if request to `path` from `ip` is within the per-path limit.
    ///
    /// Returns `true` if allowed (or if no rule matches the path). Returns
    /// `false` only when a matching per-path rule exists and the bucket is empty.
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let rule = self.path_rules.iter().find(|r| path.starts_with(r.path_prefix.as_str()));
        let Some(rule) = rule else { return true };

        let key = (rule.path_prefix.clone(), ip.to_string());
        let (tokens_per_sec, burst) = (rule.tokens_per_sec, rule.burst);

        let mut buckets = self.path_ip_buckets.write().await;
        let bucket = buckets
            .entry(key)
            .or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));

        let allowed = bucket.try_consume(1.0);
        if !allowed {
            debug!(ip = ip, path = path, "Per-path rate limit exceeded");
        }
        allowed
    }

    /// Get rate limiter configuration.
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed for given IP.
    pub async fn check_ip_limit(&self, ip: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let mut buckets = self.ip_buckets.write().await;
        let bucket = buckets.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.burst_size as f64, self.config.rps_per_ip as f64)
        });

        let allowed = bucket.try_consume(1.0);

        if !allowed {
            debug!(ip = ip, "Rate limit exceeded for IP");
        }

        allowed
    }

    /// Check if request is allowed for given user.
    pub async fn check_user_limit(&self, user_id: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let mut buckets = self.user_buckets.write().await;
        let bucket = buckets.entry(user_id.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.burst_size as f64, self.config.rps_per_user as f64)
        });

        let allowed = bucket.try_consume(1.0);

        if !allowed {
            debug!(user_id = user_id, "Rate limit exceeded for user");
        }

        allowed
    }

    /// Get remaining tokens for IP (for X-RateLimit headers).
    pub async fn get_ip_remaining(&self, ip: &str) -> f64 {
        let buckets = self.ip_buckets.read().await;
        buckets
            .get(ip)
            .map(|b| b.token_count())
            .unwrap_or(self.config.burst_size as f64)
    }

    /// Get remaining tokens for user (for X-RateLimit headers).
    pub async fn get_user_remaining(&self, user_id: &str) -> f64 {
        let buckets = self.user_buckets.read().await;
        buckets
            .get(user_id)
            .map(|b| b.token_count())
            .unwrap_or(self.config.burst_size as f64)
    }

    /// Cleanup stale entries (should be called periodically).
    pub async fn cleanup(&self) {
        let ip_buckets = self.ip_buckets.read().await;
        let user_buckets = self.user_buckets.read().await;

        // Keep entries that have been accessed recently
        // For now, keep all (cleanup is optional)
        debug!(
            ip_buckets = ip_buckets.len(),
            user_buckets = user_buckets.len(),
            "Rate limiter state"
        );
    }

    /// Number of per-path rate limit rules registered.
    pub fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }
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
    allowed: bool,
    // retry_after_ms is returned by the Lua script and preserved for future use
    // (e.g. surfacing in `Retry-After` headers from the Redis backend).
    #[allow(dead_code)]
    retry_after_ms: u64,
}

/// Atomic token-bucket Lua script for Redis.
///
/// Arguments:
/// - `KEYS[1]`  — bucket key (e.g. `fraiseql:rl:ip:1.2.3.4`)
/// - `ARGV[1]`  — capacity  (burst_size, integer tokens)
/// - `ARGV[2]`  — refill rate (tokens per second, integer)
/// - `ARGV[3]`  — now (Unix timestamp in **milliseconds**)
///
/// Returns `[allowed (0|1), remaining_tokens, retry_after_ms]`.
#[cfg(feature = "redis-rate-limiting")]
const RATE_LIMIT_LUA: &str = r"
local key      = KEYS[1]
local capacity = tonumber(ARGV[1])
local rate     = tonumber(ARGV[2])
local now      = tonumber(ARGV[3])

local bucket     = redis.call('HMGET', key, 'tokens', 'last_refill')
local tokens     = tonumber(bucket[1]) or capacity
local last_refill = tonumber(bucket[2]) or now

local elapsed = math.max(0, now - last_refill) / 1000.0
local refill  = math.floor(elapsed * rate)
tokens = math.min(capacity, tokens + refill)

if tokens >= 1 then
    tokens = tokens - 1
    redis.call('HMSET', key, 'tokens', tokens, 'last_refill', now)
    redis.call('PEXPIRE', key, math.ceil(capacity / rate * 1000))
    return {1, tokens, 0}
else
    local retry_ms = math.ceil((1 - tokens) / rate * 1000)
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
    pub async fn new(url: &str, config: RateLimitConfig) -> Result<Self, redis::RedisError> {
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
    pub fn with_path_rules_from_security(mut self, sec: &RateLimitingSecurityConfig) -> Self {
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
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Number of per-path rate limit rules registered.
    pub fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Load the Lua script into Redis and cache its SHA for subsequent calls.
    async fn load_script(&self) -> Result<String, redis::RedisError> {
        if let Some(sha) = self.script_sha.read().await.as_ref().cloned() {
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
        // Lua receives integer tokens/sec; ensure at least 1 to avoid /0 in the script.
        let rate_arg = (rate_per_sec as u64).max(1);

        let result: Vec<i64> = match redis::cmd("EVALSHA")
            .arg(&sha)
            .arg(1)
            .arg(key)
            .arg(capacity)
            .arg(rate_arg)
            .arg(now_ms)
            .query_async(&mut conn)
            .await
        {
            Ok(r) => r,
            Err(e) if e.kind() == redis::ErrorKind::NoScriptError => {
                // Script cache was cleared (e.g. Redis restart) — reload and retry.
                *self.script_sha.write().await = None;
                let sha2 = self.load_script().await?;
                redis::cmd("EVALSHA")
                    .arg(&sha2)
                    .arg(1)
                    .arg(key)
                    .arg(capacity)
                    .arg(rate_arg)
                    .arg(now_ms)
                    .query_async(&mut conn)
                    .await?
            },
            Err(e) => return Err(e),
        };

        Ok(RedisRateLimitResult {
            allowed:        result[0] == 1,
            retry_after_ms: result[2] as u64,
        })
    }

    /// Check a key against the token bucket, failing open on Redis error.
    async fn check_key(&self, key: &str, capacity: u32, rate_per_sec: f64) -> bool {
        if !self.config.enabled {
            return true;
        }
        match self.check_and_decrement(key, capacity, rate_per_sec).await {
            Ok(result) => result.allowed,
            Err(e) => {
                REDIS_RATE_LIMIT_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(error = %e, "Redis rate limiter error — failing open");
                true
            },
        }
    }

    /// Check IP limit using the Redis token bucket.
    pub async fn check_ip_limit(&self, ip: &str) -> bool {
        let key = format!("fraiseql:rl:ip:{ip}");
        self.check_key(&key, self.config.burst_size, f64::from(self.config.rps_per_ip))
            .await
    }

    /// Check user limit using the Redis token bucket.
    pub async fn check_user_limit(&self, user_id: &str) -> bool {
        let key = format!("fraiseql:rl:user:{user_id}");
        self.check_key(&key, self.config.burst_size, f64::from(self.config.rps_per_user))
            .await
    }

    /// Check per-path limit for `ip` on `path`.
    ///
    /// Returns `true` (allowed) when no rule matches the path.
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> bool {
        if !self.config.enabled {
            return true;
        }
        let rule =
            self.path_rules.iter().find(|r| path.starts_with(r.path_prefix.as_str()));
        let Some(rule) = rule else { return true };
        let key = format!("fraiseql:rl:path:{}:{ip}", rule.path_prefix);
        // capacity must be ≥1 to avoid division by zero in the Lua script
        let capacity = (rule.burst as u32).max(1);
        self.check_key(&key, capacity, rule.tokens_per_sec).await
    }

    /// Remaining tokens for `ip` (approximated; actual value lives in Redis).
    pub async fn get_ip_remaining(&self, _ip: &str) -> f64 {
        f64::from(self.config.burst_size)
    }

    /// Remaining tokens for `user_id` (approximated).
    pub async fn get_user_remaining(&self, _user_id: &str) -> f64 {
        f64::from(self.config.burst_size)
    }
}

// ─── Unified RateLimiter enum ─────────────────────────────────────────────────

/// Rate limiter that dispatches to either an in-memory or Redis backend.
///
/// Construct via [`RateLimiter::new`] (in-memory, default) or
/// [`RateLimiter::new_redis`] (distributed Redis, requires the
/// `redis-rate-limiting` Cargo feature).
pub enum RateLimiter {
    /// Single-node token-bucket limiter backed by `HashMap` with `RwLock`.
    InMemory(InMemoryRateLimiter),
    /// Distributed token-bucket limiter backed by Redis Lua scripts.
    #[cfg(feature = "redis-rate-limiting")]
    Redis(RedisRateLimiter),
}

impl RateLimiter {
    /// Create an in-memory rate limiter.
    pub fn new(config: RateLimitConfig) -> Self {
        Self::InMemory(InMemoryRateLimiter::new(config))
    }

    /// Create a Redis-backed distributed rate limiter.
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis URL is invalid or the initial connection
    /// attempt fails.
    #[cfg(feature = "redis-rate-limiting")]
    pub async fn new_redis(url: &str, config: RateLimitConfig) -> Result<Self, redis::RedisError> {
        let rl = RedisRateLimiter::new(url, config).await?;
        Ok(Self::Redis(rl))
    }

    /// Attach per-path rules from `[security.rate_limiting]` auth endpoint fields.
    #[must_use]
    pub fn with_path_rules_from_security(self, sec: &RateLimitingSecurityConfig) -> Self {
        match self {
            Self::InMemory(rl) => Self::InMemory(rl.with_path_rules_from_security(sec)),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => Self::Redis(rl.with_path_rules_from_security(sec)),
        }
    }

    /// Return the active rate limit configuration.
    pub fn config(&self) -> &RateLimitConfig {
        match self {
            Self::InMemory(rl) => rl.config(),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.config(),
        }
    }

    /// Number of per-path rate limit rules registered.
    pub fn path_rule_count(&self) -> usize {
        match self {
            Self::InMemory(rl) => rl.path_rule_count(),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.path_rule_count(),
        }
    }

    /// Check whether a request from `ip` is within the global IP rate limit.
    pub async fn check_ip_limit(&self, ip: &str) -> bool {
        match self {
            Self::InMemory(rl) => rl.check_ip_limit(ip).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_ip_limit(ip).await,
        }
    }

    /// Check whether a request from `user_id` is within the per-user limit.
    pub async fn check_user_limit(&self, user_id: &str) -> bool {
        match self {
            Self::InMemory(rl) => rl.check_user_limit(user_id).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_user_limit(user_id).await,
        }
    }

    /// Check the per-path rate limit for a request from `ip` to `path`.
    ///
    /// Returns `true` (allowed) when no rule matches the path.
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> bool {
        match self {
            Self::InMemory(rl) => rl.check_path_limit(path, ip).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_path_limit(path, ip).await,
        }
    }

    /// Return the number of seconds a client must wait before the next request
    /// is likely to be accepted under the IP-level token-bucket rate limit.
    ///
    /// Computed as `ceil(1 / rps_per_ip)` — the time for the bucket to refill
    /// one token at the configured rate.  Minimum value is 1 second.
    ///
    /// This value is used for the `Retry-After` HTTP response header so clients
    /// always receive an accurate back-off hint instead of a hardcoded constant.
    #[must_use]
    pub fn retry_after_secs(&self) -> u32 {
        let rps = self.config().rps_per_ip;
        if rps == 0 {
            return 1;
        }
        // ceil(1.0 / rps): e.g. 100 rps → 1s, 1 rps → 1s, 0.5 rps → 2s
        let secs = (1.0_f64 / f64::from(rps)).ceil() as u32;
        secs.max(1)
    }

    /// Remaining tokens for `ip` (used for `X-RateLimit-Remaining` header).
    pub async fn get_ip_remaining(&self, ip: &str) -> f64 {
        match self {
            Self::InMemory(rl) => rl.get_ip_remaining(ip).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.get_ip_remaining(ip).await,
        }
    }

    /// Remaining tokens for `user_id`.
    pub async fn get_user_remaining(&self, user_id: &str) -> f64 {
        match self {
            Self::InMemory(rl) => rl.get_user_remaining(user_id).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.get_user_remaining(user_id).await,
        }
    }

    /// Evict stale in-memory buckets.
    ///
    /// No-op for the Redis backend — Redis handles expiry via `PEXPIRE`.
    pub async fn cleanup(&self) {
        match self {
            Self::InMemory(rl) => rl.cleanup().await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(_) => {},
        }
    }
}

/// Rate limit middleware response.
///
/// Carries the number of seconds the client should wait before retrying,
/// derived from the active rate-limit configuration at the time the request
/// was rejected.  This value is emitted as both the `Retry-After` HTTP header
/// and in the GraphQL error message body.
#[derive(Debug)]
pub struct RateLimitExceeded {
    /// Seconds until the token bucket refills by at least one token.
    pub retry_after_secs: u32,
}

impl IntoResponse for RateLimitExceeded {
    fn into_response(self) -> Response {
        let retry = self.retry_after_secs;
        let retry_str = retry.to_string();
        let body = format!(
            r#"{{"errors":[{{"message":"Rate limit exceeded. Please retry after {retry} second{s}."}}]}}"#,
            s = if retry == 1 { "" } else { "s" }
        );
        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Content-Type", "application/json"),
                ("Retry-After", retry_str.as_str()),
            ],
            body,
        )
            .into_response()
    }
}

/// Rate limiting middleware for GraphQL requests.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, RateLimitExceeded> {
    // Get or create rate limiter from state
    let limiter = req
        .extensions()
        .get::<Arc<RateLimiter>>()
        .cloned()
        .unwrap_or_else(|| Arc::new(RateLimiter::new(RateLimitConfig::default())));

    let ip = addr.ip().to_string();
    let path = req.uri().path().to_string();

    // Check global IP rate limit
    if !limiter.check_ip_limit(&ip).await {
        warn!(ip = %ip, "IP rate limit exceeded");
        return Err(RateLimitExceeded { retry_after_secs: limiter.retry_after_secs() });
    }

    // Check per-path rate limit (e.g., /auth/start stricter than global)
    if !limiter.check_path_limit(&path, &ip).await {
        warn!(ip = %ip, path = %path, "Per-path rate limit exceeded");
        return Err(RateLimitExceeded { retry_after_secs: limiter.retry_after_secs() });
    }

    // Get remaining tokens for response headers
    let remaining = limiter.get_ip_remaining(&ip).await;

    let response = next.run(req).await;

    // Add rate limit headers
    let mut response = response;
    if let Ok(limit_value) = format!("{}", limiter.config().rps_per_ip).parse() {
        response.headers_mut().insert("X-RateLimit-Limit", limit_value);
    }
    if let Ok(remaining_value) = format!("{}", remaining as u32).parse() {
        response.headers_mut().insert("X-RateLimit-Remaining", remaining_value);
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(10.0, 5.0);
        assert_eq!(bucket.tokens, 10.0);
        assert_eq!(bucket.capacity, 10.0);
        assert_eq!(bucket.refill_rate, 5.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10.0, 5.0);
        assert!(bucket.try_consume(5.0));
        assert!((bucket.tokens - 5.0).abs() < 0.001); // Allow float precision
        assert!(bucket.try_consume(5.0));
        assert!(bucket.tokens.abs() < 0.001); // Allow float precision
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 5.0);
        assert!(bucket.try_consume(10.0)); // Consume all
        assert_eq!(bucket.tokens, 0.0);

        // Simulate time passing and refilling
        std::thread::sleep(std::time::Duration::from_millis(200));

        // After 0.2 seconds with 5 tokens/sec, should have 1 token
        assert!(bucket.try_consume(1.0));
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.rps_per_ip, 100);
        assert_eq!(config.rps_per_user, 1000);
    }

    #[tokio::test]
    async fn test_rate_limiter_ip_allow() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_ip: 10,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.check_ip_limit("127.0.0.1").await);
        assert!(limiter.check_ip_limit("127.0.0.1").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_ip_block() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_ip: 1,
            burst_size: 1,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.check_ip_limit("127.0.0.1").await);
        assert!(!limiter.check_ip_limit("127.0.0.1").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            rps_per_ip: 1,
            burst_size: 1,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.check_ip_limit("127.0.0.1").await);
        assert!(limiter.check_ip_limit("127.0.0.1").await); // Should allow despite limit
    }

    #[tokio::test]
    async fn test_rate_limiter_different_ips() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_ip: 1,
            burst_size: 1,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.check_ip_limit("192.168.1.1").await);
        assert!(limiter.check_ip_limit("192.168.1.2").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_user_limit() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_user: 2,
            burst_size: 2,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        assert!(limiter.check_user_limit("user123").await);
        assert!(limiter.check_user_limit("user123").await);
        assert!(!limiter.check_user_limit("user123").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_remaining() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_ip: 10,
            burst_size: 10,
            ..Default::default()
        };

        let limiter = RateLimiter::new(config);
        let before = limiter.get_ip_remaining("127.0.0.1").await;
        assert_eq!(before, 10.0);

        limiter.check_ip_limit("127.0.0.1").await;
        let after = limiter.get_ip_remaining("127.0.0.1").await;
        assert!(after < before);
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        limiter.check_ip_limit("127.0.0.1").await;
        limiter.cleanup().await; // Should not panic
    }

    // --- Phase 05: from_security_config() tests ---

    #[test]
    fn test_from_security_config_maps_fields() {
        let sec = RateLimitingSecurityConfig {
            enabled:             true,
            requests_per_second: 50,
            burst_size:          150,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(cfg.enabled);
        assert_eq!(cfg.rps_per_ip, 50);
        assert_eq!(cfg.burst_size, 150);
    }

    #[test]
    fn test_from_security_config_disabled() {
        let sec = RateLimitingSecurityConfig { enabled: false, ..Default::default() };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(!cfg.enabled);
    }

    #[test]
    fn test_from_security_config_user_limit_is_higher() {
        let sec = RateLimitingSecurityConfig {
            enabled:             true,
            requests_per_second: 100,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(cfg.rps_per_user > cfg.rps_per_ip);
    }

    #[test]
    fn test_from_security_config_defaults_per_user_to_10x() {
        let sec = RateLimitingSecurityConfig {
            enabled:             true,
            requests_per_second: 50,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert_eq!(cfg.rps_per_user, 500); // 50 × 10 default
    }

    #[test]
    fn test_from_security_config_custom_per_user_rps_overrides_default() {
        let sec = RateLimitingSecurityConfig {
            enabled:                       true,
            requests_per_second:           100,
            requests_per_second_per_user:  Some(250),
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert_eq!(cfg.rps_per_user, 250); // explicit value used
        assert_eq!(cfg.rps_per_ip, 100);   // global unchanged
    }

    #[test]
    fn test_with_path_rules_generates_auth_start_rule() {
        let sec = RateLimitingSecurityConfig {
            enabled:                 true,
            requests_per_second:     100,
            burst_size:              200,
            auth_start_max_requests: 5,
            auth_start_window_secs:  60,
            ..Default::default()
        };
        let config = RateLimitConfig::from_security_config(&sec);
        let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);
        // Verify exactly one path rule was registered for /auth/start
        assert_eq!(limiter.path_rule_count(), 1);
    }

    #[tokio::test]
    async fn test_check_path_limit_allows_unknown_path() {
        let sec = RateLimitingSecurityConfig {
            enabled:                 true,
            requests_per_second:     10,
            burst_size:              10,
            auth_start_max_requests: 1,
            auth_start_window_secs:  60,
            ..Default::default()
        };
        let config = RateLimitConfig::from_security_config(&sec);
        let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);
        // GraphQL path has no path rule → always allowed
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await);
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await);
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await);
    }

    #[tokio::test]
    async fn test_check_path_limit_enforces_auth_start() {
        let sec = RateLimitingSecurityConfig {
            enabled:                 true,
            requests_per_second:     1000,
            burst_size:              1000,
            auth_start_max_requests: 1,
            auth_start_window_secs:  60,
            ..Default::default()
        };
        let config = RateLimitConfig::from_security_config(&sec);
        let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);
        // First request: allowed (burst = 1)
        assert!(limiter.check_path_limit("/auth/start", "1.2.3.4").await);
        // Second request: blocked (bucket empty)
        assert!(!limiter.check_path_limit("/auth/start", "1.2.3.4").await);
    }

    #[tokio::test]
    async fn test_check_path_limit_different_ips_independent() {
        let sec = RateLimitingSecurityConfig {
            enabled:                 true,
            requests_per_second:     1000,
            burst_size:              1000,
            auth_start_max_requests: 1,
            auth_start_window_secs:  60,
            ..Default::default()
        };
        let config = RateLimitConfig::from_security_config(&sec);
        let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);
        // Exhaust bucket for IP1
        assert!(limiter.check_path_limit("/auth/start", "1.1.1.1").await);
        assert!(!limiter.check_path_limit("/auth/start", "1.1.1.1").await);
        // IP2 still gets its full allowance
        assert!(limiter.check_path_limit("/auth/start", "2.2.2.2").await);
    }

    // ─── retry_after_secs ────────────────────────────────────────────────────

    #[test]
    fn test_retry_after_secs_high_rps() {
        // 100 rps → 1 token per 0.01s → ceil = 1s
        let config = RateLimitConfig { rps_per_ip: 100, ..RateLimitConfig::default() };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_retry_after_secs_one_rps() {
        // 1 rps → 1 token per 1s → ceil = 1s
        let config = RateLimitConfig { rps_per_ip: 1, ..RateLimitConfig::default() };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_retry_after_secs_zero_rps_fallback() {
        // 0 rps (disabled / misconfigured) → safe fallback of 1s
        let config = RateLimitConfig { rps_per_ip: 0, ..RateLimitConfig::default() };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_rate_limit_exceeded_response_uses_config_retry_after() {
        use axum::response::IntoResponse;
        let resp = RateLimitExceeded { retry_after_secs: 5 }.into_response();
        let header = resp
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(header, "5");
    }

    // ─── Redis integration tests ─────────────────────────────────────────────
    // These require a live Redis instance.  Run with:
    //   REDIS_URL=redis://localhost:6379 cargo test -p fraiseql-server \
    //     --features redis-rate-limiting -- redis_rate_limiter --ignored

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_rate_limiter_allows_up_to_capacity() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let config = RateLimitConfig {
            enabled:               true,
            rps_per_ip:            5,
            rps_per_user:          5,
            burst_size:            5,
            cleanup_interval_secs: 300,
        };
        let rl = RateLimiter::new_redis(&url, config).await.expect("Redis connection failed");
        // Use a unique key to avoid interference between test runs
        let ip = format!("test_allow:{}", uuid::Uuid::new_v4());
        for _ in 0..5 {
            assert!(rl.check_ip_limit(&ip).await, "should be allowed within capacity");
        }
        assert!(!rl.check_ip_limit(&ip).await, "6th request should be rejected");
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_two_instances_share_bucket() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let config = RateLimitConfig {
            enabled:               true,
            rps_per_ip:            3,
            rps_per_user:          3,
            burst_size:            3,
            cleanup_interval_secs: 300,
        };
        let suffix = uuid::Uuid::new_v4();
        let a = RateLimiter::new_redis(&url, config.clone())
            .await
            .expect("Redis connection failed");
        let b = RateLimiter::new_redis(&url, config)
            .await
            .expect("Redis connection failed");
        let ip = format!("test_shared:{suffix}");

        // Instance A consumes 2 tokens
        assert!(a.check_ip_limit(&ip).await);
        assert!(a.check_ip_limit(&ip).await);
        // Instance B consumes the 3rd token
        assert!(b.check_ip_limit(&ip).await);
        // 4th token across both instances should be rejected
        assert!(!b.check_ip_limit(&ip).await, "4th request should be rejected across instances");
    }
}
