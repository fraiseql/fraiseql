//! Resilience strategies for different failure handling approaches.
//!
//! Provides different strategies for handling failures:
//! - `FailFast`: Immediately fail when circuit is open
//! - Fallback: Return a default value on failure
//! - `RetryWithBreaker`: Retry with circuit breaker protection

use std::sync::Arc;

use super::CircuitBreaker;
#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::ObserverError;
use crate::error::Result;

/// Different resilience strategies for failure handling
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ResilienceStrategy {
    /// Fast fail when circuit is open
    FailFast,
    /// Use fallback value when circuit is open
    Fallback(String),
    /// Retry with circuit breaker
    RetryWithBreaker {
        /// Maximum retry attempts
        max_attempts: u32,
        /// Backoff in milliseconds
        backoff_ms:   u64,
    },
}

/// Executor with resilience strategy
pub struct ResilientExecutor {
    circuit_breaker: Arc<CircuitBreaker>,
    strategy:        ResilienceStrategy,
}

impl ResilientExecutor {
    /// Create a new resilient executor
    #[must_use]
    pub const fn new(circuit_breaker: Arc<CircuitBreaker>, strategy: ResilienceStrategy) -> Self {
        Self {
            circuit_breaker,
            strategy,
        }
    }

    /// Execute with resilience
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::CircuitBreakerOpen`] when the circuit is open.
    /// Propagates errors from `f`; after exhausting retries under
    /// `ResilienceStrategy::RetryWithBreaker`, returns the last error.
    #[allow(clippy::future_not_send)] // Reason: single-threaded observer context
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>> + 'static,
        T: 'static,
    {
        match &self.strategy {
            ResilienceStrategy::FailFast => self.circuit_breaker.call(|| Box::pin(f())).await,
            ResilienceStrategy::Fallback(_) => {
                // Call through circuit breaker, will return error if open
                self.circuit_breaker.call(|| Box::pin(f())).await
            },
            ResilienceStrategy::RetryWithBreaker {
                max_attempts,
                backoff_ms,
            } => {
                let mut attempt = 0;
                loop {
                    attempt += 1;

                    match self.circuit_breaker.call(|| Box::pin(f())).await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            if attempt >= *max_attempts {
                                return Err(e);
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(*backoff_ms)).await;
                        },
                    }
                }
            },
        }
    }
}

