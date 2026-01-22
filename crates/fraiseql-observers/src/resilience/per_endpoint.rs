//! Per-endpoint circuit breaker management.
//!
//! Manages separate circuit breakers for each external endpoint,
//! providing isolation and independent failure handling.

use super::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use crate::error::Result;
use dashmap::DashMap;
use std::sync::Arc;

/// Manages circuit breakers per endpoint
#[derive(Clone)]
pub struct PerEndpointCircuitBreaker {
    breakers: Arc<DashMap<String, Arc<CircuitBreaker>>>,
    default_config: CircuitBreakerConfig,
}

impl PerEndpointCircuitBreaker {
    /// Create a new per-endpoint breaker manager
    #[must_use] 
    pub fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(DashMap::new()),
            default_config,
        }
    }

    /// Get or create a circuit breaker for an endpoint
    #[must_use] 
    pub fn get_or_create(&self, endpoint: &str) -> Arc<CircuitBreaker> {
        self.breakers
            .entry(endpoint.to_string())
            .or_insert_with(|| Arc::new(CircuitBreaker::new(self.default_config.clone())))
            .clone()
    }

    /// Execute a call through the appropriate endpoint breaker
    pub async fn call<F, T>(&self, endpoint: &str, f: F) -> Result<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        let breaker = self.get_or_create(endpoint);
        breaker.call(f).await
    }

    /// Reset a specific endpoint breaker
    pub fn reset_endpoint(&self, endpoint: &str) {
        self.breakers.remove(endpoint);
    }

    /// Reset all endpoint breakers
    pub fn reset_all(&self) {
        self.breakers.clear();
    }

    /// Get state of all managed breakers
    pub async fn get_all_states(&self) -> Vec<(String, CircuitState)> {
        let mut states = Vec::new();
        for entry in self.breakers.iter() {
            let endpoint = entry.key().clone();
            let breaker = entry.value().clone();
            let state = breaker.get_state().await;
            states.push((endpoint, state));
        }
        states
    }

    /// Get number of managed endpoints
    #[must_use] 
    pub fn endpoint_count(&self) -> usize {
        self.breakers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
