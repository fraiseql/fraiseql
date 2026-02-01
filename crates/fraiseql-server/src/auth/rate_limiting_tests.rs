// Rate limiting integration tests
// Phase 7, Cycle 5: Tests for KeyedRateLimiter functionality

#[cfg(test)]
mod rate_limiting_tests {
    use crate::auth::rate_limiting::{KeyedRateLimiter, RateLimitConfig};

    #[test]
    fn test_rate_limit_allows_requests_within_limit() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window_secs:  60,
        });

        for i in 0..10 {
            let result = limiter.check(&format!("user_{}", i));
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[test]
    fn test_rate_limit_rejects_over_limit() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_secs:  60,
        });

        for i in 0..3 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        let result = limiter.check("key");
        assert!(result.is_err(), "4th request should be rejected");
    }

    #[test]
    fn test_rate_limit_per_key_independent() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_secs:  60,
        });

        limiter.check("key1").ok();
        limiter.check("key1").ok();

        let result = limiter.check("key2");
        assert!(result.is_ok(), "Different key should have independent limit");
    }

    #[test]
    fn test_rate_limit_error_contains_retry_info() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");

        match result {
            Err(crate::auth::error::AuthError::RateLimited { retry_after_secs }) => {
                assert_eq!(retry_after_secs, 60);
            },
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_rate_limit_by_ip() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window_secs:  60,
        });

        let ip = "192.168.1.100";

        for i in 0..5 {
            let result = limiter.check(ip);
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }

        let result = limiter.check(ip);
        assert!(result.is_err(), "6th request should be rejected");
    }

    #[test]
    fn test_different_ips_independent_limits() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_secs:  60,
        });

        let ip1 = "192.168.1.1";
        let ip2 = "192.168.1.2";

        for _ in 0..3 {
            limiter.check(ip1).ok();
        }

        let result = limiter.check(ip2);
        assert!(result.is_ok(), "Different IP should have independent limit");

        let result = limiter.check(ip1);
        assert!(result.is_err(), "IP1 should be blocked");
    }

    #[test]
    fn test_failed_login_attempts() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window_secs:  3600,
        });

        let user = "alice@example.com";

        for i in 0..5 {
            let result = limiter.check(user);
            assert!(result.is_ok(), "Attempt {} should be allowed", i);
        }

        let result = limiter.check(user);
        assert!(result.is_err(), "6th attempt should be blocked");
    }

    #[test]
    fn test_multiple_users_independent() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 5,
            window_secs:  3600,
        });

        for _ in 0..5 {
            limiter.check("user1").ok();
        }

        let result = limiter.check("user1");
        assert!(result.is_err(), "User1 should be blocked");

        let result = limiter.check("user2");
        assert!(result.is_ok(), "User2 should have fresh attempts");
    }

    #[test]
    fn test_active_limiters_count() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 100,
            window_secs:  60,
        });

        assert_eq!(limiter.active_limiters(), 0);

        limiter.check("key1").ok();
        assert_eq!(limiter.active_limiters(), 1);

        limiter.check("key2").ok();
        assert_eq!(limiter.active_limiters(), 2);
    }

    #[test]
    fn test_clear_limiters() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");
        assert!(result.is_err());

        limiter.clear();

        let result = limiter.check("key");
        assert!(result.is_ok(), "After clear, should allow again");
    }

    #[test]
    fn test_thread_safe_rate_limiting() {
        use std::sync::Arc;

        let limiter = Arc::new(KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 100,
            window_secs:  60,
        }));

        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = Arc::clone(&limiter);
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = limiter_clone.check("concurrent");
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().ok();
        }

        let result = limiter.check("concurrent");
        assert!(result.is_err(), "After 100 concurrent requests, next should fail");
    }

    #[test]
    fn test_presets() {
        let standard_ip = RateLimitConfig::per_ip_standard();
        assert_eq!(standard_ip.max_requests, 100);
        assert_eq!(standard_ip.window_secs, 60);

        let strict_ip = RateLimitConfig::per_ip_strict();
        assert_eq!(strict_ip.max_requests, 50);

        let user_limit = RateLimitConfig::per_user_standard();
        assert_eq!(user_limit.max_requests, 10);

        let failed = RateLimitConfig::failed_login_attempts();
        assert_eq!(failed.max_requests, 5);
        assert_eq!(failed.window_secs, 3600);
    }
}
