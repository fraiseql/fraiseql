//! Rate limiting middleware for GraphQL requests.
//!
//! Implements request rate limiting with:
//! - Per-IP rate limiting
//! - Per-user rate limiting (if authenticated)
//! - Per-path rate limiting (for auth endpoints)
//! - Token bucket algorithm
//! - Configurable burst capacity
//! - X-RateLimit headers

mod config;
mod in_memory;
mod key;
mod redis;
mod token_bucket;

pub use config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig};
pub use key::build_rate_limit_key;

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

use self::config::RateLimitConfig as _RateLimitConfig;
use self::in_memory::InMemoryRateLimiter;
use self::key::is_private_or_loopback;
#[cfg(feature = "redis-rate-limiting")]
use self::redis::RedisRateLimiter;

// Re-export redis metrics for use by the metrics endpoint
#[cfg(feature = "redis-rate-limiting")]
pub use redis::{REDIS_RATE_LIMIT_ERRORS, redis_error_count_total};

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
    pub const fn config(&self) -> &RateLimitConfig {
        match self {
            Self::InMemory(rl) => rl.config(),
            #[cfg(feature = "redis-rate-limiting")]
            Self::Redis(rl) => rl.config(),
        }
    }

    /// Number of per-path rate limit rules registered.
    pub const fn path_rule_count(&self) -> usize {
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
///
/// # Errors
///
/// Returns `RateLimitExceeded` if the per-path, per-user, or per-IP rate limit is exceeded.
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
        .unwrap_or_else(|| Arc::new(RateLimiter::new(_RateLimitConfig::default())));

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
        let bucket = token_bucket::TokenBucket::new(10.0, 5.0);
        assert_eq!(bucket.tokens, 10.0);
        assert_eq!(bucket.capacity, 10.0);
        assert_eq!(bucket.refill_rate, 5.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = token_bucket::TokenBucket::new(10.0, 5.0);
        assert!(bucket.try_consume(5.0));
        assert!((bucket.tokens - 5.0).abs() < 0.001); // Allow float precision
        assert!(bucket.try_consume(5.0));
        assert!(bucket.tokens.abs() < 0.001); // Allow float precision
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        // Fabricate a bucket whose last_refill is 200 ms in the past — no sleep needed.
        let mut bucket = token_bucket::TokenBucket {
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

    // --- from_security_config() tests ---

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
