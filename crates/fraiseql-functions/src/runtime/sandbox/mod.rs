//! Sandbox enforcement: concurrency limits for function invocations.
//!
//! Each function can have at most `max_concurrent` simultaneous invocations.
//! A per-function `ConcurrencyLimiter` wraps a `tokio::sync::Semaphore` and
//! returns an error immediately when the cap is exceeded (no queueing).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fraiseql_error::{FraiseQLError, Result};

/// Default maximum concurrent invocations per function.
pub const DEFAULT_MAX_CONCURRENT: u32 = 10;

/// Per-function concurrency gate backed by a `tokio::sync::Semaphore`.
///
/// Acquire a permit with [`ConcurrencyLimiter::acquire`] before invoking a
/// function and drop it when the invocation completes.
#[derive(Debug, Clone)]
pub struct ConcurrencyLimiter {
    semaphore: Arc<tokio::sync::Semaphore>,
    max_concurrent: u32,
}

impl ConcurrencyLimiter {
    /// Create a new limiter that allows at most `max_concurrent` simultaneous invocations.
    #[must_use]
    pub fn new(max_concurrent: u32) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            max_concurrent,
        }
    }

    /// Attempt to acquire a concurrency permit without blocking.
    ///
    /// Returns `Ok(permit)` if a slot is available, or
    /// `Err(FraiseQLError::Validation)` when at capacity.
    ///
    /// Drop the returned [`tokio::sync::SemaphorePermit`] to release the slot.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the concurrency cap has been reached.
    pub fn try_acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>> {
        self.semaphore.try_acquire().map_err(|_| FraiseQLError::Validation {
            message: format!(
                "concurrency limit reached: maximum {} simultaneous invocations",
                self.max_concurrent
            ),
            path: None,
        })
    }

    /// Return the configured maximum concurrency.
    #[must_use]
    pub const fn max_concurrent(&self) -> u32 {
        self.max_concurrent
    }

    /// Return the number of currently available permits.
    #[must_use]
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// Registry of per-function concurrency limiters.
///
/// Limiters are created on first use and reused across invocations.
/// Thread-safe via an interior `Mutex`.
#[derive(Debug, Clone, Default)]
pub struct ConcurrencyLimiterRegistry {
    limiters: Arc<Mutex<HashMap<String, Arc<ConcurrencyLimiter>>>>,
    default_max_concurrent: u32,
}

impl ConcurrencyLimiterRegistry {
    /// Create a new registry with the given default concurrency cap.
    #[must_use]
    pub fn new(default_max_concurrent: u32) -> Self {
        Self {
            limiters: Arc::new(Mutex::new(HashMap::new())),
            default_max_concurrent,
        }
    }

    /// Create a registry with the default concurrency cap of [`DEFAULT_MAX_CONCURRENT`].
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }

    /// Get or create the concurrency limiter for a function.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned (should never happen in normal operation).
    pub fn get_or_create(&self, function_name: &str) -> Arc<ConcurrencyLimiter> {
        let mut map = self.limiters.lock().expect("concurrency registry mutex poisoned");
        map.entry(function_name.to_string())
            .or_insert_with(|| Arc::new(ConcurrencyLimiter::new(self.default_max_concurrent)))
            .clone()
    }

    /// Register a function with a custom concurrency limit.
    ///
    /// Overwrites any existing limiter for this function name.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn register(&self, function_name: &str, max_concurrent: u32) {
        let mut map = self.limiters.lock().expect("concurrency registry mutex poisoned");
        map.insert(
            function_name.to_string(),
            Arc::new(ConcurrencyLimiter::new(max_concurrent)),
        );
    }
}

#[cfg(test)]
mod tests;
