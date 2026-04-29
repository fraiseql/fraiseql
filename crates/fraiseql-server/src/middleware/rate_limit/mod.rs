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
mod dispatch;
mod in_memory;
mod key;
mod middleware_fn;
mod redis;
mod token_bucket;

pub use config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig};
pub use dispatch::RateLimiter;
pub use key::build_rate_limit_key;
pub use middleware_fn::{RateLimitExceeded, rate_limit_middleware};
// Re-export redis metrics for use by the metrics endpoint
#[cfg(feature = "redis-rate-limiting")]
pub use redis::{REDIS_RATE_LIMIT_ERRORS, redis_error_count_total};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::{
        middleware_fn::{extract_jwt_subject, extract_real_ip},
        *,
    };

    #[test]
    fn test_token_bucket_creation() {
        let bucket = token_bucket::TokenBucket::new(10.0, 5.0);
        assert!((bucket.tokens - 10.0).abs() < f64::EPSILON);
        assert!((bucket.capacity - 10.0).abs() < f64::EPSILON);
        assert!((bucket.refill_rate - 5.0).abs() < f64::EPSILON);
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
                .checked_sub(std::time::Duration::from_millis(200))
                .unwrap(),
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
            enabled: true,
            requests_per_second: 50,
            burst_size: 150,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(cfg.enabled);
        assert_eq!(cfg.rps_per_ip, 50);
        assert_eq!(cfg.burst_size, 150);
    }

    #[test]
    fn test_from_security_config_disabled() {
        let sec = RateLimitingSecurityConfig {
            enabled: false,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(!cfg.enabled);
    }

    #[test]
    fn test_from_security_config_user_limit_is_higher() {
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 100,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert!(cfg.rps_per_user > cfg.rps_per_ip);
    }

    #[test]
    fn test_from_security_config_defaults_per_user_to_10x() {
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 50,
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert_eq!(cfg.rps_per_user, 500); // 50 × 10 default
    }

    #[test]
    fn test_from_security_config_custom_per_user_rps_overrides_default() {
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 100,
            requests_per_second_per_user: Some(250),
            ..Default::default()
        };
        let cfg = RateLimitConfig::from_security_config(&sec);
        assert_eq!(cfg.rps_per_user, 250); // explicit value used
        assert_eq!(cfg.rps_per_ip, 100); // global unchanged
    }

    #[test]
    fn test_with_path_rules_generates_auth_start_rule() {
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 100,
            burst_size: 200,
            auth_start_max_requests: 5,
            auth_start_window_secs: 60,
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
            enabled: true,
            requests_per_second: 10,
            burst_size: 10,
            auth_start_max_requests: 1,
            auth_start_window_secs: 60,
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
            enabled: true,
            requests_per_second: 1000,
            burst_size: 1000,
            auth_start_max_requests: 1,
            auth_start_window_secs: 60,
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
            enabled: true,
            requests_per_second: 1000,
            burst_size: 1000,
            auth_start_max_requests: 1,
            auth_start_window_secs: 60,
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
            enabled: true,
            requests_per_second: 1000,
            burst_size: 1000,
            auth_start_max_requests: 1,
            auth_start_window_secs: 60,
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
        assert!(
            !limiter.check_path_limit("/auth/start/extra", "1.2.3.4").await.allowed,
            "/auth/start/extra SHOULD share the /auth/start bucket (sub-path)"
        );
    }

    // ─── retry_after_secs ────────────────────────────────────────────────────

    #[test]
    fn test_retry_after_secs_high_rps() {
        // 100 rps → 1 token per 0.01s → ceil = 1s
        let config = RateLimitConfig {
            rps_per_ip: 100,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_retry_after_secs_one_rps() {
        // 1 rps → 1 token per 1s → ceil = 1s
        let config = RateLimitConfig {
            rps_per_ip: 1,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_retry_after_secs_zero_rps_fallback() {
        // 0 rps (disabled / misconfigured) → safe fallback of 1s
        let config = RateLimitConfig {
            rps_per_ip: 0,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1);
    }

    #[test]
    fn test_rate_limit_exceeded_response_uses_config_retry_after() {
        use axum::response::IntoResponse;
        let resp = RateLimitExceeded {
            retry_after_secs: 5,
        }
        .into_response();
        let header = resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert_eq!(header, "5");
    }

    // ─── retry_after_for_path tests ──────────────────────────────────────────

    #[test]
    fn test_retry_after_for_path_uses_path_window() {
        // 5 req per 60s → tokens_per_sec = 5/60 ≈ 0.083 → ceil(1/0.083) = 12s
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 100,
            burst_size: 200,
            auth_start_max_requests: 5,
            auth_start_window_secs: 60,
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
            enabled: true,
            requests_per_second: 100,
            burst_size: 200,
            auth_start_max_requests: 5,
            auth_start_window_secs: 60,
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
        let b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
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
        let b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        let token = format!("Bearer header.{b64}.sig");
        assert_eq!(extract_jwt_subject(&token), None);
    }

    // ─── extract_real_ip tests ────────────────────────────────────────────────

    #[test]
    fn test_extract_real_ip_without_proxy_returns_peer() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        use axum::{body::Body, http::Request};
        let req = Request::builder().body(Body::empty()).unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
        assert_eq!(extract_real_ip(&req, false, &[], &addr), "1.2.3.4");
    }

    #[test]
    fn test_extract_real_ip_with_proxy_prefers_x_real_ip() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        use axum::{body::Body, http::Request};
        let req = Request::builder()
            .header("x-real-ip", "10.20.30.40")
            .header("x-forwarded-for", "5.5.5.5")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80);
        // Empty CIDRs: all proxies trusted
        assert_eq!(extract_real_ip(&req, true, &[], &addr), "10.20.30.40");
    }

    #[test]
    fn test_extract_real_ip_with_proxy_falls_back_to_xff() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        use axum::{body::Body, http::Request};
        let req = Request::builder()
            .header("x-forwarded-for", "203.0.113.7, 10.0.0.1")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80);
        // Empty CIDRs: all proxies trusted
        assert_eq!(extract_real_ip(&req, true, &[], &addr), "203.0.113.7");
    }

    #[test]
    fn test_extract_real_ip_trust_disabled_ignores_headers() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        use axum::{body::Body, http::Request};
        let req = Request::builder()
            .header("x-real-ip", "evil.attacker.ip")
            .header("x-forwarded-for", "6.6.6.6")
            .body(Body::empty())
            .unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 5678);
        assert_eq!(extract_real_ip(&req, false, &[], &addr), "1.2.3.4");
    }

    // ─── max_buckets cap tests (S34) ─────────────────────────────────────────

    #[tokio::test]
    async fn test_ip_bucket_cap_denies_new_ip_when_full() {
        // max_buckets=2: first two distinct IPs are tracked; third is denied
        let config = RateLimitConfig {
            enabled: true,
            rps_per_ip: 1_000,
            burst_size: 1_000,
            max_buckets: 2,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);

        // Fill the map with two IPs
        assert!(limiter.check_ip_limit("1.1.1.1").await.allowed, "first IP should be tracked");
        assert!(limiter.check_ip_limit("2.2.2.2").await.allowed, "second IP should be tracked");

        // Known IPs are still allowed even though the map is full
        assert!(
            limiter.check_ip_limit("1.1.1.1").await.allowed,
            "known IP must still pass after cap is reached"
        );

        // A brand-new IP is denied (cap exceeded)
        assert!(
            !limiter.check_ip_limit("3.3.3.3").await.allowed,
            "unseen IP must be denied when ip_buckets is at max_buckets"
        );
    }

    #[tokio::test]
    async fn test_user_bucket_cap_denies_new_user_when_full() {
        let config = RateLimitConfig {
            enabled: true,
            rps_per_user: 1_000,
            burst_size: 1_000,
            max_buckets: 2,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);

        assert!(limiter.check_user_limit("alice").await.allowed, "first user should be tracked");
        assert!(limiter.check_user_limit("bob").await.allowed, "second user should be tracked");

        // Existing user still allowed
        assert!(
            limiter.check_user_limit("alice").await.allowed,
            "known user must pass after cap"
        );

        // New user denied
        assert!(
            !limiter.check_user_limit("carol").await.allowed,
            "unseen user must be denied when user_buckets is at max_buckets"
        );
    }

    #[tokio::test]
    async fn test_path_ip_bucket_cap_denies_new_combination_when_full() {
        let sec = RateLimitingSecurityConfig {
            enabled: true,
            requests_per_second: 1_000,
            burst_size: 1_000,
            auth_start_max_requests: 100,
            auth_start_window_secs: 60,
            ..Default::default()
        };
        let config = RateLimitConfig {
            max_buckets: 1,
            ..RateLimitConfig::from_security_config(&sec)
        };
        let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);

        // First (path, IP) pair fills the map
        assert!(
            limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed,
            "first (path, ip) combination should be tracked"
        );

        // Same pair is still allowed (already in map)
        assert!(
            limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed,
            "known (path, ip) pair must still pass"
        );

        // New IP is denied — map is at capacity
        assert!(
            !limiter.check_path_limit("/auth/start", "2.2.2.2").await.allowed,
            "unseen (path, ip) combination must be denied when path_ip_buckets is at max_buckets"
        );
    }

    // ─── Redis integration tests ─────────────────────────────────────────────
    // These require a live Redis instance.  Run with:
    //   REDIS_URL=redis://localhost:6379 cargo test -p fraiseql-server \
    //     --features redis-rate-limiting -- redis_rate_limiter --ignored

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_rate_limiter_allows_up_to_capacity() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let config = RateLimitConfig {
            enabled:               true,
            rps_per_ip:            5,
            rps_per_user:          5,
            burst_size:            5,
            cleanup_interval_secs: 300,
            trust_proxy_headers:   false,
            trusted_proxy_cidrs:   Vec::new(),
            max_buckets:           100_000,
        };
        let rl = RateLimiter::new_redis(&url, config).await.expect("Redis connection failed");
        // Use a unique key to avoid interference between test runs
        let ip = format!("test_allow:{}", uuid::Uuid::new_v4());
        for _ in 0..5 {
            assert!(rl.check_ip_limit(&ip).await.allowed, "should be allowed within capacity");
        }
        assert!(!rl.check_ip_limit(&ip).await.allowed, "6th request should be rejected");
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_two_instances_share_bucket() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let config = RateLimitConfig {
            enabled:               true,
            rps_per_ip:            3,
            rps_per_user:          3,
            burst_size:            3,
            cleanup_interval_secs: 300,
            trust_proxy_headers:   false,
            trusted_proxy_cidrs:   Vec::new(),
            max_buckets:           100_000,
        };
        let suffix = uuid::Uuid::new_v4();
        let a = RateLimiter::new_redis(&url, config.clone())
            .await
            .expect("Redis connection failed");
        let b = RateLimiter::new_redis(&url, config).await.expect("Redis connection failed");
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
