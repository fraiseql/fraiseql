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
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn test_limiter_allows_up_to_max_concurrent() {
        let limiter = ConcurrencyLimiter::new(3);
        let _p1 = limiter.try_acquire().unwrap();
        let _p2 = limiter.try_acquire().unwrap();
        let _p3 = limiter.try_acquire().unwrap();

        // 4th attempt must be rejected
        assert!(limiter.try_acquire().is_err());
    }

    #[test]
    fn test_limiter_releases_permit_on_drop() {
        let limiter = ConcurrencyLimiter::new(1);

        {
            let _permit = limiter.try_acquire().unwrap();
            assert!(limiter.try_acquire().is_err()); // at capacity
        } // permit dropped here

        // Should be available again
        assert!(limiter.try_acquire().is_ok());
    }

    #[test]
    fn test_limiter_error_message_includes_limit() {
        let limiter = ConcurrencyLimiter::new(2);
        let _p1 = limiter.try_acquire().unwrap();
        let _p2 = limiter.try_acquire().unwrap();

        let err = limiter.try_acquire().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains('2') || msg.contains("concurrency"), "error: {msg}");
    }

    #[test]
    fn test_limiter_available_permits_tracks_usage() {
        let limiter = ConcurrencyLimiter::new(4);
        assert_eq!(limiter.available_permits(), 4);

        let _p1 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.available_permits(), 3);

        let _p2 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.available_permits(), 2);
    }

    #[test]
    fn test_registry_creates_limiters_on_demand() {
        let registry = ConcurrencyLimiterRegistry::new(5);
        let limiter = registry.get_or_create("my_function");
        assert_eq!(limiter.max_concurrent(), 5);
    }

    #[test]
    fn test_registry_reuses_existing_limiters() {
        let registry = ConcurrencyLimiterRegistry::new(3);
        let l1 = registry.get_or_create("fn_a");
        let l2 = registry.get_or_create("fn_a");

        // Same Arc pointer (same semaphore state)
        assert!(Arc::ptr_eq(&l1.semaphore, &l2.semaphore));
    }

    #[test]
    fn test_registry_isolates_different_functions() {
        let registry = ConcurrencyLimiterRegistry::new(1);
        let fn_a = registry.get_or_create("fn_a");
        let fn_b = registry.get_or_create("fn_b");

        let _permit_a = fn_a.try_acquire().unwrap();
        // fn_a is at capacity, but fn_b is independent
        assert!(fn_a.try_acquire().is_err());
        assert!(fn_b.try_acquire().is_ok());
    }

    #[test]
    fn test_registry_custom_per_function_limit() {
        let registry = ConcurrencyLimiterRegistry::new(10);
        registry.register("critical_fn", 2);

        let limiter = registry.get_or_create("critical_fn");
        assert_eq!(limiter.max_concurrent(), 2);
    }
}
