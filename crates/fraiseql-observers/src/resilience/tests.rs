#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod resilience_mod_tests {
    use std::time::Duration;

    use super::super::*;
    use crate::error::ObserverError;

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

    #[tokio::test(start_paused = true)]
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

        // Advance frozen time past the timeout
        tokio::time::advance(Duration::from_millis(150)).await;

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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod degradation_tests {
    use std::sync::Arc;

    use super::super::{CircuitBreaker, CircuitBreakerConfig, degradation::*};

    #[tokio::test]
    async fn test_degradation_creation() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        assert!(!degradation.is_degraded());
        assert!(degradation.is_enabled());
    }

    #[tokio::test]
    async fn test_degradation_level_normal() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        let level = degradation.get_degradation_level().await;
        assert_eq!(level, DegradationLevel::Normal);
    }

    #[tokio::test]
    async fn test_degradation_disabled() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        degradation.set_enabled(false);
        let level = degradation.get_degradation_level().await;

        assert_eq!(level, DegradationLevel::Critical);
    }

    #[tokio::test]
    async fn test_degradation_with_execution() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        let result = degradation
            .with_degradation(|level| {
                Box::pin(async move {
                    if level == DegradationLevel::Normal {
                        Ok(42)
                    } else {
                        Ok(0)
                    }
                })
            })
            .await;

        assert_eq!(result.ok(), Some(42));
    }

    #[test]
    fn test_degradation_level_display() {
        assert_eq!(DegradationLevel::Normal.to_string(), "normal");
        assert_eq!(DegradationLevel::Degraded.to_string(), "degraded");
        assert_eq!(DegradationLevel::Critical.to_string(), "critical");
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod per_endpoint_tests {
    use super::super::{CircuitBreakerConfig, CircuitState, per_endpoint::*};
    use crate::error::ObserverError;

    #[tokio::test]
    async fn test_per_endpoint_creation() {
        let config = CircuitBreakerConfig::default();
        let manager = PerEndpointCircuitBreaker::new(config);

        assert_eq!(manager.endpoint_count(), 0);
    }

    #[tokio::test]
    async fn test_per_endpoint_independent_breakers() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0.1,
            sample_size: 2,
            open_timeout_ms: 1000,
            half_open_max_requests: 3,
        };
        let manager = PerEndpointCircuitBreaker::new(config);

        // Trigger failure on endpoint 1
        for _ in 0..2 {
            let _ = manager
                .call("endpoint1", || {
                    Box::pin(async {
                        Err::<i32, _>(ObserverError::ActionExecutionFailed {
                            reason: "test".to_string(),
                        })
                    })
                })
                .await;
        }

        // Endpoint 1 should be open
        let breaker1 = manager.get_or_create("endpoint1");
        assert_eq!(breaker1.get_state().await, CircuitState::Open);

        // Endpoint 2 should still be closed
        let breaker2 = manager.get_or_create("endpoint2");
        assert_eq!(breaker2.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_per_endpoint_reset() {
        let config = CircuitBreakerConfig::default();
        let manager = PerEndpointCircuitBreaker::new(config);

        // Create some endpoints
        let _ = manager.get_or_create("endpoint1");
        let _ = manager.get_or_create("endpoint2");

        assert_eq!(manager.endpoint_count(), 2);

        // Reset one endpoint
        manager.reset_endpoint("endpoint1");
        assert_eq!(manager.endpoint_count(), 1);

        // Reset all
        manager.reset_all();
        assert_eq!(manager.endpoint_count(), 0);
    }

    #[tokio::test]
    async fn test_per_endpoint_state_retrieval() {
        let config = CircuitBreakerConfig::default();
        let manager = PerEndpointCircuitBreaker::new(config);

        let _ = manager.get_or_create("endpoint1");
        let _ = manager.get_or_create("endpoint2");

        let states = manager.get_all_states().await;

        assert_eq!(states.len(), 2);
        for (_, state) in states {
            assert_eq!(state, CircuitState::Closed);
        }
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod strategies_tests {
    use std::sync::Arc;

    use super::super::{CircuitBreaker, CircuitBreakerConfig, strategies::*};

    #[tokio::test]
    async fn test_strategy_fail_fast() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor = ResilientExecutor::new(breaker, ResilienceStrategy::FailFast);

        let value = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await
            .expect("fail-fast strategy should succeed when closure returns Ok");
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn test_strategy_fallback() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor =
            ResilientExecutor::new(breaker, ResilienceStrategy::Fallback("default".to_string()));

        let value = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await
            .expect("fallback strategy should succeed when closure returns Ok");
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn test_strategy_retry_with_breaker() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let executor = ResilientExecutor::new(
            breaker,
            ResilienceStrategy::RetryWithBreaker {
                max_attempts: 3,
                backoff_ms: 10,
            },
        );

        let value = executor
            .execute(|| Box::pin(async { Ok::<i32, _>(42) }))
            .await
            .expect("retry strategy should succeed when closure returns Ok");
        assert_eq!(value, 42);
    }

    #[test]
    fn test_resilience_strategy_clone() {
        let strategy1 = ResilienceStrategy::FailFast;
        let strategy2 = strategy1;
        assert!(matches!(strategy2, ResilienceStrategy::FailFast));
    }

    #[test]
    fn test_resilience_strategy_display() {
        let strategy = ResilienceStrategy::Fallback("test".to_string());
        let debug_str = format!("{strategy:?}");
        assert!(debug_str.contains("Fallback"));
    }
}
