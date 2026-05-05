//! Tests for `rate_limit/` modules.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience
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

// ── token_bucket_tests ──────────────────────────────────────────────────────

mod token_bucket_tests {
    #![allow(clippy::unwrap_used)]

    use std::time::{Duration, Instant};

    use super::super::token_bucket::TokenBucket;

    #[test]
    fn new_bucket_starts_at_capacity() {
        let bucket = TokenBucket::new(100.0, 10.0);
        assert!((bucket.tokens - 100.0).abs() < f64::EPSILON);
        assert!((bucket.capacity - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn consume_more_than_available_fails() {
        let mut bucket = TokenBucket::new(3.0, 1.0);
        assert!(!bucket.try_consume(4.0), "consuming more than capacity must fail");
    }

    #[test]
    fn token_count_never_exceeds_capacity() {
        let bucket = TokenBucket {
            tokens:      50.0,
            capacity:    100.0,
            refill_rate: 1000.0,
            last_refill: Instant::now().checked_sub(Duration::from_secs(1000)).unwrap(),
        };
        assert!(bucket.token_count() <= 100.0, "token_count must never exceed capacity");
    }

    #[test]
    fn refill_restores_tokens_after_idle_period() {
        let mut bucket = TokenBucket {
            tokens:      0.0,
            capacity:    10.0,
            refill_rate: 100.0, // 100 tokens/sec
            last_refill: Instant::now().checked_sub(Duration::from_millis(100)).unwrap(),
        };
        assert!(bucket.try_consume(1.0), "refilled bucket must allow consumption");
    }

    #[test]
    fn zero_refill_rate_never_refills() {
        let mut bucket = TokenBucket {
            tokens:      0.0,
            capacity:    10.0,
            refill_rate: 0.0,
            last_refill: Instant::now().checked_sub(Duration::from_secs(60)).unwrap(),
        };
        assert!(!bucket.try_consume(1.0), "zero refill rate means no refill ever");
    }

    #[test]
    fn fractional_consume_works() {
        let mut bucket = TokenBucket::new(1.0, 0.0);
        assert!(bucket.try_consume(0.5));
        assert!(bucket.try_consume(0.5));
        assert!(!bucket.try_consume(0.1));
    }
}

// ── dispatch_tests ──────────────────────────────────────────────────────────

mod dispatch_tests {
    use super::super::{RateLimitConfig, dispatch::RateLimiter};

    #[test]
    fn new_creates_in_memory_backend() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert!(matches!(limiter, RateLimiter::InMemory(_)));
    }

    #[test]
    fn config_returns_reference_to_inner_config() {
        let config = RateLimitConfig {
            rps_per_ip: 42,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.config().rps_per_ip, 42);
    }

    #[test]
    fn path_rule_count_starts_at_zero() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        assert_eq!(limiter.path_rule_count(), 0);
    }

    #[test]
    fn retry_after_secs_minimum_is_one() {
        let config = RateLimitConfig {
            rps_per_ip: u32::MAX,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.retry_after_secs(), 1, "minimum retry_after must be 1s");
    }
}

// ── key_tests ───────────────────────────────────────────────────────────────

mod key_tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::super::key::{build_rate_limit_key, is_private_or_loopback, path_matches_rule};

    #[test]
    fn key_without_prefix() {
        let key = build_rate_limit_key("ip", "1.2.3.4", None);
        assert_eq!(key, "fraiseql:rl:ip:1.2.3.4");
    }

    #[test]
    fn key_with_prefix() {
        let key = build_rate_limit_key("path", "1.2.3.4", Some("/auth/start"));
        assert_eq!(key, "fraiseql:rl:path:/auth/start:1.2.3.4");
    }

    #[test]
    fn loopback_ipv4_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn rfc1918_10_x_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn rfc1918_172_16_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn rfc1918_192_168_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn link_local_is_private() {
        assert!(is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn public_ipv4_is_not_private() {
        assert!(!is_private_or_loopback(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn loopback_ipv6_is_private() {
        assert!(is_private_or_loopback(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn public_ipv6_is_not_private() {
        assert!(!is_private_or_loopback(IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))));
    }

    #[test]
    fn exact_match() {
        assert!(path_matches_rule("/auth/start", "/auth/start"));
    }

    #[test]
    fn sub_path_matches() {
        assert!(path_matches_rule("/auth/start/extra", "/auth/start"));
    }

    #[test]
    fn query_string_matches() {
        assert!(path_matches_rule("/auth/start?code=abc", "/auth/start"));
    }

    #[test]
    fn superset_does_not_match() {
        assert!(!path_matches_rule("/auth/startover", "/auth/start"));
    }

    #[test]
    fn hyphenated_suffix_does_not_match() {
        assert!(!path_matches_rule("/auth/start-session", "/auth/start"));
    }

    #[test]
    fn completely_different_path_does_not_match() {
        assert!(!path_matches_rule("/graphql", "/auth/start"));
    }

    #[test]
    fn empty_path_does_not_match_prefix() {
        assert!(!path_matches_rule("", "/auth/start"));
    }
}

// ── middleware_fn_tests ──────────────────────────────────────────────────────

mod middleware_fn_tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use axum::{body::Body, http::Request};

    use super::super::middleware_fn::extract_real_ip;

    fn socket_addr(ip: [u8; 4]) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), 12345)
    }

    fn req_with_xff(xff: &str) -> Request<Body> {
        Request::builder()
            .uri("http://example.com/graphql")
            .header("x-forwarded-for", xff)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn test_spoofed_xforwardedfor_ignored_when_direct_ip_not_in_trusted_cidrs() {
        let cidrs: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let addr = socket_addr([203, 0, 113, 1]);
        let req = req_with_xff("1.2.3.4");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "203.0.113.1", "Should use direct IP, not spoofed X-Forwarded-For");
    }

    #[test]
    fn test_forwarded_ip_used_when_direct_ip_is_trusted_proxy() {
        let cidrs: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let addr = socket_addr([10, 0, 1, 5]);
        let req = req_with_xff("5.6.7.8");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "5.6.7.8", "Should use X-Forwarded-For from trusted proxy");
    }

    #[test]
    fn test_no_cidrs_trusts_all_proxies() {
        let cidrs: Vec<ipnet::IpNet> = vec![];
        let addr = socket_addr([203, 0, 113, 1]);
        let req = req_with_xff("9.9.9.9");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "9.9.9.9", "Empty CIDRs: all proxies trusted");
    }
}

// ── mod_tests (rate_limit module-level tests) ────────────────────────────────

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
    assert!((bucket.tokens - 5.0).abs() < 0.001);
    assert!(bucket.try_consume(5.0));
    assert!(bucket.tokens.abs() < 0.001);
    assert!(!bucket.try_consume(1.0));
}

#[test]
fn test_token_bucket_refill() {
    let mut bucket = token_bucket::TokenBucket {
        tokens:      0.0,
        capacity:    10.0,
        refill_rate: 5.0,
        last_refill: std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(200))
            .unwrap(),
    };
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
    assert!(limiter.check_ip_limit("127.0.0.1", None).await.allowed);
    assert!(limiter.check_ip_limit("127.0.0.1", None).await.allowed);
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
    assert!(limiter.check_ip_limit("127.0.0.1", None).await.allowed);
    assert!(!limiter.check_ip_limit("127.0.0.1", None).await.allowed);
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
    assert!(limiter.check_ip_limit("127.0.0.1", None).await.allowed);
    assert!(limiter.check_ip_limit("127.0.0.1", None).await.allowed);
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
    assert!(limiter.check_ip_limit("192.168.1.1", None).await.allowed);
    assert!(limiter.check_ip_limit("192.168.1.2", None).await.allowed);
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
    assert!(limiter.check_user_limit("user123", None).await.allowed);
    assert!(limiter.check_user_limit("user123", None).await.allowed);
    assert!(!limiter.check_user_limit("user123", None).await.allowed);
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
    let first = limiter.check_ip_limit("127.0.0.1", None).await;
    assert!(first.allowed);
    assert!(first.remaining < 10.0, "remaining should be 9 after first token consumed");

    let second = limiter.check_ip_limit("127.0.0.1", None).await;
    assert!(second.remaining < first.remaining, "remaining must decrease per request");
}

#[tokio::test]
async fn test_rate_limiter_cleanup() {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);

    limiter.check_ip_limit("127.0.0.1", None).await;
    limiter.cleanup().await;
}

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
    assert_eq!(cfg.rps_per_user, 500);
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
    assert_eq!(cfg.rps_per_user, 250);
    assert_eq!(cfg.rps_per_ip, 100);
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
    assert!(limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);
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
    assert!(limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed);
    assert!(!limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed);
    assert!(limiter.check_path_limit("/auth/start", "2.2.2.2").await.allowed);
}

#[tokio::test]
async fn test_path_prefix_does_not_match_superset_paths() {
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

    assert!(limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);
    assert!(!limiter.check_path_limit("/auth/start", "1.2.3.4").await.allowed);

    assert!(
        limiter.check_path_limit("/auth/startover", "1.2.3.4").await.allowed,
        "/auth/startover must not share the /auth/start bucket"
    );
    assert!(
        limiter.check_path_limit("/auth/start-session", "1.2.3.4").await.allowed,
        "/auth/start-session must not share the /auth/start bucket"
    );
    assert!(
        !limiter.check_path_limit("/auth/start/extra", "1.2.3.4").await.allowed,
        "/auth/start/extra SHOULD share the /auth/start bucket (sub-path)"
    );
}

#[test]
fn test_retry_after_secs_high_rps() {
    let config = RateLimitConfig {
        rps_per_ip: 100,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);
    assert_eq!(limiter.retry_after_secs(), 1);
}

#[test]
fn test_retry_after_secs_one_rps() {
    let config = RateLimitConfig {
        rps_per_ip: 1,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);
    assert_eq!(limiter.retry_after_secs(), 1);
}

#[test]
fn test_retry_after_secs_zero_rps_fallback() {
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

#[test]
fn test_retry_after_for_path_uses_path_window() {
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

#[test]
fn test_extract_jwt_subject_returns_sub_claim() {
    use base64::Engine as _;
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

#[tokio::test]
async fn test_ip_bucket_cap_denies_new_ip_when_full() {
    let config = RateLimitConfig {
        enabled: true,
        rps_per_ip: 1_000,
        burst_size: 1_000,
        max_buckets: 2,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);

    assert!(limiter.check_ip_limit("1.1.1.1", None).await.allowed, "first IP should be tracked");
    assert!(limiter.check_ip_limit("2.2.2.2", None).await.allowed, "second IP should be tracked");

    assert!(
        limiter.check_ip_limit("1.1.1.1", None).await.allowed,
        "known IP must still pass after cap is reached"
    );

    assert!(
        !limiter.check_ip_limit("3.3.3.3", None).await.allowed,
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

    assert!(limiter.check_user_limit("alice", None).await.allowed, "first user should be tracked");
    assert!(limiter.check_user_limit("bob", None).await.allowed, "second user should be tracked");

    assert!(
        limiter.check_user_limit("alice", None).await.allowed,
        "known user must pass after cap"
    );

    assert!(
        !limiter.check_user_limit("carol", None).await.allowed,
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

    assert!(
        limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed,
        "first (path, ip) combination should be tracked"
    );

    assert!(
        limiter.check_path_limit("/auth/start", "1.1.1.1").await.allowed,
        "known (path, ip) pair must still pass"
    );

    assert!(
        !limiter.check_path_limit("/auth/start", "2.2.2.2").await.allowed,
        "unseen (path, ip) combination must be denied when path_ip_buckets is at max_buckets"
    );
}

#[tokio::test]
async fn test_tenant_rate_limit_allows_within_burst() {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);

    for _ in 0..5 {
        assert!(
            limiter.check_tenant_limit("tenant-abc", 5, 5).await.allowed,
            "should allow within burst"
        );
    }
    assert!(
        !limiter.check_tenant_limit("tenant-abc", 5, 5).await.allowed,
        "should deny when burst exhausted"
    );
}

#[tokio::test]
async fn test_tenant_rate_limit_independent_buckets() {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);

    assert!(limiter.check_tenant_limit("tenant-a", 1, 1).await.allowed);
    assert!(!limiter.check_tenant_limit("tenant-a", 1, 1).await.allowed);

    assert!(limiter.check_tenant_limit("tenant-b", 1, 1).await.allowed);
}

#[tokio::test]
async fn test_tenant_rate_limit_cleanup_does_not_panic() {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);

    limiter.check_tenant_limit("tenant-abc", 10, 10).await;
    limiter.cleanup().await;
}

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
    let ip = format!("test_allow:{}", uuid::Uuid::new_v4());
    for _ in 0..5 {
        assert!(rl.check_ip_limit(&ip, None).await.allowed, "should be allowed within capacity");
    }
    assert!(!rl.check_ip_limit(&ip, None).await.allowed, "6th request should be rejected");
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

    assert!(a.check_ip_limit(&ip, None).await.allowed);
    assert!(a.check_ip_limit(&ip, None).await.allowed);
    assert!(b.check_ip_limit(&ip, None).await.allowed);
    assert!(
        !b.check_ip_limit(&ip, None).await.allowed,
        "4th request should be rejected across instances"
    );
}
