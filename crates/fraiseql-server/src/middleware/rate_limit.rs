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
    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for the client IP.
    ///
    /// Enable only when FraiseQL is deployed behind a trusted reverse proxy
    /// (e.g. nginx, Cloudflare, AWS ALB) that sets these headers.  Enabling
    /// without a trusted proxy allows clients to spoof their IP address.
    #[serde(default)]
    pub trust_proxy_headers: bool,
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

    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for client IP extraction.
    ///
    /// Must only be enabled when behind a trusted reverse proxy.
    pub trust_proxy_headers: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:               true,
            rps_per_ip:            100,  // 100 req/sec per IP
            rps_per_user:          1000, // 1000 req/sec per user
            burst_size:            500,  // Allow bursts up to 500 requests
            cleanup_interval_secs: 300,  // Clean up every 5 minutes
            trust_proxy_headers:   false,
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
            trust_proxy_headers:   sec.trust_proxy_headers,
        }
    }
}

/// Result returned by all `check_*` rate-limit methods.
///
/// Carries the allow/deny decision, the approximate remaining token count
/// (used for the `X-RateLimit-Remaining` response header), and the
/// recommended `Retry-After` interval in seconds (0 when the request was
/// allowed).
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Whether the request should be allowed.
    pub allowed:           bool,
    /// Tokens remaining in the bucket after this request (≥ 0).
    pub remaining:         f64,
    /// Seconds the client should wait before retrying (0 when allowed).
    pub retry_after_secs: u32,
}

impl CheckResult {
    fn allow(remaining: f64) -> Self {
        Self { allowed: true, remaining, retry_after_secs: 0 }
    }

    fn deny(retry_after_secs: u32) -> Self {
        Self { allowed: false, remaining: 0.0, retry_after_secs }
    }
}

/// Build a namespaced rate-limiting key for use in both in-memory and Redis backends.
///
/// Format: `fraiseql:rl:{strategy}:{identifier}` for simple strategies, or
/// `fraiseql:rl:{strategy}:{prefix}:{identifier}` when an optional path prefix is supplied.
///
/// Exposed as `pub` for property testing.
pub fn build_rate_limit_key(
    strategy: &str,
    identifier: &str,
    prefix: Option<&str>,
) -> String {
    match prefix {
        Some(p) => format!("fraiseql:rl:{strategy}:{p}:{identifier}"),
        None => format!("fraiseql:rl:{strategy}:{identifier}"),
    }
}

/// Returns `true` if `ip` is a loopback or RFC 1918 private address.
///
/// Used to warn operators that rate limiting may be inoperative when running
/// behind a reverse proxy without `trust_proxy_headers = true`.
fn is_private_or_loopback(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Returns `true` if `path` is governed by the rule whose canonical prefix is
/// `prefix`.
///
/// Requires that `path` equals `prefix` exactly, or that it is followed
/// immediately by `/` or `?`. This prevents `/auth/start` from matching
/// `/auth/startover` (DoS vector: exhausting the `/auth/start` bucket via an
/// unrelated path).
fn path_matches_rule(path: &str, prefix: &str) -> bool {
    if path == prefix {
        return true;
    }
    let rest = match path.strip_prefix(prefix) {
        Some(r) => r,
        None => return false,
    };
    rest.starts_with('/') || rest.starts_with('?')
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
    /// Returns an allowed [`CheckResult`] when no rule governs the path.
    /// Returns a denied result only when a matching rule exists and the bucket
    /// is empty.  `CheckResult::retry_after_secs` is set to the path-window
    /// interval (`ceil(1 / tokens_per_sec)`).
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let rule = self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix));
        let Some(rule) = rule else {
            return CheckResult::allow(f64::from(self.config.burst_size));
        };

        let key = (rule.path_prefix.clone(), ip.to_string());
        let (tokens_per_sec, burst) = (rule.tokens_per_sec, rule.burst);

        let mut buckets = self.path_ip_buckets.write().await;
        let bucket = buckets
            .entry(key)
            .or_insert_with(|| TokenBucket::new(burst, tokens_per_sec));

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, path = path, "Per-path rate limit exceeded");
            let retry = if tokens_per_sec > 0.0 {
                ((1.0_f64 / tokens_per_sec).ceil() as u32).max(1)
            } else {
                1
            };
            CheckResult::deny(retry)
        }
    }

    /// Get rate limiter configuration.
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed for given IP.
    pub async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut buckets = self.ip_buckets.write().await;
        let bucket = buckets.entry(ip.to_string()).or_insert_with(|| {
            TokenBucket::new(f64::from(self.config.burst_size), f64::from(self.config.rps_per_ip))
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(ip = ip, "Rate limit exceeded for IP");
            let rps = self.config.rps_per_ip;
            let retry = if rps == 0 { 1 } else { ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1) };
            CheckResult::deny(retry)
        }
    }

    /// Check if request is allowed for given user.
    pub async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        if !self.config.enabled {
            return CheckResult::allow(f64::from(self.config.burst_size));
        }

        let mut buckets = self.user_buckets.write().await;
        let bucket = buckets.entry(user_id.to_string()).or_insert_with(|| {
            TokenBucket::new(
                f64::from(self.config.burst_size),
                f64::from(self.config.rps_per_user),
            )
        });

        let allowed = bucket.try_consume(1.0);
        let remaining = bucket.token_count();
        drop(buckets);

        if allowed {
            CheckResult::allow(remaining)
        } else {
            debug!(user_id = user_id, "Rate limit exceeded for user");
            let rps = self.config.rps_per_user;
            let retry =
                if rps == 0 { 1 } else { ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1) };
            CheckResult::deny(retry)
        }
    }

    /// Evict stale in-memory buckets (called by background cleanup task).
    ///
    /// A bucket is stale once it has been idle for longer than the time required
    /// to fully refill from empty (`burst_size / rps_per_ip`).  At that point the
    /// next request would start a fresh full bucket anyway, so the entry is safe
    /// to remove.
    pub async fn cleanup(&self) {
        let ip_refill_secs = if self.config.rps_per_ip == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_ip)
        };
        let user_refill_secs = if self.config.rps_per_user == 0 {
            self.config.cleanup_interval_secs as f64
        } else {
            f64::from(self.config.burst_size) / f64::from(self.config.rps_per_user)
        };

        let now = std::time::Instant::now();
        let ip_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(ip_refill_secs))
            .unwrap_or(now);
        let user_threshold = now
            .checked_sub(std::time::Duration::from_secs_f64(user_refill_secs))
            .unwrap_or(now);

        let mut ip_buckets = self.ip_buckets.write().await;
        let before_ip = ip_buckets.len();
        ip_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        let evicted_ip = before_ip - ip_buckets.len();
        drop(ip_buckets);

        let mut user_buckets = self.user_buckets.write().await;
        let before_user = user_buckets.len();
        user_buckets.retain(|_, b| b.last_refill >= user_threshold);
        let evicted_user = before_user - user_buckets.len();
        drop(user_buckets);

        let mut path_buckets = self.path_ip_buckets.write().await;
        path_buckets.retain(|_, b| b.last_refill >= ip_threshold);
        drop(path_buckets);

        debug!(evicted_ip, evicted_user, "Rate limiter cleanup complete");
    }

    /// Number of per-path rate limit rules registered.
    pub fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    ///
    /// Returns `ceil(1 / tokens_per_sec)` for the rule matching `path`, or 1 if no rule
    /// matches (which shouldn't happen in practice — callers only invoke this after a
    /// rejection).
    pub fn retry_after_for_path(&self, path: &str) -> u32 {
        if let Some(rule) =
            self.path_rules.iter().find(|r| path_matches_rule(path, &r.path_prefix))
        {
            if rule.tokens_per_sec > 0.0 {
                return ((1.0_f64 / rule.tokens_per_sec).ceil() as u32).max(1);
            }
        }
        1
    }
}

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

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    pub fn retry_after_for_path(&self, path: &str) -> u32 {
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
    pub async fn check_ip_limit(&self, ip: &str) -> CheckResult {
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
    pub async fn check_user_limit(&self, user_id: &str) -> CheckResult {
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
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
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

// ─── Unified RateLimiter enum ─────────────────────────────────────────────────

/// Rate limiter that dispatches to either an in-memory or Redis backend.
///
/// Construct via [`RateLimiter::new`] (in-memory, default) or
/// `RateLimiter::new_redis` (distributed Redis, requires the
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

    /// Seconds a client should wait before retrying after a per-path rate limit rejection.
    ///
    /// Returns the window duration for the matching path rule (e.g. 60s for an
    /// auth/start rule with 5 req/60s), not the IP token-bucket interval.
    pub fn retry_after_for_path(&self, path: &str) -> u32 {
        match self {
            Self::InMemory(rl) => rl.retry_after_for_path(path),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.retry_after_for_path(path),
        }
    }

    /// Check whether a request from `ip` is within the global IP rate limit.
    pub async fn check_ip_limit(&self, ip: &str) -> CheckResult {
        match self {
            Self::InMemory(rl) => rl.check_ip_limit(ip).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_ip_limit(ip).await,
        }
    }

    /// Check whether a request from `user_id` is within the per-user limit.
    pub async fn check_user_limit(&self, user_id: &str) -> CheckResult {
        match self {
            Self::InMemory(rl) => rl.check_user_limit(user_id).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_user_limit(user_id).await,
        }
    }

    /// Check the per-path rate limit for a request from `ip` to `path`.
    ///
    /// Returns an allowed [`CheckResult`] when no rule governs the path.
    /// `CheckResult::retry_after_secs` reflects the actual per-path window, not
    /// the global IP rate.
    pub async fn check_path_limit(&self, path: &str, ip: &str) -> CheckResult {
        match self {
            Self::InMemory(rl) => rl.check_path_limit(path, ip).await,
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.check_path_limit(path, ip).await,
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

    /// Conservative static estimate of how long (in seconds) a client must wait
    /// before the IP-level bucket refills one token: `ceil(1 / rps_per_ip)`.
    ///
    /// Used when no backend-computed `retry_after_ms` is available (e.g., the
    /// in-memory backend before the precise value is plumbed end-to-end, or as
    /// a fallback on Redis errors).  Minimum 1 second.
    #[must_use]
    pub fn retry_after_secs(&self) -> u32 {
        let rps = self.config().rps_per_ip;
        if rps == 0 {
            return 1;
        }
        ((1.0_f64 / f64::from(rps)).ceil() as u32).max(1)
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

/// Emitted at most once when the server appears to be behind a proxy but
/// `trust_proxy_headers` is `false` — rate limiting would bucket all requests
/// under the proxy's IP in that configuration.
static PROXY_WARNING_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Extract the real client IP from request headers when behind a trusted reverse proxy.
///
/// Checks `X-Real-IP` first, then the first address in `X-Forwarded-For` (set by
/// the proxy to the original client).  Falls back to the TCP peer address when
/// neither header is present or `trust_proxy` is false.
///
/// **Security**: only enable `trust_proxy` when the server is guaranteed to sit
/// behind a proxy that sets these headers; otherwise clients can spoof the IP.
fn extract_real_ip(
    req: &Request<Body>,
    trust_proxy: bool,
    addr: &SocketAddr,
) -> String {
    if trust_proxy {
        if let Some(real_ip) = req
            .headers()
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return real_ip.to_string();
        }
        if let Some(xff) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
                return first.to_string();
            }
        }
    } else if is_private_or_loopback(addr.ip())
        && !PROXY_WARNING_LOGGED.load(std::sync::atomic::Ordering::Relaxed)
        && !PROXY_WARNING_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        warn!(
            peer_ip = %addr.ip(),
            "Rate limiter: peer address is loopback/RFC-1918 — server appears to be \
             behind a reverse proxy. All requests will share a single rate-limit bucket \
             unless you set `trust_proxy_headers = true` in [security.rate_limiting]."
        );
    }
    addr.ip().to_string()
}

/// Decode a JWT bearer token's payload section and extract the `sub` claim
/// without performing cryptographic signature verification.
///
/// Signature verification is intentionally omitted: rate limiting is a
/// best-effort control that degrades gracefully — an invalid or forged JWT
/// simply returns `None`, falling back to IP-based limiting.  Verified
/// identity is handled by the auth middleware upstream.
fn extract_jwt_subject(authorization: &str) -> Option<String> {
    use base64::Engine as _;
    let token = authorization.strip_prefix("Bearer ")?;
    let payload_b64 = token.split('.').nth(1)?;
    let decoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json.get("sub").and_then(|v| v.as_str()).map(String::from)
}

/// Rate limiting middleware for GraphQL requests.
///
/// Decision order:
/// 1. Per-path limit (auth endpoints) — always checked, uses path-specific window.
/// 2. Per-user limit (authenticated requests) — checked when a JWT `sub` claim is
///    present in the `Authorization` header; authenticated users get `rps_per_user`
///    (default 10× `rps_per_ip`) instead of the shared IP bucket.
/// 3. Per-IP limit (unauthenticated or no bearer token) — fallback.
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

    let ip = extract_real_ip(&req, limiter.config().trust_proxy_headers, &addr);
    let path = req.uri().path().to_string();

    // Extract JWT subject for per-user limiting (no signature verification needed here).
    let user_id = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_jwt_subject);

    // ── Per-path limit (strictest, always enforced) ───────────────────────
    let path_result = limiter.check_path_limit(&path, &ip).await;
    if !path_result.allowed {
        warn!(ip = %ip, path = %path, "Per-path rate limit exceeded");
        return Err(RateLimitExceeded { retry_after_secs: path_result.retry_after_secs });
    }

    // ── Per-user or per-IP limit ──────────────────────────────────────────
    let limit_result = if let Some(ref uid) = user_id {
        // Authenticated: apply the higher per-user bucket.
        limiter.check_user_limit(uid).await
    } else {
        // Unauthenticated: apply the shared IP bucket.
        limiter.check_ip_limit(&ip).await
    };

    if !limit_result.allowed {
        if let Some(ref uid) = user_id {
            warn!(user_id = %uid, "Per-user rate limit exceeded");
        } else {
            warn!(ip = %ip, "IP rate limit exceeded");
        }
        return Err(RateLimitExceeded { retry_after_secs: limit_result.retry_after_secs });
    }

    let remaining = limit_result.remaining;

    let response = next.run(req).await;

    // Add rate limit headers
    let mut response = response;
    let limit = if user_id.is_some() {
        limiter.config().rps_per_user
    } else {
        limiter.config().rps_per_ip
    };
    if let Ok(limit_value) = format!("{limit}").parse() {
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
        // Fabricate a bucket whose last_refill is 200 ms in the past — no sleep needed.
        let mut bucket = TokenBucket {
            tokens:      0.0,
            capacity:    10.0,
            refill_rate: 5.0,
            last_refill: std::time::Instant::now()
                - std::time::Duration::from_millis(200),
        };
        // After 0.2 s at 5 tokens/s → 1 token refilled; should allow one consume.
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
        assert!(limiter.check_ip_limit("127.0.0.1").await.allowed);
        assert!(limiter.check_ip_limit("127.0.0.1").await.allowed);
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
        assert!(limiter.check_ip_limit("127.0.0.1").await.allowed);
        assert!(!limiter.check_ip_limit("127.0.0.1").await.allowed);
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
        assert!(limiter.check_ip_limit("127.0.0.1").await.allowed);
        assert!(limiter.check_ip_limit("127.0.0.1").await.allowed); // Should allow despite limit
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
        assert!(limiter.check_ip_limit("192.168.1.1").await.allowed);
        assert!(limiter.check_ip_limit("192.168.1.2").await.allowed);
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
        assert!(limiter.check_user_limit("user123").await.allowed);
        assert!(limiter.check_user_limit("user123").await.allowed);
        assert!(!limiter.check_user_limit("user123").await.allowed);
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
        // First check: bucket full — remaining should equal burst_size - 1
        let first = limiter.check_ip_limit("127.0.0.1").await;
        assert!(first.allowed);
        assert!(first.remaining < 10.0, "remaining should be 9 after first token consumed");

        let second = limiter.check_ip_limit("127.0.0.1").await;
        assert!(second.remaining < first.remaining, "remaining must decrease per request");
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
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await.allowed);
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await.allowed);
        assert!(limiter.check_path_limit("/graphql", "1.2.3.4").await.allowed);
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
        assert!(limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);
        // Second request: blocked (bucket empty)
        assert!(!limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);
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
        assert!(limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed);
        assert!(!limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed);
        // IP2 still gets its full allowance
        assert!(limiter.check_path_limit("/auth/start", "2.2.2.2").await.allowed);
    }

    #[tokio::test]
    async fn test_path_prefix_does_not_match_superset_paths() {
        // Regression: /auth/startover must NOT consume the /auth/start bucket (DoS vector).
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

        // Exhaust the /auth/start bucket for this IP.
        assert!(limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);
        assert!(!limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);

        // /auth/startover is not governed by the /auth/start rule — must be allowed.
        assert!(
            limiter.check_path_limit("/auth/startover", "1.2.3.4").await.allowed,
            "/auth/startover must not share the /auth/start bucket"
        );
        // /auth/start-session likewise.
        assert!(
            limiter.check_path_limit("/auth/start-session", "1.2.3.4").await.allowed,
            "/auth/start-session must not share the /auth/start bucket"
        );
        // /auth/start/extra (sub-path) should be governed by the rule.
        assert!(!limiter.check_path_limit("/auth/start/extra", "1.2.3.4").await.allowed,
            "/auth/start/extra SHOULD share the /auth/start bucket (sub-path)");
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

    // ─── retry_after_for_path tests ──────────────────────────────────────────

    #[test]
    fn test_retry_after_for_path_uses_path_window() {
        // 5 req per 60s → tokens_per_sec = 5/60 ≈ 0.083 → ceil(1/0.083) = 12s
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
        // Should be 12s (ceil(60/5) = 12), NOT 1s from the IP rate
        assert_eq!(limiter.retry_after_for_path("/auth/start"), 12);
    }

    #[test]
    fn test_retry_after_for_path_unknown_path_returns_one() {
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
        assert_eq!(limiter.retry_after_for_path("/graphql"), 1);
    }

    // ─── extract_jwt_subject tests ────────────────────────────────────────────

    #[test]
    fn test_extract_jwt_subject_returns_sub_claim() {
        use base64::Engine as _;
        // Build a minimal JWT payload with a sub claim
        let payload = serde_json::json!({"sub": "user-42", "exp": 9_999_999_999_u64});
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(payload.to_string().as_bytes());
        let token = format!("Bearer header.{b64}.sig");
        assert_eq!(extract_jwt_subject(&token), Some("user-42".to_string()));
    }

    #[test]
    fn test_extract_jwt_subject_no_bearer_prefix_returns_none() {
        assert_eq!(extract_jwt_subject("Basic dXNlcjpwYXNz"), None);
    }

    #[test]
    fn test_extract_jwt_subject_malformed_token_returns_none() {
        assert_eq!(extract_jwt_subject("Bearer notajwt"), None);
    }

    #[test]
    fn test_extract_jwt_subject_missing_sub_returns_none() {
        use base64::Engine as _;
        let payload = serde_json::json!({"iss": "provider", "exp": 9_999_999_999_u64});
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(payload.to_string().as_bytes());
        let token = format!("Bearer header.{b64}.sig");
        assert_eq!(extract_jwt_subject(&token), None);
    }

    // ─── extract_real_ip tests ────────────────────────────────────────────────

    #[test]
    fn test_extract_real_ip_without_proxy_returns_peer() {
        use axum::body::Body;
        use axum::http::Request;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let req = Request::builder().body(Body::empty()).unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
        assert_eq!(extract_real_ip(&req, false, &addr), "1.2.3.4");
    }

    #[test]
    fn test_extract_real_ip_with_proxy_prefers_x_real_ip() {
        use axum::body::Body;
        use axum::http::Request;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let req = Request::builder()
            .header("x-real-ip", "10.20.30.40")
            .header("x-forwarded-for", "5.5.5.5")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80);
        assert_eq!(extract_real_ip(&req, true, &addr), "10.20.30.40");
    }

    #[test]
    fn test_extract_real_ip_with_proxy_falls_back_to_xff() {
        use axum::body::Body;
        use axum::http::Request;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let req = Request::builder()
            .header("x-forwarded-for", "203.0.113.7, 10.0.0.1")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80);
        assert_eq!(extract_real_ip(&req, true, &addr), "203.0.113.7");
    }

    #[test]
    fn test_extract_real_ip_trust_disabled_ignores_headers() {
        use axum::body::Body;
        use axum::http::Request;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let req = Request::builder()
            .header("x-real-ip", "evil.attacker.ip")
            .header("x-forwarded-for", "6.6.6.6")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 5678);
        assert_eq!(extract_real_ip(&req, false, &addr), "1.2.3.4");
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
            trust_proxy_headers:   false,
        };
        let rl = RateLimiter::new_redis(&url, config).await.expect("Redis connection failed");
        // Use a unique key to avoid interference between test runs
        let ip = format!("test_allow:{}", uuid::Uuid::new_v4());
        for _ in 0..5 {
            assert!(
                rl.check_ip_limit(&ip).await.allowed,
                "should be allowed within capacity"
            );
        }
        assert!(!rl.check_ip_limit(&ip).await.allowed, "6th request should be rejected");
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
            trust_proxy_headers:   false,
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
        assert!(a.check_ip_limit(&ip).await.allowed);
        assert!(a.check_ip_limit(&ip).await.allowed);
        // Instance B consumes the 3rd token
        assert!(b.check_ip_limit(&ip).await.allowed);
        // 4th token across both instances should be rejected
        assert!(
            !b.check_ip_limit(&ip).await.allowed,
            "4th request should be rejected across instances"
        );
    }
}
