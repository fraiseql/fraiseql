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

impl ValidationRateLimitingConfig {
    /// Returns a builder for `ValidationRateLimitingConfig`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> ValidationRateLimitingConfigBuilder {
        ValidationRateLimitingConfigBuilder::default()
    }
}

/// Builder for [`ValidationRateLimitingConfig`].
#[derive(Debug, Default)]
pub struct ValidationRateLimitingConfigBuilder {
    inner: ValidationRateLimitingConfig,
}

impl ValidationRateLimitingConfigBuilder {
    /// Sets whether validation rate limiting is enabled.
    #[must_use = "builder method returns modified builder"]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.inner.enabled = enabled;
        self
    }

    /// Sets the max requests and window for validation errors.
    #[must_use = "builder method returns modified builder"]
    pub const fn validation_errors(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.inner.validation_errors_max_requests = max_requests;
        self.inner.validation_errors_window_secs = window_secs;
        self
    }

    /// Sets the max requests and window for depth errors.
    #[must_use = "builder method returns modified builder"]
    pub const fn depth_errors(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.inner.depth_errors_max_requests = max_requests;
        self.inner.depth_errors_window_secs = window_secs;
        self
    }

    /// Sets the max requests and window for complexity errors.
    #[must_use = "builder method returns modified builder"]
    pub const fn complexity_errors(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.inner.complexity_errors_max_requests = max_requests;
        self.inner.complexity_errors_window_secs = window_secs;
        self
    }

    /// Sets the max requests and window for malformed errors.
    #[must_use = "builder method returns modified builder"]
    pub const fn malformed_errors(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.inner.malformed_errors_max_requests = max_requests;
        self.inner.malformed_errors_window_secs = window_secs;
        self
    }

    /// Sets the max requests and window for async validation errors.
    #[must_use = "builder method returns modified builder"]
    pub const fn async_validation_errors(mut self, max_requests: u32, window_secs: u64) -> Self {
        self.inner.async_validation_errors_max_requests = max_requests;
        self.inner.async_validation_errors_window_secs = window_secs;
        self
    }

    /// Builds the [`ValidationRateLimitingConfig`].
    #[must_use = "building a config that is not used has no effect"]
    pub const fn build(self) -> ValidationRateLimitingConfig {
        self.inner
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
pub(crate) struct DimensionRateLimiter {
    records:   Arc<Mutex<LruCache<String, RequestRecord>>>,
    dimension: RateLimitDimension,
    clock:     Arc<dyn Clock>,
}

impl DimensionRateLimiter {
    #[cfg(test)]
    pub(crate) fn new(max_requests: u32, window_secs: u64) -> Self {
        Self::new_with_clock(max_requests, window_secs, Arc::new(SystemClock))
    }

    pub(crate) fn new_with_clock(
        max_requests: u32,
        window_secs: u64,
        clock: Arc<dyn Clock>,
    ) -> Self {
        #[allow(clippy::expect_used)]
        // Reason: MAX_RATE_LIMITER_ENTRIES is a non-zero compile-time constant
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

    pub(crate) fn check(&self, key: &str) -> Result<(), FraiseQLError> {
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

    pub(crate) fn clear(&self) {
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
    #[must_use] 
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
