//! Rate limiter enum dispatch — routes calls to the active backend.
//!
//! `RateLimiter` is the public handle used by the rest of the server.
//! It wraps either the in-memory or the Redis backend behind a uniform
//! async API so callers never need to know which backend is active.

#[cfg(feature = "redis-rate-limiting")]
use super::redis::RedisRateLimiter;
use super::{
    config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig},
    in_memory::InMemoryRateLimiter,
};

/// Rate limiter that dispatches to either an in-memory or Redis backend.
///
/// Construct via [`RateLimiter::new`] (in-memory, default) or
/// `RateLimiter::new_redis` (distributed Redis, requires the
/// `redis-rate-limiting` Cargo feature).
#[allow(private_interfaces)] // Reason: RedisRateLimiter is intentionally pub(super) — internal backend, public enum
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
    pub const fn config(&self) -> &RateLimitConfig {
        match self {
            Self::InMemory(rl) => rl.config(),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.config(),
        }
    }

    /// Number of per-path rate limit rules registered.
    #[allow(clippy::missing_const_for_fn)] // Reason: non-const when `redis-rate-limiting` feature is enabled
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
