//! Validation-specific rate limiting with per-dimension tracking.
//!
//! Provides rate limiting for different types of validation errors:
//! - validation_errors: General field-level validation failures
//! - depth_errors: Query depth limit violations
//! - complexity_errors: Query complexity limit violations
//! - malformed_errors: Malformed query/input errors
//! - async_validation_errors: Async validator failures

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::FraiseQLError;

/// Rate limit configuration for a single dimension
#[derive(Debug, Clone)]
pub struct RateLimitDimension {
    /// Maximum number of errors allowed in the window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_secs:  u64,
}

impl RateLimitDimension {
    fn is_rate_limited(&self) -> bool {
        self.max_requests == 0
    }
}

/// Configuration for validation-specific rate limiting
#[derive(Debug, Clone)]
pub struct ValidationRateLimitingConfig {
    /// Enable validation rate limiting
    pub enabled: bool,
    /// Validation errors limit
    pub validation_errors_max_requests: u32,
    /// Validation errors window in seconds
    pub validation_errors_window_secs: u64,
    /// Depth errors limit
    pub depth_errors_max_requests: u32,
    /// Depth errors window in seconds
    pub depth_errors_window_secs: u64,
    /// Complexity errors limit
    pub complexity_errors_max_requests: u32,
    /// Complexity errors window in seconds
    pub complexity_errors_window_secs: u64,
    /// Malformed errors limit
    pub malformed_errors_max_requests: u32,
    /// Malformed errors window in seconds
    pub malformed_errors_window_secs: u64,
    /// Async validation errors limit
    pub async_validation_errors_max_requests: u32,
    /// Async validation errors window in seconds
    pub async_validation_errors_window_secs: u64,
}

impl Default for ValidationRateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            validation_errors_max_requests: 100,
            validation_errors_window_secs: 60,
            depth_errors_max_requests: 50,
            depth_errors_window_secs: 60,
            complexity_errors_max_requests: 30,
            complexity_errors_window_secs: 60,
            malformed_errors_max_requests: 40,
            malformed_errors_window_secs: 60,
            async_validation_errors_max_requests: 60,
            async_validation_errors_window_secs: 60,
        }
    }
}

/// Request record for tracking
#[derive(Debug, Clone)]
struct RequestRecord {
    /// Number of errors in current window
    count:        u32,
    /// Unix timestamp of window start
    window_start: u64,
}

/// Single dimension rate limiter
struct DimensionRateLimiter {
    records:   Arc<Mutex<HashMap<String, RequestRecord>>>,
    dimension: RateLimitDimension,
}

impl DimensionRateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            records:   Arc::new(Mutex::new(HashMap::new())),
            dimension: RateLimitDimension {
                max_requests,
                window_secs,
            },
        }
    }

    fn get_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    fn check(&self, key: &str) -> Result<(), FraiseQLError> {
        if self.dimension.is_rate_limited() {
            return Ok(());
        }

        let mut records = self.records.lock().unwrap();
        let now = Self::get_timestamp();

        let record = records.entry(key.to_string()).or_insert_with(|| RequestRecord {
            count:        0,
            window_start: now,
        });

        // Check if window has expired
        if now >= record.window_start + self.dimension.window_secs {
            // Reset window
            record.count = 1;
            record.window_start = now;
            Ok(())
        } else if record.count < self.dimension.max_requests {
            // Request allowed
            record.count += 1;
            Ok(())
        } else {
            // Rate limited
            Err(FraiseQLError::RateLimited {
                message:          "Rate limit exceeded for validation errors".to_string(),
                retry_after_secs: self.dimension.window_secs,
            })
        }
    }

    fn clear(&self) {
        let mut records = self.records.lock().unwrap();
        records.clear();
    }
}

impl Clone for DimensionRateLimiter {
    fn clone(&self) -> Self {
        Self {
            records:   Arc::clone(&self.records),
            dimension: self.dimension.clone(),
        }
    }
}

/// Validation-specific rate limiter with per-dimension tracking
#[derive(Clone)]
#[allow(clippy::module_name_repetitions)]
// Reason: Fields represent different error types (validation vs depth vs complexity),
// and the "_errors" suffix is semantically correct for each dimension.
pub struct ValidationRateLimiter {
    validation_errors:       DimensionRateLimiter,
    depth_errors:            DimensionRateLimiter,
    complexity_errors:       DimensionRateLimiter,
    malformed_errors:        DimensionRateLimiter,
    async_validation_errors: DimensionRateLimiter,
}

impl ValidationRateLimiter {
    /// Create a new validation rate limiter with the given configuration
    pub fn new(config: ValidationRateLimitingConfig) -> Self {
        Self {
            validation_errors:       DimensionRateLimiter::new(
                config.validation_errors_max_requests,
                config.validation_errors_window_secs,
            ),
            depth_errors:            DimensionRateLimiter::new(
                config.depth_errors_max_requests,
                config.depth_errors_window_secs,
            ),
            complexity_errors:       DimensionRateLimiter::new(
                config.complexity_errors_max_requests,
                config.complexity_errors_window_secs,
            ),
            malformed_errors:        DimensionRateLimiter::new(
                config.malformed_errors_max_requests,
                config.malformed_errors_window_secs,
            ),
            async_validation_errors: DimensionRateLimiter::new(
                config.async_validation_errors_max_requests,
                config.async_validation_errors_window_secs,
            ),
        }
    }

    /// Check rate limit for validation errors
    pub fn check_validation_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.validation_errors.check(key)
    }

    /// Check rate limit for depth errors
    pub fn check_depth_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.depth_errors.check(key)
    }

    /// Check rate limit for complexity errors
    pub fn check_complexity_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.complexity_errors.check(key)
    }

    /// Check rate limit for malformed errors
    pub fn check_malformed_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.malformed_errors.check(key)
    }

    /// Check rate limit for async validation errors
    pub fn check_async_validation_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.async_validation_errors.check(key)
    }

    /// Clear all rate limiter state
    pub fn clear(&self) {
        self.validation_errors.clear();
        self.depth_errors.clear();
        self.complexity_errors.clear();
        self.malformed_errors.clear();
        self.async_validation_errors.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_rate_limiter_allows_within_limit() {
        let limiter = DimensionRateLimiter::new(3, 60);
        assert!(limiter.check("key").is_ok());
        assert!(limiter.check("key").is_ok());
        assert!(limiter.check("key").is_ok());
    }

    #[test]
    fn test_dimension_rate_limiter_rejects_over_limit() {
        let limiter = DimensionRateLimiter::new(2, 60);
        assert!(limiter.check("key").is_ok());
        assert!(limiter.check("key").is_ok());
        assert!(limiter.check("key").is_err());
    }

    #[test]
    fn test_dimension_rate_limiter_per_key() {
        let limiter = DimensionRateLimiter::new(2, 60);
        assert!(limiter.check("key1").is_ok());
        assert!(limiter.check("key1").is_ok());
        assert!(limiter.check("key2").is_ok());
    }

    #[test]
    fn test_dimension_rate_limiter_clear() {
        let limiter = DimensionRateLimiter::new(1, 60);
        assert!(limiter.check("key").is_ok());
        assert!(limiter.check("key").is_err());
        limiter.clear();
        assert!(limiter.check("key").is_ok());
    }

    #[test]
    fn test_config_defaults() {
        let config = ValidationRateLimitingConfig::default();
        assert!(config.enabled);
        assert!(config.validation_errors_max_requests > 0);
        assert!(config.depth_errors_max_requests > 0);
        assert!(config.complexity_errors_max_requests > 0);
        assert!(config.malformed_errors_max_requests > 0);
        assert!(config.async_validation_errors_max_requests > 0);
    }

    #[test]
    fn test_validation_limiter_independent_dimensions() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(config);
        let key = "test-key";

        // Fill up validation errors
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(key);
        }

        // Validation errors should be limited
        assert!(limiter.check_validation_errors(key).is_err());

        // But other dimensions should still work
        assert!(limiter.check_depth_errors(key).is_ok());
        assert!(limiter.check_complexity_errors(key).is_ok());
        assert!(limiter.check_malformed_errors(key).is_ok());
        assert!(limiter.check_async_validation_errors(key).is_ok());
    }

    #[test]
    fn test_validation_limiter_clone_shares_state() {
        let config = ValidationRateLimitingConfig::default();
        let limiter1 = ValidationRateLimiter::new(config);
        let limiter2 = limiter1.clone();

        let key = "shared-key";

        for _ in 0..100 {
            let _ = limiter1.check_validation_errors(key);
        }

        // limiter2 should see the same limit
        assert!(limiter2.check_validation_errors(key).is_err());
    }
}
