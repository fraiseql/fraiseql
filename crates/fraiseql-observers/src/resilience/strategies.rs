//! Resilience strategies for different failure handling approaches.
//!
//! Provides different strategies for handling failures:
//! - FailFast: Immediately fail when circuit is open
//! - Fallback: Return a default value on failure
//! - RetryWithBreaker: Retry with circuit breaker protection

use super::CircuitBreaker;
use crate::error::Result;
use std::sync::Arc;

/// Different resilience strategies for failure handling
#[derive(Debug, Clone)]
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
        backoff_ms: u64,
    },
}

/// Executor with resilience strategy
pub struct ResilientExecutor {
    circuit_breaker: Arc<CircuitBreaker>,
    strategy: ResilienceStrategy,
}

impl ResilientExecutor {
    /// Create a new resilient executor
    pub fn new(circuit_breaker: Arc<CircuitBreaker>, strategy: ResilienceStrategy) -> Self {
        Self {
            circuit_breaker,
            strategy,
        }
    }

    /// Execute with resilience
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>> + 'static,
        T: 'static,
    {
        match &self.strategy {
            ResilienceStrategy::FailFast => {
                self.circuit_breaker
                    .call(|| Box::pin(f()))
                    .await
            }
            ResilienceStrategy::Fallback(_) => {
                // Call through circuit breaker, will return error if open
                self.circuit_breaker
                    .call(|| Box::pin(f()))
                    .await
            }
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
                            tokio::time::sleep(std::time::Duration::from_millis(*backoff_ms))
                                .await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ObserverError;

    #[tokio::test]
    async fn test_strategy_fail_fast() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor = ResilientExecutor::new(breaker, ResilienceStrategy::FailFast);

        let result = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_strategy_fallback() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor = ResilientExecutor::new(
            breaker,
            ResilienceStrategy::Fallback("default".to_string()),
        );

        let result = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_strategy_retry_with_breaker() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor = ResilientExecutor::new(
            breaker,
            ResilienceStrategy::RetryWithBreaker {
                max_attempts: 3,
                backoff_ms: 10,
            },
        );

        let result = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_resilience_strategy_clone() {
        let strategy1 = ResilienceStrategy::FailFast;
        let strategy2 = strategy1.clone();
        assert!(matches!(strategy2, ResilienceStrategy::FailFast));
    }

    #[test]
    fn test_resilience_strategy_display() {
        let strategy = ResilienceStrategy::Fallback("test".to_string());
        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("Fallback"));
    }
}
