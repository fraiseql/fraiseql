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
#![allow(clippy::panic)] // Reason: a test helper that cannot answer must fail loudly, not return a plausible 0

use super::{middleware_fn::extract_real_ip, *};

/// Live bucket counts, for the eviction assertions below (#1080).
///
/// The maps are `pub(super)` on `InMemoryRateLimiter`, and this module sits inside
/// `rate_limit`, so the sweep's effect can be observed directly rather than inferred from
/// behaviour. A Redis limiter has no local map — these are only meaningful in-memory, so
/// they panic rather than silently answering 0 and letting a test pass on the wrong backend.
fn ip_bucket_count(limiter: &RateLimiter) -> usize {
    match limiter {
        RateLimiter::InMemory(rl) => rl.ip_buckets.len(),
        #[cfg(feature = "redis-rate-limiting")]
        RateLimiter::Redis(_) => panic!("bucket counts are in-memory only"),
    }
}

fn tenant_bucket_count(limiter: &RateLimiter) -> usize {
    match limiter {
        RateLimiter::InMemory(rl) => rl.tenant_buckets.len(),
        #[cfg(feature = "redis-rate-limiting")]
        RateLimiter::Redis(_) => panic!("bucket counts are in-memory only"),
    }
}

// ── token_bucket_tests ──────────────────────────────────────────────────────

mod token_bucket_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
            last_refill: Instant::now().checked_sub(Duration::from_mins(1)).unwrap(),
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

    // ── #368/#788: one shared rule builder for both backends ────────────────

    #[test]
    fn security_path_rules_cover_the_social_v1_endpoints() {
        use super::super::{config::RateLimitingSecurityConfig, key::PathRateLimit};

        let sec = RateLimitingSecurityConfig {
            auth_start_max_requests: 10,
            auth_start_window_secs: 60,
            auth_callback_max_requests: 20,
            auth_callback_window_secs: 60,
            // Explicit zeros: a group set to 0 is disabled. (The type's Default
            // carries protective non-zero budgets since #977 — the producer's
            // defaults — so "unset" no longer implies "no rule".)
            auth_refresh_max_requests: 0,
            auth_refresh_window_secs: 0,
            auth_logout_max_requests: 0,
            auth_logout_window_secs: 0,
            ..RateLimitingSecurityConfig::default()
        };
        let rules = PathRateLimit::rules_from_security(&sec);
        let burst_of =
            |prefix: &str| rules.iter().find(|r| r.path_prefix == prefix).map(|r| r.burst as u32);
        // The PKCE endpoints keep their rules…
        assert_eq!(burst_of("/auth/start"), Some(10), "{rules:?}");
        assert_eq!(burst_of("/auth/callback"), Some(20), "{rules:?}");
        // …and the social endpoints (#368) are governed by the same settings —
        // an authorize flood fills the bounded CSRF state store (#788/H25), so
        // it faces the auth_start budget; the callback the callback budget.
        assert_eq!(burst_of("/auth/v1/authorize"), Some(10), "{rules:?}");
        assert_eq!(burst_of("/auth/v1/callback"), Some(20), "{rules:?}");
        // Groups explicitly set to zero build no rule.
        assert_eq!(burst_of("/auth/refresh"), None);
        assert_eq!(burst_of("/auth/logout"), None);
    }

    /// The unified `Default` (#977) is the producer's: every auth group carries
    /// a protective non-zero budget, so a hand-authored `{"enabled": true}`
    /// section throttles the auth endpoints instead of leaving them unlimited.
    #[test]
    fn default_security_config_builds_protective_auth_rules() {
        use super::super::{config::RateLimitingSecurityConfig, key::PathRateLimit};

        let rules = PathRateLimit::rules_from_security(&RateLimitingSecurityConfig::default());
        for prefix in [
            "/auth/start",
            "/auth/callback",
            "/auth/refresh",
            "/auth/logout",
        ] {
            assert!(
                rules.iter().any(|r| r.path_prefix == prefix),
                "the default config must build a rule for {prefix}; got {rules:?}"
            );
        }
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
    assert!(limiter.check_ip_limit("127.0.0.1").await.allowed);
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
    let first = limiter.check_ip_limit("127.0.0.1").await;
    assert!(first.allowed);
    assert!(first.remaining < 10.0, "remaining should be 9 after first token consumed");

    let second = limiter.check_ip_limit("127.0.0.1").await;
    assert!(second.remaining < first.remaining, "remaining must decrease per request");
}

/// `cleanup()` must actually evict, not merely return.
///
/// This test used to call `cleanup()` and assert nothing at all — it would have passed
/// against a `cleanup()` with an empty body, which is very nearly the state the server
/// shipped in: the function was correct and had no caller (#1080). A sweep is only worth
/// spawning if it removes something, so the assertion is on the map.
///
/// Staleness threshold is `burst_size / rps_per_ip` seconds, so 1/1000 here — a bucket is
/// stale almost immediately, and the sleep only has to outlast a millisecond.
#[tokio::test]
async fn cleanup_evicts_a_stale_ip_bucket() {
    let config = RateLimitConfig {
        rps_per_ip: 1000,
        burst_size: 1,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);

    limiter.check_ip_limit("127.0.0.1").await;
    assert_eq!(ip_bucket_count(&limiter), 1, "precondition: the request must mint a bucket");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    limiter.cleanup().await;

    assert_eq!(
        ip_bucket_count(&limiter),
        0,
        "cleanup must evict a bucket idle for longer than its refill window"
    );
}

/// The counterweight: a bucket that is NOT stale must survive.
///
/// Without this, a `cleanup()` that simply cleared every map would satisfy the test above
/// — and would silently reset every client's budget on each sweep.
#[tokio::test]
async fn cleanup_keeps_a_fresh_ip_bucket() {
    let config = RateLimitConfig {
        rps_per_ip: 1,
        burst_size: 600,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);

    limiter.check_ip_limit("127.0.0.1").await;
    limiter.cleanup().await;

    assert_eq!(
        ip_bucket_count(&limiter),
        1,
        "a bucket well inside its 600s refill window must not be evicted"
    );
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
        // The other groups are explicitly disabled: since #977 the Default
        // carries non-zero budgets, so "unset" would build their rules too.
        auth_callback_max_requests: 0,
        auth_callback_window_secs: 0,
        auth_refresh_max_requests: 0,
        auth_refresh_window_secs: 0,
        auth_logout_max_requests: 0,
        auth_logout_window_secs: 0,
        ..Default::default()
    };
    let config = RateLimitConfig::from_security_config(&sec);
    let limiter = RateLimiter::new(config).with_path_rules_from_security(&sec);
    // auth_start governs both the PKCE /auth/start and the social
    // /auth/v1/authorize (#368) — one setting, two rules.
    assert_eq!(limiter.path_rule_count(), 2);
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
async fn test_ip_bucket_cap_evicts_rather_than_denying_when_full() {
    let config = RateLimitConfig {
        enabled: true,
        rps_per_ip: 1_000,
        burst_size: 1_000,
        max_buckets: 2,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);

    assert!(limiter.check_ip_limit("1.1.1.1").await.allowed, "first IP should be tracked");
    assert!(limiter.check_ip_limit("2.2.2.2").await.allowed, "second IP should be tracked");

    assert!(
        limiter.check_ip_limit("1.1.1.1").await.allowed,
        "known IP must still pass after cap is reached"
    );

    // #1143 inverted this deliberately. It used to assert the unseen IP was DENIED —
    // pinning a denial of service against strangers as intended behaviour. A full map
    // now evicts the least-recently-used of a sample instead: accuracy degrades,
    // availability does not.
    assert!(
        limiter.check_ip_limit("3.3.3.3").await.allowed,
        "an unseen IP must be SERVED when ip_buckets is full; the cap is a memory \
         ceiling, not a licence to refuse strangers"
    );
}

#[tokio::test]
async fn test_user_bucket_cap_evicts_rather_than_denying_when_full() {
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

    assert!(
        limiter.check_user_limit("alice").await.allowed,
        "known user must pass after cap"
    );

    // #1143: inverted with the IP case above, and for the same reason.
    assert!(
        limiter.check_user_limit("carol").await.allowed,
        "an unseen user must be SERVED when user_buckets is full"
    );
}

#[tokio::test]
async fn test_path_ip_bucket_cap_evicts_rather_than_denying_when_full() {
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

    // #1143: inverted with the two cases above. This one matters most — the per-path
    // buckets guard the auth endpoints, so refusing an unseen (path, ip) pair locks
    // out precisely the logins and registrations the limit exists to protect.
    assert!(
        limiter.check_path_limit("/auth/start", "2.2.2.2").await.allowed,
        "an unseen (path, ip) combination must be SERVED when path_ip_buckets is full"
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

/// Tenant buckets are swept on the IP threshold too — asserted, not assumed.
///
/// The previous version of this test was named `..._does_not_panic` and did exactly that:
/// it called `cleanup()` and checked nothing. Honest about its own weakness, but it left
/// the tenant map's participation in the sweep unpinned.
#[tokio::test]
async fn cleanup_evicts_a_stale_tenant_bucket() {
    let config = RateLimitConfig {
        rps_per_ip: 1000,
        burst_size: 1,
        ..RateLimitConfig::default()
    };
    let limiter = RateLimiter::new(config);

    limiter.check_tenant_limit("tenant-abc", 10, 10).await;
    assert_eq!(tenant_bucket_count(&limiter), 1, "precondition: the check must mint a bucket");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    limiter.cleanup().await;

    assert_eq!(
        tenant_bucket_count(&limiter),
        0,
        "cleanup must evict a stale tenant bucket, not only IP and user buckets"
    );
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
#[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
async fn test_redis_rate_limiter_allows_up_to_capacity() {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
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
        assert!(rl.check_ip_limit(&ip).await.allowed, "should be allowed within capacity");
    }
    assert!(!rl.check_ip_limit(&ip).await.allowed, "6th request should be rejected");
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
#[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
async fn test_redis_two_instances_share_bucket() {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
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

    assert!(a.check_ip_limit(&ip).await.allowed);
    assert!(a.check_ip_limit(&ip).await.allowed);
    assert!(b.check_ip_limit(&ip).await.allowed);
    assert!(
        !b.check_ip_limit(&ip).await.allowed,
        "4th request should be rejected across instances"
    );
}

// ---------------------------------------------------------------------------
// #1143: the bucket key must be derivable only from facts a client cannot forge
// ---------------------------------------------------------------------------

/// A rate limiter's key space must be bounded by something the caller cannot inflate.
/// That is the whole property — capacity caps and eviction are compensation for its
/// absence, not substitutes for it.
///
/// **Two of the three vectors are closed structurally rather than by these tests, and
/// deliberately so.** `check_ip_limit` and `check_user_limit` no longer *accept* a
/// tenant, and the HTTP middleware no longer reads a JWT `sub` it cannot verify, so a
/// test reproducing those attacks would not compile. Making an illegal state
/// unrepresentable beats asserting it does not arise. For the record, measured before
/// the change: **50 of 50 requests allowed** against `rps_per_ip = 1, burst = 1`, from
/// one IP, unauthenticated, by varying `X-Tenant-ID` — the limit did not limit.
///
/// What remains variable is the address itself, so that is what is tested here.
mod bucket_keys_are_not_attacker_controlled {
    use std::net::{IpAddr, SocketAddr};

    use super::*;
    use crate::middleware::rate_limit::key::normalise_ip_key;

    /// A proxy forwards `X-Forwarded-For`; it does not validate it. Returning the raw
    /// string made every distinct value a distinct bucket — the `X-Tenant-ID`
    /// amplification one header over, and reachable by anyone behind a trusted proxy.
    #[test]
    fn an_unparseable_forwarded_value_is_not_an_address() {
        for junk in [
            "not-an-ip",
            "",
            "   ",
            "1.2.3",
            "tenant-42",
            "::gg",
            "1.2.3.4.5",
        ] {
            assert_eq!(normalise_ip_key(junk), None, "{junk:?} must not become a key");
        }
    }

    /// A single routine `IPv6` allocation *is* a /64, so keying on the full /128 would
    /// let one ordinary customer mint 2^64 buckets. Bounding the tenant while leaving
    /// this open would close the front door and leave the side door ajar.
    #[test]
    fn ipv6_addresses_in_one_allocation_share_one_key() {
        let a = normalise_ip_key("2001:db8:1:2::1").unwrap();
        let b = normalise_ip_key("2001:db8:1:2:ffff:ffff:ffff:ffff").unwrap();
        assert_eq!(a, b, "a /64 is one key");
        assert_eq!(a, "2001:db8:1:2::/64");

        // A different /64 is a different client, and must remain distinguishable.
        let other = normalise_ip_key("2001:db8:1:3::1").unwrap();
        assert_ne!(a, other, "distinct allocations must not collapse together");
    }

    /// `IPv4` is keyed whole — a /32 is one host, and collapsing it would merge
    /// unrelated clients into one bucket.
    #[test]
    fn ipv4_is_keyed_whole() {
        assert_eq!(normalise_ip_key("203.0.113.7").unwrap(), "203.0.113.7");
        assert_ne!(
            normalise_ip_key("203.0.113.7").unwrap(),
            normalise_ip_key("203.0.113.8").unwrap()
        );
    }

    /// The end-to-end consequence: one `IPv6` customer varying the low 64 bits gets one
    /// bucket, so the limit holds. Without the /64 collapse this loop would allow
    /// every request and leave 200 buckets behind.
    #[tokio::test]
    async fn one_ipv6_allocation_cannot_mint_a_fresh_budget() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled: true,
            rps_per_ip: 1,
            burst_size: 1,
            ..Default::default()
        });

        let mut allowed = 0;
        for i in 0..200 {
            let addr: SocketAddr = format!("[2001:db8:1:2::{i:x}]:443").parse().unwrap();
            let ip: IpAddr = addr.ip();
            let key = normalise_ip_key(&ip.to_string()).unwrap();
            if limiter.check_ip_limit(&key).await.allowed {
                allowed += 1;
            }
        }

        assert_eq!(allowed, 1, "one /64, burst of 1: got {allowed}/200");
        assert_eq!(ip_bucket_count(&limiter), 1, "and it occupies one bucket");
    }
}

/// #1143 direction 3: a full bucket map degrades accuracy, never availability.
mod a_full_map_does_not_refuse_strangers {
    use super::*;

    /// Denying an unseen key is a denial of service against strangers — and
    /// `ip_buckets` only grows on requests that have no bucket yet, so the strangers
    /// are exactly the unauthenticated ones: every login and registration attempt.
    ///
    /// #1080 made a full map recoverable rather than permanent. This asserts the
    /// stronger property: it is never fatal in the first place.
    #[tokio::test]
    async fn a_new_client_is_served_when_the_map_is_full() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled: true,
            rps_per_ip: 100,
            burst_size: 100,
            max_buckets: 32,
            ..Default::default()
        });

        // Fill well past capacity with distinct clients.
        for i in 0..200 {
            let ip = format!("203.0.113.{}", i % 256);
            let _ = limiter.check_ip_limit(&ip).await;
        }

        // A client never seen before must still be served.
        let stranger = limiter.check_ip_limit("198.51.100.42").await;
        assert!(
            stranger.allowed,
            "a full map must not refuse an unseen client — that is a DoS against \
             exactly the unauthenticated callers"
        );

        // …and the map stays bounded. Best-effort under concurrency, so allow slack.
        let live = ip_bucket_count(&limiter);
        assert!(live <= 64, "map must stay bounded near max_buckets=32, saw {live}");
    }

    /// #1080's property, restated so it cannot regress: a client denied on a full map
    /// becomes servable again without a restart.
    #[tokio::test]
    async fn the_limiter_still_enforces_after_eviction_pressure() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled: true,
            rps_per_ip: 1,
            burst_size: 1,
            max_buckets: 8,
            ..Default::default()
        });

        // Its own bucket is fresh, so the first passes and the second does not —
        // eviction must not have turned the limiter into a no-op.
        assert!(limiter.check_ip_limit("198.51.100.7").await.allowed);
        assert!(
            !limiter.check_ip_limit("198.51.100.7").await.allowed,
            "eviction must degrade accuracy under pressure, not disable enforcement"
        );
    }
}

// ── #1171 part 1: max_buckets is an operator knob ────────────────────────────
//
// It was hardcoded in `assemble`, and `RateLimitingSecurityConfig` had no such field
// at all — so the memory ceiling on the bucket maps was the one rate-limit number an
// operator could not set. Two directions: the compiled value must arrive, and the
// default must still hold when the section does not mention it.

#[test]
fn max_buckets_arrives_from_the_compiled_schema() {
    let sec = RateLimitingSecurityConfig {
        enabled: true,
        max_buckets: 4_096,
        ..Default::default()
    };
    let config = RateLimitConfig::from_security_config(&sec);
    assert_eq!(
        config.max_buckets, 4_096,
        "the compiled [security.rate_limiting] max_buckets must reach the limiter"
    );
}

#[test]
fn max_buckets_keeps_its_default_when_the_section_omits_it() {
    let sec = RateLimitingSecurityConfig {
        enabled: true,
        ..Default::default()
    };
    let config = RateLimitConfig::from_security_config(&sec);
    assert_eq!(
        config.max_buckets, 100_000,
        "an operator who never named max_buckets keeps the documented ceiling"
    );
}

#[tokio::test]
async fn a_compiled_max_buckets_bounds_the_live_map() {
    // The knob is only real if the limiter honours it. Three distinct IPs against a
    // ceiling of two: the map must not hold all three.
    let sec = RateLimitingSecurityConfig {
        enabled: true,
        requests_per_second: 1_000,
        burst_size: 1_000,
        max_buckets: 2,
        ..Default::default()
    };
    let limiter = RateLimiter::new(RateLimitConfig::from_security_config(&sec));
    for ip in ["10.0.0.1", "10.0.0.2", "10.0.0.3"] {
        assert!(limiter.check_ip_limit(ip).await.allowed, "budget is 1000/s; {ip} must pass");
    }
    assert!(
        ip_bucket_count(&limiter) <= 2,
        "a compiled ceiling of 2 must bound the map, saw {}",
        ip_bucket_count(&limiter)
    );
}

#[test]
fn an_env_override_sizes_max_buckets_per_deployment() {
    // The compiled schema is one artefact shipped to every environment; how much memory
    // a host will spend on tracking state is not a property of it. `FRAISEQL_RATE_LIMIT
    // _MAX_BUCKETS` is the layer that knows.
    let sec = RateLimitingSecurityConfig {
        enabled: true,
        max_buckets: 100_000,
        ..Default::default()
    };
    let mut config = RateLimitConfig::from_security_config(&sec);
    RateLimitOverrides {
        max_buckets: Some(1_024),
        ..Default::default()
    }
    .apply_to(&mut config);
    assert_eq!(config.max_buckets, 1_024, "the env override must win over the compiled value");
}

#[test]
fn a_max_buckets_override_alone_counts_as_an_override() {
    // `is_empty` decides whether overrides can switch rate limiting on at all (#774). A
    // new field that `is_empty` does not know about is invisible to that decision.
    let overrides = RateLimitOverrides {
        max_buckets: Some(1_024),
        ..Default::default()
    };
    assert!(!overrides.is_empty(), "max_buckets must count as a supplied override");
}

// ── #1171 part 2: the per-user bucket, on a verified subject ─────────────────
//
// #1143 deleted the HTTP per-user allowance because the identity behind it came from an
// unverified JWT payload — varying `sub` minted a fresh full bucket, so the "allowance"
// was an unlimited budget for anyone who sent a JWT-shaped string. These drive the real
// middleware through `tower::ServiceExt::oneshot` and assert both halves: the allowance
// is back for a caller the deployment's validator accepts, and it is still unavailable
// to everyone else.

mod verified_per_user_tests {
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::Arc,
    };

    use axum::{
        Extension, Router,
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
        routing::get,
    };
    use fraiseql_core::security::{AuthConfig, AuthMiddleware};
    use tower::ServiceExt;

    use super::super::{
        RateLimitConfig, dispatch::RateLimiter, identity::VerifiedSubject,
        middleware_fn::rate_limit_middleware,
    };

    const SECRET: &str = "a-shared-secret-of-sufficient-length-for-hs256-signing";

    /// A budget of one request, so the second from the same bucket is refused. Anything
    /// larger cannot tell "two buckets" from "one generous bucket".
    fn limiter() -> Arc<RateLimiter> {
        Arc::new(RateLimiter::new(RateLimitConfig {
            enabled: true,
            rps_per_ip: 1,
            rps_per_user: 1,
            burst_size: 1,
            ..RateLimitConfig::default()
        }))
    }

    /// An audience is configured, because the core validator refuses a token that
    /// carries `aud` when none is expected (`JwtAudienceMismatch { expected: "(not
    /// configured)" }`). A deployment that means to accept these tokens declares it.
    fn hs256_subject() -> Arc<VerifiedSubject> {
        Arc::new(VerifiedSubject::Hs256(Arc::new(AuthMiddleware::from_config(
            AuthConfig::with_hs256(SECRET).with_audience("fraiseql"),
        ))))
    }

    fn token_for(sub: &str, secret: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after the epoch")
            .as_secs();
        let claims = fraiseql_auth::Claims {
            sub:   sub.to_owned(),
            iat:   now,
            exp:   now + 3600,
            nbf:   None,
            iss:   "fraiseql".to_owned(),
            aud:   vec!["fraiseql".to_owned()],
            extra: std::collections::HashMap::new(),
        };
        fraiseql_auth::generate_hs256_token(&claims, secret.as_bytes()).expect("token")
    }

    fn app(limiter: Arc<RateLimiter>, subject: Option<Arc<VerifiedSubject>>) -> Router {
        let mut app = Router::new()
            .route("/graphql", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(rate_limit_middleware))
            .layer(Extension(limiter));
        if let Some(subject) = subject {
            app = app.layer(Extension(subject));
        }
        app
    }

    async fn send(app: &Router, ip: [u8; 4], bearer: Option<&str>) -> StatusCode {
        let mut builder = Request::builder().uri("/graphql");
        if let Some(token) = bearer {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let mut request = builder.body(Body::empty()).expect("request");
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), 12_345)));
        app.clone().oneshot(request).await.expect("response").status()
    }

    /// The gap #1171 names: many authenticated users behind one egress address are not
    /// one client, and before this they shared one budget.
    #[tokio::test]
    async fn two_verified_users_behind_one_ip_do_not_share_a_bucket() {
        let app = app(limiter(), Some(hs256_subject()));
        let (alice, bob) = (token_for("alice", SECRET), token_for("bob", SECRET));

        assert_eq!(send(&app, [203, 0, 113, 9], Some(&alice)).await, StatusCode::OK);
        assert_eq!(
            send(&app, [203, 0, 113, 9], Some(&bob)).await,
            StatusCode::OK,
            "bob's first request must not be refused by alice's spent budget — same IP, \
             different verified subject"
        );
    }

    /// …and each of them is still limited, so the allowance is a budget rather than an
    /// exemption. Without this, a fix that simply skipped the limiter for authenticated
    /// callers would pass the test above.
    #[tokio::test]
    async fn a_verified_user_is_still_limited() {
        let app = app(limiter(), Some(hs256_subject()));
        let alice = token_for("alice", SECRET);

        assert_eq!(send(&app, [203, 0, 113, 9], Some(&alice)).await, StatusCode::OK);
        assert_eq!(
            send(&app, [203, 0, 113, 9], Some(&alice)).await,
            StatusCode::TOO_MANY_REQUESTS,
            "a verified subject gets its own bucket, not a bypass"
        );
    }

    /// #1143's property, kept. A token signed with the wrong secret does not verify, so
    /// it yields no subject and the request falls back to the address bucket — varying
    /// `sub` on a forged token mints nothing.
    #[tokio::test]
    async fn a_forged_token_cannot_mint_a_bucket() {
        let app = app(limiter(), Some(hs256_subject()));
        let forged_one = token_for("attacker-1", "not-the-deployments-signing-secret-at-all");
        let forged_two = token_for("attacker-2", "not-the-deployments-signing-secret-at-all");

        assert_eq!(send(&app, [198, 51, 100, 7], Some(&forged_one)).await, StatusCode::OK);
        assert_eq!(
            send(&app, [198, 51, 100, 7], Some(&forged_two)).await,
            StatusCode::TOO_MANY_REQUESTS,
            "a second forged identity from the same address must hit the same IP bucket"
        );
    }

    /// An unauthenticated caller buckets on its address, exactly as before.
    #[tokio::test]
    async fn an_anonymous_caller_buckets_on_its_address() {
        let app = app(limiter(), Some(hs256_subject()));

        assert_eq!(send(&app, [198, 51, 100, 8], None).await, StatusCode::OK);
        assert_eq!(send(&app, [198, 51, 100, 8], None).await, StatusCode::TOO_MANY_REQUESTS);
    }

    /// A deployment with no authentication configured has no verified subject to key on.
    /// Its requests bucket on the address whatever they carry — so a bearer token cannot
    /// buy a fresh bucket by the mere fact of being present.
    #[tokio::test]
    async fn without_a_validator_a_bearer_token_changes_nothing() {
        let app = app(limiter(), None);
        let alice = token_for("alice", SECRET);
        let bob = token_for("bob", SECRET);

        assert_eq!(send(&app, [198, 51, 100, 9], Some(&alice)).await, StatusCode::OK);
        assert_eq!(
            send(&app, [198, 51, 100, 9], Some(&bob)).await,
            StatusCode::TOO_MANY_REQUESTS,
            "no validator means no per-user bucket, for anyone"
        );
    }
}
