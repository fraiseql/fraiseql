//! Rate limiting integration tests.
//!
//! Tests to verify rate limiting middleware is properly integrated into the server
//! and functions correctly for GraphQL and other endpoints.

#[cfg(test)]
mod tests {
    use fraiseql_server::middleware::{RateLimitConfig, RateLimiter};

    /// Helper to create a rate limiter with configurable limits.
    fn create_test_limiter(rps_per_ip: u32, burst_size: u32) -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            enabled: true,
            rps_per_ip,
            rps_per_user: 1000,
            burst_size,
            cleanup_interval_secs: 300,
        })
    }

    #[tokio::test]
    async fn test_rate_limiter_respects_ip_limit() {
        let limiter = create_test_limiter(5, 5);

        // First 5 requests should be allowed
        for i in 0..5 {
            let allowed = limiter.check_ip_limit("192.168.1.1").await;
            assert!(allowed, "Request {} should be allowed within rate limit", i + 1);
        }

        // 6th request should be denied
        let allowed = limiter.check_ip_limit("192.168.1.1").await;
        assert!(!allowed, "Request 6 should exceed rate limit");
    }

    #[tokio::test]
    async fn test_rate_limiter_independent_per_ip() {
        let limiter = create_test_limiter(2, 2);

        // IP 1 gets 2 requests
        assert!(limiter.check_ip_limit("192.168.1.1").await);
        assert!(limiter.check_ip_limit("192.168.1.1").await);
        assert!(!limiter.check_ip_limit("192.168.1.1").await);

        // IP 2 should have its own limit
        assert!(limiter.check_ip_limit("192.168.1.2").await);
        assert!(limiter.check_ip_limit("192.168.1.2").await);
        assert!(!limiter.check_ip_limit("192.168.1.2").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_config_accessible() {
        let limiter = create_test_limiter(100, 500);
        let config = limiter.config();

        assert_eq!(config.rps_per_ip, 100);
        assert_eq!(config.burst_size, 500);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled:               false,
            rps_per_ip:            1,
            rps_per_user:          1,
            burst_size:            1,
            cleanup_interval_secs: 300,
        });

        // Even with extremely low limits, should allow through when disabled
        for _ in 0..100 {
            assert!(limiter.check_ip_limit("192.168.1.1").await);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_remaining_tokens() {
        let limiter = create_test_limiter(10, 10);

        // Start with full capacity
        let remaining_before = limiter.get_ip_remaining("192.168.1.1").await;
        assert_eq!(remaining_before, 10.0);

        // Use one token
        limiter.check_ip_limit("192.168.1.1").await;
        let remaining_after = limiter.get_ip_remaining("192.168.1.1").await;
        assert!(remaining_after < remaining_before);
    }

    #[tokio::test]
    async fn test_rate_limiter_user_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            100,
            rps_per_user:          3,
            burst_size:            3,
            cleanup_interval_secs: 300,
        });

        // Should allow 3 requests for authenticated user
        assert!(limiter.check_user_limit("user123").await);
        assert!(limiter.check_user_limit("user123").await);
        assert!(limiter.check_user_limit("user123").await);

        // 4th request should be denied
        assert!(!limiter.check_user_limit("user123").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_independent_users() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            100,
            rps_per_user:          2,
            burst_size:            2,
            cleanup_interval_secs: 300,
        });

        // User 1 gets 2 requests
        assert!(limiter.check_user_limit("user1").await);
        assert!(limiter.check_user_limit("user1").await);
        assert!(!limiter.check_user_limit("user1").await);

        // User 2 should have independent limit
        assert!(limiter.check_user_limit("user2").await);
        assert!(limiter.check_user_limit("user2").await);
        assert!(!limiter.check_user_limit("user2").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_user_remaining() {
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            100,
            rps_per_user:          10,
            burst_size:            10,
            cleanup_interval_secs: 300,
        });

        let remaining_before = limiter.get_user_remaining("user123").await;
        assert_eq!(remaining_before, 10.0);

        limiter.check_user_limit("user123").await;
        let remaining_after = limiter.get_user_remaining("user123").await;
        assert!(remaining_after < remaining_before);
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup() {
        let limiter = create_test_limiter(10, 10);

        // Use some requests
        limiter.check_ip_limit("192.168.1.1").await;
        limiter.check_ip_limit("192.168.1.2").await;

        // Cleanup should not panic
        limiter.cleanup().await;
    }

    #[tokio::test]
    async fn test_rate_limiter_burst_capacity() {
        // Test that burst_size determines maximum accumulated tokens
        let limiter = RateLimiter::new(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            100,
            rps_per_user:          100,
            burst_size:            5,
            cleanup_interval_secs: 300,
        });

        // Should be able to get initial burst_size worth of tokens
        for _ in 0..5 {
            assert!(limiter.check_ip_limit("192.168.1.1").await);
        }

        // But no more than burst_size
        assert!(!limiter.check_ip_limit("192.168.1.1").await);
    }

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.rps_per_ip, 100);
        assert_eq!(config.rps_per_user, 1000);
        assert_eq!(config.burst_size, 500);
    }
}
