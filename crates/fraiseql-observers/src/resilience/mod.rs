//! Circuit breaker pattern for resilience and graceful degradation.
//!
//! This module implements the circuit breaker pattern to protect the system
//! from cascading failures. The circuit breaker has three states:
//!
//! - **Closed**: Normal operation, all requests pass through
//! - **Open**: Failures detected, requests fail fast
//! - **`HalfOpen`**: Recovery testing, limited requests allowed
//!
//! # Example
//!
//! ```no_run
//! // Requires: tokio async runtime; replace external_service() with a real async call.
//! use fraiseql_observers::resilience::{CircuitBreaker, CircuitBreakerConfig};
//!
//! # async fn example() -> fraiseql_observers::Result<()> {
//! async fn external_service() -> fraiseql_observers::Result<()> { Ok(()) }
//!
//! let config = CircuitBreakerConfig::default();
//! let breaker = CircuitBreaker::new(config);
//!
//! let result = breaker.call(|| {
//!     Box::pin(async { external_service().await })
//! }).await?;
//! # Ok(())
//! # }
//! ```

pub mod degradation;
pub mod per_endpoint;
pub mod strategies;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

pub use degradation::{DegradationLevel, GracefulDegradation};
pub use per_endpoint::PerEndpointCircuitBreaker;
pub use strategies::{ResilienceStrategy, ResilientExecutor};
use tokio::sync::Mutex;

use crate::error::{ObserverError, Result};

/// Circuit breaker state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CircuitState {
    /// Normal operation, requests pass through
    Closed,
    /// Failures detected, fast fail mode
    Open,
    /// Recovery testing with limited requests
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
            Self::Open => write!(f, "open"),
            Self::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure rate threshold (0.0-1.0) to trigger open state
    pub failure_threshold: f64,
    /// Number of requests to sample for failure rate calculation
    pub sample_size: usize,
    /// Timeout from Open to `HalfOpen` transition (milliseconds)
    pub open_timeout_ms: u64,
    /// Maximum requests allowed in `HalfOpen` state
    pub half_open_max_requests: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 0.5,    // 50% failure rate threshold
            sample_size: 100,          // Sample last 100 requests
            open_timeout_ms: 30000,    // 30 seconds before HalfOpen
            half_open_max_requests: 5, // Test up to 5 requests
        }
    }
}

/// Circuit breaker state machine implementation
#[derive(Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<CircuitState>>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    half_open_requests: Arc<AtomicUsize>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    #[must_use]
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(Mutex::new(None)),
            half_open_requests: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Execute a function through the circuit breaker
    ///
    /// # Arguments
    ///
    /// * `f` - Async function to execute
    ///
    /// # Returns
    ///
    /// Returns the result of the function if executed, or an error if the circuit is open
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::CircuitBreakerOpen`] if the circuit is open.
    /// Propagates any error returned by `f`.
    pub async fn call<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        let state = self.get_state().await;

        match state {
            CircuitState::Closed => {
                // Attempt the call in closed state
                match f().await {
                    Ok(result) => {
                        self.record_success();
                        Ok(result)
                    },
                    Err(e) => {
                        self.record_failure().await;
                        Err(e)
                    },
                }
            },
            CircuitState::Open => {
                // Fail fast in open state
                Err(ObserverError::CircuitBreakerOpen {
                    message: "Circuit breaker is open".to_string(),
                })
            },
            CircuitState::HalfOpen => {
                // Allow limited requests in half-open state
                let permits = self.half_open_requests.load(Ordering::SeqCst);
                if permits >= self.config.half_open_max_requests {
                    return Err(ObserverError::CircuitBreakerOpen {
                        message: "Half-open circuit at max requests".to_string(),
                    });
                }

                self.half_open_requests.fetch_add(1, Ordering::SeqCst);

                match f().await {
                    Ok(result) => {
                        self.record_success();
                        Ok(result)
                    },
                    Err(e) => {
                        self.record_failure().await;
                        Err(e)
                    },
                }
            },
        }
    }

    /// Get the current circuit state (with state transition logic)
    pub async fn get_state(&self) -> CircuitState {
        let mut state = self.state.lock().await;
        let current_state = *state;

        // Check if we should transition from Open to HalfOpen
        if current_state == CircuitState::Open {
            let last_failure = *self.last_failure_time.lock().await;
            if let Some(failure_time) = last_failure {
                #[allow(clippy::cast_possible_truncation)]
                // Reason: circuit breaker timeouts are short, millis fit in u64
                let elapsed = failure_time.elapsed().as_millis() as u64;
                if elapsed >= self.config.open_timeout_ms {
                    // Transition to HalfOpen for recovery attempt
                    *state = CircuitState::HalfOpen;
                    self.half_open_requests.store(0, Ordering::SeqCst);
                    return CircuitState::HalfOpen;
                }
            }
        }

        // Check if we should transition from HalfOpen to Closed
        if current_state == CircuitState::HalfOpen {
            let failure_rate = self.calculate_failure_rate();
            if failure_rate <= self.config.failure_threshold {
                // Recovery successful, back to closed
                *state = CircuitState::Closed;
                self.reset_counters();
                return CircuitState::Closed;
            } else if self.half_open_requests.load(Ordering::SeqCst)
                >= self.config.half_open_max_requests
            {
                // Recovery failed, back to open
                *state = CircuitState::Open;
                return CircuitState::Open;
            }
        }

        *state
    }

    /// Record a successful request
    fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a failed request
    async fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        *self.last_failure_time.lock().await = Some(Instant::now());

        // Check if we should transition from Closed to Open
        let mut state = self.state.lock().await;
        if *state == CircuitState::Closed {
            let failure_rate = self.calculate_failure_rate();
            if failure_rate > self.config.failure_threshold {
                // Threshold exceeded, open the circuit
                *state = CircuitState::Open;
            }
        }
    }

    /// Calculate current failure rate
    #[allow(clippy::cast_precision_loss)] // Reason: f64 precision is acceptable for metrics counters
    fn calculate_failure_rate(&self) -> f64 {
        let failures = self.failure_count.load(Ordering::SeqCst) as f64;
        let successes = self.success_count.load(Ordering::SeqCst) as f64;
        let total = failures + successes;

        if total < self.config.sample_size as f64 {
            // Not enough samples yet, be lenient
            0.0
        } else {
            failures / total
        }
    }

    /// Reset all counters
    fn reset_counters(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_creation() {
        let config = CircuitBreakerConfig::default();
        let breaker = CircuitBreaker::new(config);

        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0.5,
            sample_size: 10,
            open_timeout_ms: 1000,
            half_open_max_requests: 3,
        };
        let breaker = CircuitBreaker::new(config);

        // Call should succeed in closed state
        let result = breaker.call(|| Box::pin(async { Ok::<i32, ObserverError>(42) })).await;

        let val = result.unwrap_or_else(|e| panic!("expected Ok in closed state: {e}"));
        assert_eq!(val, 42);
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure_transition() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0.5,
            sample_size: 3,
            open_timeout_ms: 1000,
            half_open_max_requests: 3,
        };
        let breaker = CircuitBreaker::new(config);

        // Record 3 failures (100% failure rate with sample_size=3)
        for _ in 0..3 {
            let _ = breaker
                .call(|| {
                    Box::pin(async {
                        Err::<i32, _>(ObserverError::ActionExecutionFailed {
                            reason: "test".to_string(),
                        })
                    })
                })
                .await;
        }

        // Circuit should now be open
        let state = breaker.get_state().await;
        assert_eq!(state, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_state_fails_fast() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0.1,
            sample_size: 1,
            open_timeout_ms: 10000,
            half_open_max_requests: 3,
        };
        let breaker = CircuitBreaker::new(config);

        // Trigger open state
        let _ = breaker
            .call(|| {
                Box::pin(async {
                    Err::<i32, _>(ObserverError::ActionExecutionFailed {
                        reason: "test".to_string(),
                    })
                })
            })
            .await;

        // Should fail fast in open state
        let result = breaker.call(|| Box::pin(async { Ok::<i32, _>(42) })).await;

        assert!(
            matches!(result, Err(ObserverError::CircuitBreakerOpen { .. })),
            "open circuit must fail with CircuitBreakerOpen, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_limited_requests() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0.1,
            sample_size: 1,
            open_timeout_ms: 100,
            half_open_max_requests: 2,
        };
        let breaker = CircuitBreaker::new(config);

        // Trigger open state
        let _ = breaker
            .call(|| {
                Box::pin(async {
                    Err::<i32, _>(ObserverError::ActionExecutionFailed {
                        reason: "test".to_string(),
                    })
                })
            })
            .await;

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow limited requests in half-open
        let result1 = breaker.call(|| Box::pin(async { Ok::<i32, _>(1) })).await;
        result1.unwrap_or_else(|e| panic!("expected Ok for first half-open request: {e}"));

        let result2 = breaker.call(|| Box::pin(async { Ok::<i32, _>(2) })).await;
        result2.unwrap_or_else(|e| panic!("expected Ok for second half-open request: {e}"));

        // Third request should fail (half_open_max_requests=2 exceeded)
        let result3 = breaker.call(|| Box::pin(async { Ok::<i32, _>(3) })).await;
        assert!(
            matches!(result3, Err(ObserverError::CircuitBreakerOpen { .. })),
            "third half-open request must fail with CircuitBreakerOpen, got: {result3:?}"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_circuit_breaker_config_defaults() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 0.5);
        assert_eq!(config.sample_size, 100);
        assert_eq!(config.open_timeout_ms, 30000);
        assert_eq!(config.half_open_max_requests, 5);
    }

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(CircuitState::Closed.to_string(), "closed");
        assert_eq!(CircuitState::Open.to_string(), "open");
        assert_eq!(CircuitState::HalfOpen.to_string(), "half-open");
    }
}
