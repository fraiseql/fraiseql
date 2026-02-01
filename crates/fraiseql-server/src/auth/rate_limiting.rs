// Rate limiting for brute-force protection
// Uses an in-memory approach with Arc and Mutex for simplicity
// Phase 7, Cycle 5: GREEN phase - Implementation

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::auth::error::{AuthError, Result};

/// Rate limit configuration for an endpoint
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed in the window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_secs:  u64,
}

impl RateLimitConfig {
    /// IP-based rate limiting for public endpoints
    /// 100 requests per 60 seconds (typical for auth/start, auth/callback)
    pub fn per_ip_standard() -> Self {
        Self {
            max_requests: 100,
            window_secs:  60,
        }
    }

    /// Stricter IP-based rate limiting for sensitive endpoints
    /// 50 requests per 60 seconds
    pub fn per_ip_strict() -> Self {
        Self {
            max_requests: 50,
            window_secs:  60,
        }
    }

    /// User-based rate limiting for authenticated endpoints
    /// 10 requests per 60 seconds
    pub fn per_user_standard() -> Self {
        Self {
            max_requests: 10,
            window_secs:  60,
        }
    }

    /// Failed login attempt limiting
    /// 5 failed attempts per 3600 seconds (1 hour)
    pub fn failed_login_attempts() -> Self {
        Self {
            max_requests: 5,
            window_secs:  3600,
        }
    }
}

/// Request record for tracking
#[derive(Debug, Clone)]
struct RequestRecord {
    /// Number of requests in current window
    count:        u32,
    /// Unix timestamp of window start
    window_start: u64,
}

/// Per-key rate limiter using in-memory tracking
/// Maintains separate rate limits for each key (IP, user ID, etc.)
pub struct KeyedRateLimiter {
    records: Arc<Mutex<HashMap<String, RequestRecord>>>,
    config:  RateLimitConfig,
}

impl KeyedRateLimiter {
    /// Create a new keyed rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Get current Unix timestamp in seconds
    fn current_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    /// Check if a request should be allowed for the given key
    ///
    /// Returns Ok(()) if allowed, Err with status code if rate limited
    pub fn check(&self, key: &str) -> Result<()> {
        let mut records = self.records.lock().unwrap();
        let now = Self::current_timestamp();

        let record = records.entry(key.to_string()).or_insert_with(|| RequestRecord {
            count:        0,
            window_start: now,
        });

        // Check if window has expired
        if now >= record.window_start + self.config.window_secs {
            // Reset window
            record.count = 1;
            record.window_start = now;
            Ok(())
        } else if record.count < self.config.max_requests {
            // Request allowed
            record.count += 1;
            Ok(())
        } else {
            // Rate limited
            Err(AuthError::RateLimited {
                retry_after_secs: self.config.window_secs,
            })
        }
    }

    /// Get the number of active rate limiters (for monitoring)
    pub fn active_limiters(&self) -> usize {
        let records = self.records.lock().unwrap();
        records.len()
    }

    /// Clear all rate limiters (for testing or reset)
    pub fn clear(&self) {
        let mut records = self.records.lock().unwrap();
        records.clear();
    }

    /// Create a copy for independent testing
    pub fn clone_config(&self) -> RateLimitConfig {
        self.config.clone()
    }
}

/// Global rate limiters for different endpoints
pub struct RateLimiters {
    /// auth/start: per-IP, 100 req/min
    pub auth_start:    KeyedRateLimiter,
    /// auth/callback: per-IP, 50 req/min
    pub auth_callback: KeyedRateLimiter,
    /// auth/refresh: per-user, 10 req/min
    pub auth_refresh:  KeyedRateLimiter,
    /// auth/logout: per-user, 20 req/min
    pub auth_logout:   KeyedRateLimiter,
    /// Failed login tracking: per-user, 5 attempts/hour
    pub failed_logins: KeyedRateLimiter,
}

impl RateLimiters {
    /// Create default rate limiters for all endpoints
    pub fn new() -> Self {
        Self {
            auth_start:    KeyedRateLimiter::new(RateLimitConfig::per_ip_standard()),
            auth_callback: KeyedRateLimiter::new(RateLimitConfig::per_ip_strict()),
            auth_refresh:  KeyedRateLimiter::new(RateLimitConfig::per_user_standard()),
            auth_logout:   KeyedRateLimiter::new(RateLimitConfig::per_user_standard()),
            failed_logins: KeyedRateLimiter::new(RateLimitConfig::failed_login_attempts()),
        }
    }

    /// Create with custom configurations
    pub fn with_configs(
        start_cfg: RateLimitConfig,
        callback_cfg: RateLimitConfig,
        refresh_cfg: RateLimitConfig,
        logout_cfg: RateLimitConfig,
        failed_cfg: RateLimitConfig,
    ) -> Self {
        Self {
            auth_start:    KeyedRateLimiter::new(start_cfg),
            auth_callback: KeyedRateLimiter::new(callback_cfg),
            auth_refresh:  KeyedRateLimiter::new(refresh_cfg),
            auth_logout:   KeyedRateLimiter::new(logout_cfg),
            failed_logins: KeyedRateLimiter::new(failed_cfg),
        }
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window_secs:  60,
        });

        // Should allow up to max_requests
        for i in 0..3 {
            let result = limiter.check("key");
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[test]
    fn test_rate_limiter_rejects_over_limit() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_secs:  60,
        });

        limiter.check("key").ok();
        limiter.check("key").ok();

        // Third should fail
        let result = limiter.check("key");
        assert!(result.is_err(), "Request over limit should fail");
    }

    #[test]
    fn test_rate_limiter_per_key() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 2,
            window_secs:  60,
        });

        // Key 1: use allowance
        limiter.check("key1").ok();
        limiter.check("key1").ok();

        // Key 2: should have fresh allowance
        let result = limiter.check("key2");
        assert!(result.is_ok(), "Different key should have independent limit");
    }

    #[test]
    fn test_rate_limiter_error_contains_retry_after() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window_secs:  60,
        });

        limiter.check("key").ok();
        let result = limiter.check("key");

        match result {
            Err(AuthError::RateLimited { retry_after_secs }) => {
                assert_eq!(retry_after_secs, 60);
            },
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_rate_limiter_active_limiters_count() {
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
    fn test_rate_limiters_default() {
        let limiters = RateLimiters::new();

        // auth/start should allow requests
        let result = limiters.auth_start.check("ip_1");
        assert!(result.is_ok());

        // auth/refresh should track per-user
        let result = limiters.auth_refresh.check("user_1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_rate_limit_config_presets() {
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

    #[test]
    fn test_ip_based_rate_limiting() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig::per_ip_standard());

        let ip = "203.0.113.1";

        // Should allow up to 100 requests
        for _ in 0..100 {
            let result = limiter.check(ip);
            assert!(result.is_ok());
        }

        // 101st should fail
        let result = limiter.check(ip);
        assert!(result.is_err());
    }

    #[test]
    fn test_failed_login_tracking() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig::failed_login_attempts());

        let user = "alice@example.com";

        // Should allow 5 failed attempts
        for _ in 0..5 {
            let result = limiter.check(user);
            assert!(result.is_ok());
        }

        // 6th should fail
        let result = limiter.check(user);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_users_independent() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig::failed_login_attempts());

        // User 1 uses attempts
        for _ in 0..5 {
            limiter.check("user1").ok();
        }

        // User 1 blocked
        let result = limiter.check("user1");
        assert!(result.is_err());

        // User 2 should have fresh attempts
        let result = limiter.check("user2");
        assert!(result.is_ok());
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

        // After clear, should allow again
        let result = limiter.check("key");
        assert!(result.is_ok());
    }

    #[test]
    fn test_thread_safe_rate_limiting() {
        use std::sync::Arc as StdArc;

        let limiter = StdArc::new(KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 100,
            window_secs:  60,
        }));

        let mut handles = vec![];

        for _ in 0..10 {
            let limiter_clone = StdArc::clone(&limiter);
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

        // After 100 concurrent requests, next should fail
        let result = limiter.check("concurrent");
        assert!(result.is_err());
    }

    #[test]
    fn test_rate_limiting_many_keys() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window_secs:  60,
        });

        // Simulate 1000 different IPs, each with requests
        for i in 0..1000 {
            let key = format!("192.168.{}.{}", i / 256, i % 256);
            let result = limiter.check(&key);
            assert!(result.is_ok());
        }

        assert_eq!(limiter.active_limiters(), 1000);
    }

    #[test]
    fn test_endpoint_combinations() {
        let limiters = RateLimiters::new();

        let ip = "203.0.113.1";
        let user = "bob@example.com";

        // Complete flow
        let result = limiters.auth_start.check(ip);
        assert!(result.is_ok());

        let result = limiters.auth_callback.check(ip);
        assert!(result.is_ok());

        let result = limiters.auth_refresh.check(user);
        assert!(result.is_ok());

        let result = limiters.auth_logout.check(user);
        assert!(result.is_ok());

        let result = limiters.failed_logins.check(user);
        assert!(result.is_ok());
    }

    #[test]
    fn test_attack_prevention_scenario() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window_secs:  60,
        });

        let target = "admin@example.com";

        // Attacker tries 10 failed attempts
        for _ in 0..10 {
            let _ = limiter.check(target);
        }

        // 11th blocked
        let result = limiter.check(target);
        assert!(result.is_err());
    }
}
