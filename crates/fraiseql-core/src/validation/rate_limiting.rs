//! Validation-specific rate limiting with per-dimension tracking.
//!
//! Provides rate limiting for different types of validation errors:
//! - `validation_errors`: General field-level validation failures
//! - `depth_errors`: Query depth limit violations
//! - `complexity_errors`: Query complexity limit violations
//! - `malformed_errors`: Malformed query/input errors
//! - `async_validation_errors`: Async validator failures

use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use lru::LruCache;

use crate::{
    error::FraiseQLError,
    utils::clock::{Clock, SystemClock},
};

/// Maximum number of distinct keys (IPs, user IDs) tracked per rate-limiter
/// dimension. Prevents unbounded memory growth from IP rotation attacks.
const MAX_RATE_LIMITER_ENTRIES: usize = 100_000;

/// Rate limit configuration for a single dimension
#[derive(Debug, Clone)]
pub struct RateLimitDimension {
    /// Maximum number of errors allowed in the window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_secs:  u64,
}

impl RateLimitDimension {
    const fn is_rate_limited(&self) -> bool {
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
    records:   Arc<Mutex<LruCache<String, RequestRecord>>>,
    dimension: RateLimitDimension,
    clock:     Arc<dyn Clock>,
}

impl DimensionRateLimiter {
    #[cfg(test)]
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self::new_with_clock(max_requests, window_secs, Arc::new(SystemClock))
    }

    fn new_with_clock(max_requests: u32, window_secs: u64, clock: Arc<dyn Clock>) -> Self {
        #[allow(clippy::expect_used)]
        // Reason: invariant holds at this point; panic would indicate a logic error
        // Reason: MAX_RATE_LIMITER_ENTRIES is a non-zero compile-time constant.
        let cap = NonZeroUsize::new(MAX_RATE_LIMITER_ENTRIES)
            .expect("MAX_RATE_LIMITER_ENTRIES must be > 0");
        Self {
            records: Arc::new(Mutex::new(LruCache::new(cap))),
            dimension: RateLimitDimension {
                max_requests,
                window_secs,
            },
            clock,
        }
    }

    fn check(&self, key: &str) -> Result<(), FraiseQLError> {
        if self.dimension.is_rate_limited() {
            return Ok(());
        }

        let mut records = self.records.lock().expect("records mutex poisoned");
        let now = self.clock.now_secs();

        // `get_or_insert` promotes the entry to most-recently-used, evicting the
        // least-recently-used entry when the cache is at capacity.
        let record = records.get_or_insert_mut(key.to_string(), || RequestRecord {
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
        let mut records = self.records.lock().expect("records mutex poisoned");
        records.clear();
    }
}

impl Clone for DimensionRateLimiter {
    fn clone(&self) -> Self {
        Self {
            records:   Arc::clone(&self.records),
            dimension: self.dimension.clone(),
            clock:     Arc::clone(&self.clock),
        }
    }
}

/// Validation-specific rate limiter with per-dimension tracking
#[derive(Clone)]
#[allow(clippy::module_name_repetitions, clippy::struct_field_names)] // Reason: RateLimiting prefix provides clarity at call sites
pub struct ValidationRateLimiter {
    validation_errors:       DimensionRateLimiter,
    depth_errors:            DimensionRateLimiter,
    complexity_errors:       DimensionRateLimiter,
    malformed_errors:        DimensionRateLimiter,
    async_validation_errors: DimensionRateLimiter,
}

impl ValidationRateLimiter {
    /// Create a new validation rate limiter with the given configuration.
    pub fn new(config: &ValidationRateLimitingConfig) -> Self {
        Self::new_with_clock(config, Arc::new(SystemClock))
    }

    /// Create a validation rate limiter with a custom clock (for testing).
    pub fn new_with_clock(config: &ValidationRateLimitingConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            validation_errors:       DimensionRateLimiter::new_with_clock(
                config.validation_errors_max_requests,
                config.validation_errors_window_secs,
                Arc::clone(&clock),
            ),
            depth_errors:            DimensionRateLimiter::new_with_clock(
                config.depth_errors_max_requests,
                config.depth_errors_window_secs,
                Arc::clone(&clock),
            ),
            complexity_errors:       DimensionRateLimiter::new_with_clock(
                config.complexity_errors_max_requests,
                config.complexity_errors_window_secs,
                Arc::clone(&clock),
            ),
            malformed_errors:        DimensionRateLimiter::new_with_clock(
                config.malformed_errors_max_requests,
                config.malformed_errors_window_secs,
                Arc::clone(&clock),
            ),
            async_validation_errors: DimensionRateLimiter::new_with_clock(
                config.async_validation_errors_max_requests,
                config.async_validation_errors_window_secs,
                clock,
            ),
        }
    }

    /// Check rate limit for validation errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::RateLimited`] if the key has exceeded the
    /// validation-errors rate limit within the configured window.
    pub fn check_validation_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.validation_errors.check(key)
    }

    /// Check rate limit for depth errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::RateLimited`] if the key has exceeded the
    /// depth-errors rate limit within the configured window.
    pub fn check_depth_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.depth_errors.check(key)
    }

    /// Check rate limit for complexity errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::RateLimited`] if the key has exceeded the
    /// complexity-errors rate limit within the configured window.
    pub fn check_complexity_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.complexity_errors.check(key)
    }

    /// Check rate limit for malformed errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::RateLimited`] if the key has exceeded the
    /// malformed-errors rate limit within the configured window.
    pub fn check_malformed_errors(&self, key: &str) -> Result<(), FraiseQLError> {
        self.malformed_errors.check(key)
    }

    /// Check rate limit for async validation errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::RateLimited`] if the key has exceeded the
    /// async-validation-errors rate limit within the configured window.
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
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 1: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 2: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 3: {e}"));
    }

    #[test]
    fn test_dimension_rate_limiter_rejects_over_limit() {
        let limiter = DimensionRateLimiter::new(2, 60);
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 1: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 2: {e}"));
        assert!(
            matches!(limiter.check("key"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited error on request 3, got: {:?}",
            limiter.check("key")
        );
    }

    #[test]
    fn test_dimension_rate_limiter_per_key() {
        let limiter = DimensionRateLimiter::new(2, 60);
        limiter
            .check("key1")
            .unwrap_or_else(|e| panic!("expected Ok for key1 request 1: {e}"));
        limiter
            .check("key1")
            .unwrap_or_else(|e| panic!("expected Ok for key1 request 2: {e}"));
        limiter
            .check("key2")
            .unwrap_or_else(|e| panic!("expected Ok for key2 request 1 (independent key): {e}"));
    }

    #[test]
    fn test_dimension_rate_limiter_clear() {
        let limiter = DimensionRateLimiter::new(1, 60);
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok before limit: {e}"));
        assert!(
            matches!(limiter.check("key"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited error at limit, got: {:?}",
            limiter.check("key")
        );
        limiter.clear();
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok after clear: {e}"));
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
        let limiter = ValidationRateLimiter::new(&config);
        let key = "test-key";

        // Fill up validation errors
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(key);
        }

        // Validation errors should be limited
        assert!(
            matches!(limiter.check_validation_errors(key), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited after exhausting validation_errors quota"
        );

        // But other dimensions should still work
        limiter
            .check_depth_errors(key)
            .unwrap_or_else(|e| panic!("depth_errors should still allow: {e}"));
        limiter
            .check_complexity_errors(key)
            .unwrap_or_else(|e| panic!("complexity_errors should still allow: {e}"));
        limiter
            .check_malformed_errors(key)
            .unwrap_or_else(|e| panic!("malformed_errors should still allow: {e}"));
        limiter
            .check_async_validation_errors(key)
            .unwrap_or_else(|e| panic!("async_validation_errors should still allow: {e}"));
    }

    #[test]
    fn test_validation_limiter_clone_shares_state() {
        let config = ValidationRateLimitingConfig::default();
        let limiter1 = ValidationRateLimiter::new(&config);
        let limiter2 = limiter1.clone();

        let key = "shared-key";

        for _ in 0..100 {
            let _ = limiter1.check_validation_errors(key);
        }

        // limiter2 should see the same limit
        assert!(
            matches!(limiter2.check_validation_errors(key), Err(FraiseQLError::RateLimited { .. })),
            "cloned limiter should share rate limit state"
        );
    }

    #[test]
    fn test_window_rollover_does_not_leak_across_windows() {
        use std::time::Duration;

        use crate::utils::clock::ManualClock;

        let clock = ManualClock::new();
        let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());
        let config = ValidationRateLimitingConfig {
            enabled: true,
            validation_errors_max_requests: 2,
            validation_errors_window_secs: 60,
            ..ValidationRateLimitingConfig::default()
        };
        let limiter = ValidationRateLimiter::new_with_clock(&config, clock_arc);

        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok on 1st request: {e}")); // 1st
        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok on 2nd request: {e}")); // 2nd
        assert!(
            matches!(limiter.check_validation_errors("u1"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited on 3rd request (over limit)"
        ); // over limit

        clock.advance(Duration::from_secs(61)); // cross the window boundary

        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok after window rollover: {e}")); // new window, limit reset
    }

    /// Sentinel: advancing by exactly `window_secs` must reset the window.
    ///
    /// Kills the `>= → >` mutation on the window-expiry check:
    /// `now >= record.window_start + self.dimension.window_secs`
    #[test]
    fn test_window_exact_boundary_triggers_rollover() {
        use std::time::Duration;

        use crate::utils::clock::ManualClock;

        let clock = ManualClock::new();
        let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());
        let window_secs = 60u64;
        let max = 2u32;
        let limiter = DimensionRateLimiter::new_with_clock(max, window_secs, clock_arc);

        // Fill to limit
        for _ in 0..max {
            limiter.check("u").unwrap_or_else(|e| panic!("expected Ok filling window: {e}"));
        }
        assert!(
            matches!(limiter.check("u"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited when over limit"
        );

        // Advance by EXACTLY window_secs — the `>=` boundary must trigger a reset
        clock.advance(Duration::from_secs(window_secs));

        limiter
            .check("u")
            .unwrap_or_else(|e| panic!("expected Ok at exact window boundary (>= not >): {e}"));
    }

    /// Sentinel: `max_requests = 0` must disable the limiter (every request allowed).
    ///
    /// Kills the `== 0 → != 0` and `== 0 → > 0` mutations on `is_rate_limited()`.
    #[test]
    fn test_max_requests_zero_disables_limiter() {
        let limiter = DimensionRateLimiter::new(0, 60);

        for i in 0..10u32 {
            limiter
                .check("key")
                .unwrap_or_else(|e| panic!("expected Ok with max_requests=0 on request {i}: {e}"));
        }
    }

    /// Sentinel: `window_secs = 0` must not panic.
    ///
    /// With a zero-length window `now >= window_start + 0` is always true, so
    /// every call resets the counter and the limiter never triggers.
    #[test]
    fn test_window_secs_zero_does_not_panic() {
        use crate::utils::clock::ManualClock;

        let clock_arc: Arc<dyn Clock> = Arc::new(ManualClock::new());
        // max_requests > 0 so the limiter is "active", but window_secs = 0
        let limiter = DimensionRateLimiter::new_with_clock(5, 0, clock_arc);

        // Every request resets the window because now >= window_start + 0 is always true
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (1st): {e}"));
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (2nd): {e}"));
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (3rd): {e}"));
    }
}
