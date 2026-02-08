//! Integration tests for validation rate limiting.
//!
//! Tests validation-specific rate limiting with per-dimension tracking
//! (IP address, user ID, API key, tenant).

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use fraiseql_core::validation::rate_limiting::{
        ValidationRateLimiter, ValidationRateLimitingConfig,
    };

    /// Test basic rate limiting on validation errors.
    #[test]
    fn test_validation_rate_limiting_basic() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "192.168.1.100";

        // First 100 requests should succeed (default config)
        for i in 0..100 {
            let result = limiter.check_validation_errors(key);
            assert!(result.is_ok(), "Request {} should succeed", i + 1);
        }

        // Request beyond limit should fail
        let result = limiter.check_validation_errors(key);
        assert!(result.is_err(), "Request beyond limit should be rate limited");
    }

    /// Test rate limiting with different keys (IP addresses).
    #[test]
    fn test_validation_rate_limiting_different_keys() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);

        let key1 = "192.168.1.100";
        let key2 = "192.168.1.101";

        // Key1 hits limit at 100
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(key1);
        }

        // Key1 should be limited
        assert!(limiter.check_validation_errors(key1).is_err());

        // Key2 should still have allowance
        assert!(limiter.check_validation_errors(key2).is_ok());
    }

    /// Test rate limiting with time window expiry.
    #[test]
    fn test_validation_rate_limiting_window_expiry() {
        let config = ValidationRateLimitingConfig {
            validation_errors_window_secs:  1,
            validation_errors_max_requests: 3,
            ..ValidationRateLimitingConfig::default()
        };

        let limiter = ValidationRateLimiter::new(config);
        let key = "test-key";

        // Use up the limit
        assert!(limiter.check_validation_errors(key).is_ok());
        assert!(limiter.check_validation_errors(key).is_ok());
        assert!(limiter.check_validation_errors(key).is_ok());
        assert!(limiter.check_validation_errors(key).is_err());

        // Wait for window to expire
        thread::sleep(Duration::from_millis(1100));

        // Should allow more requests
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test rate limiting for depth violations.
    #[test]
    fn test_validation_rate_limiting_depth_errors() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "192.168.1.100";

        // Depth errors should have separate tracking
        for _ in 0..50 {
            assert!(limiter.check_depth_errors(key).is_ok());
        }

        // Should still allow validation errors
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test rate limiting for complexity violations.
    #[test]
    fn test_validation_rate_limiting_complexity_errors() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "192.168.1.100";

        // Complexity errors should have separate tracking
        for _ in 0..30 {
            assert!(limiter.check_complexity_errors(key).is_ok());
        }

        // Should still allow validation errors
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test rate limiting for malformed queries.
    #[test]
    fn test_validation_rate_limiting_malformed_errors() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "192.168.1.100";

        // Malformed errors should have separate tracking
        for _ in 0..40 {
            assert!(limiter.check_malformed_errors(key).is_ok());
        }

        // Should still allow validation errors
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test rate limiting for async validation failures.
    #[test]
    fn test_validation_rate_limiting_async_validation_errors() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "192.168.1.100";

        // Async validation errors should have separate tracking
        for _ in 0..60 {
            assert!(limiter.check_async_validation_errors(key).is_ok());
        }

        // Should still allow validation errors
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test per-user rate limiting.
    #[test]
    fn test_validation_rate_limiting_per_user() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);

        let user1 = "user:123";
        let user2 = "user:456";

        // User1 hits limit
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(user1);
        }

        assert!(limiter.check_validation_errors(user1).is_err());

        // User2 should be independent
        assert!(limiter.check_validation_errors(user2).is_ok());
    }

    /// Test tenant isolation.
    #[test]
    fn test_validation_rate_limiting_tenant_isolation() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);

        let tenant1_ip = "tenant:1:192.168.1.100";
        let tenant2_ip = "tenant:2:192.168.1.100";

        // Tenant1 hits limit
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(tenant1_ip);
        }

        assert!(limiter.check_validation_errors(tenant1_ip).is_err());

        // Tenant2 with same IP should be independent
        assert!(limiter.check_validation_errors(tenant2_ip).is_ok());
    }

    /// Test concurrent access to rate limiter.
    #[test]
    fn test_validation_rate_limiting_concurrent() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = std::sync::Arc::new(ValidationRateLimiter::new(config));

        let key = "concurrent-test";
        let mut handles = vec![];

        // Spawn 10 threads, each making 10 requests
        for _ in 0..10 {
            let limiter_clone = limiter.clone();
            let key_clone = key.to_string();

            let handle = std::thread::spawn(move || {
                let mut successful = 0;

                for _ in 0..10 {
                    if limiter_clone.check_validation_errors(&key_clone).is_ok() {
                        successful += 1;
                    }
                }

                successful
            });

            handles.push(handle);
        }

        let mut total_success = 0;

        for handle in handles {
            let success = handle.join().unwrap();
            total_success += success;
        }

        // Should have exactly 100 successful requests (limit is 100)
        assert_eq!(total_success, 100);
    }

    /// Test rate limiting with custom config.
    #[test]
    fn test_validation_rate_limiting_custom_config() {
        let config = ValidationRateLimitingConfig {
            validation_errors_max_requests: 5,
            validation_errors_window_secs:  60,
            ..ValidationRateLimitingConfig::default()
        };

        let limiter = ValidationRateLimiter::new(config);
        let key = "custom-config";

        // Should allow 5 requests
        for i in 0..5 {
            assert!(
                limiter.check_validation_errors(key).is_ok(),
                "Request {} should succeed",
                i + 1
            );
        }

        // 6th should fail
        assert!(limiter.check_validation_errors(key).is_err());
    }

    /// Test that cloned limiter shares state.
    #[test]
    fn test_validation_rate_limiting_clone_shares_state() {
        let config = ValidationRateLimitingConfig::default();
        let limiter1 = ValidationRateLimiter::new(config);
        let limiter2 = limiter1.clone();

        let key = "shared-key";

        // Make requests on limiter1
        for _ in 0..100 {
            let _ = limiter1.check_validation_errors(key);
        }

        // limiter2 (cloned) should see the same limit
        assert!(limiter2.check_validation_errors(key).is_err());
    }

    /// Test error response includes retry_after_secs.
    #[test]
    fn test_validation_rate_limiting_error_response() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "error-test";

        // Use up allowance
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(key);
        }

        // Next should fail with rate limit error
        let result = limiter.check_validation_errors(key);
        assert!(result.is_err());

        // Error should indicate rate limiting
        if let Err(err) = result {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.to_lowercase().contains("rate") || err_str.to_lowercase().contains("limit")
            );
        }
    }

    /// Test clearing rate limiter state.
    #[test]
    fn test_validation_rate_limiting_clear() {
        let config = ValidationRateLimitingConfig {
            validation_errors_max_requests: 3,
            ..ValidationRateLimitingConfig::default()
        };

        let limiter = ValidationRateLimiter::new(config);
        let key = "test-key";

        // Hit the limit
        for _ in 0..3 {
            let _ = limiter.check_validation_errors(key);
        }
        assert!(limiter.check_validation_errors(key).is_err());

        // Clear and retry
        limiter.clear();
        assert!(limiter.check_validation_errors(key).is_ok());
    }

    /// Test configuration loading and environment variable overrides.
    #[test]
    fn test_validation_rate_limiting_config_defaults() {
        let config = ValidationRateLimitingConfig::default();

        // Check that config has sensible defaults
        assert!(config.validation_errors_max_requests > 0);
        assert!(config.validation_errors_window_secs > 0);
        assert!(config.depth_errors_max_requests > 0);
        assert!(config.complexity_errors_max_requests > 0);
        assert!(config.malformed_errors_max_requests > 0);
        assert!(config.async_validation_errors_max_requests > 0);
    }

    /// Test multiple concurrent clients with different error types.
    #[test]
    fn test_validation_rate_limiting_multiple_clients_different_errors() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = std::sync::Arc::new(ValidationRateLimiter::new(config));

        let mut handles = vec![];

        // 3 clients with different error types
        for error_type in 0..3 {
            let limiter_clone = limiter.clone();

            let handle = std::thread::spawn(move || {
                let key = format!("client:{}", error_type);
                let mut count = 0;

                for _ in 0..20 {
                    let result = match error_type {
                        0 => limiter_clone.check_validation_errors(&key),
                        1 => limiter_clone.check_depth_errors(&key),
                        _ => limiter_clone.check_complexity_errors(&key),
                    };

                    if result.is_ok() {
                        count += 1;
                    }
                }

                count
            });

            handles.push(handle);
        }

        let mut total_requests = 0;

        for handle in handles {
            let count = handle.join().unwrap();
            total_requests += count;
        }

        // Each client should get 20 requests independently
        assert_eq!(total_requests, 60);
    }

    /// Test that config can be cloned.
    #[test]
    fn test_validation_rate_limiting_config_clone() {
        let config1 = ValidationRateLimitingConfig::default();
        let config2 = config1.clone();

        let limiter1 = ValidationRateLimiter::new(config1);
        let limiter2 = ValidationRateLimiter::new(config2);

        let key = "config-clone-test";

        // Both should have same limits
        for _ in 0..100 {
            assert!(limiter1.check_validation_errors(key).is_ok());
            assert!(limiter2.check_validation_errors(key).is_ok());
        }

        assert!(limiter1.check_validation_errors(key).is_err());
        assert!(limiter2.check_validation_errors(key).is_err());
    }

    /// Test independent rate limiting for different error dimensions.
    #[test]
    fn test_validation_rate_limiting_independent_dimensions() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "test-key";

        // All dimensions should be independent
        // Use up validation errors
        for _ in 0..100 {
            assert!(limiter.check_validation_errors(key).is_ok());
        }
        assert!(limiter.check_validation_errors(key).is_err());

        // But depth errors should still work
        assert!(limiter.check_depth_errors(key).is_ok());

        // And complexity errors
        assert!(limiter.check_complexity_errors(key).is_ok());

        // And async validation errors
        assert!(limiter.check_async_validation_errors(key).is_ok());
    }
}
