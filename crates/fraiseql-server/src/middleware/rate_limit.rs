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
    /// Maps `requests_per_second` → `rps_per_ip` / `rps_per_user` and `burst_size` directly.
    pub fn from_security_config(sec: &RateLimitingSecurityConfig) -> Self {
        Self {
            enabled:               sec.enabled,
            rps_per_ip:            sec.requests_per_second,
            rps_per_user:          sec.requests_per_second.saturating_mul(10),
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

/// Rate limiter state tracker.
pub struct RateLimiter {
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

impl RateLimiter {
    /// Create new rate limiter.
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
}

/// Rate limit middleware response.
#[derive(Debug)]
pub struct RateLimitExceeded;

impl IntoResponse for RateLimitExceeded {
    fn into_response(self) -> Response {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("Content-Type", "application/json"), ("Retry-After", "60")],
            r#"{"errors":[{"message":"Rate limit exceeded. Please retry after 60 seconds."}]}"#,
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
        return Err(RateLimitExceeded);
    }

    // Check per-path rate limit (e.g., /auth/start stricter than global)
    if !limiter.check_path_limit(&path, &ip).await {
        warn!(ip = %ip, path = %path, "Per-path rate limit exceeded");
        return Err(RateLimitExceeded);
    }

    // Get remaining tokens for response headers
    let remaining = limiter.get_ip_remaining(&ip).await;

    let response = next.run(req).await;

    // Add rate limit headers
    let mut response = response;
    if let Ok(limit_value) = format!("{}", limiter.config.rps_per_ip).parse() {
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
        // Verify a path rule was registered (indirectly, by checking path limiting works)
        assert_eq!(limiter.path_rules.len(), 1);
        assert_eq!(limiter.path_rules[0].path_prefix, "/auth/start");
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
}
