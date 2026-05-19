//! Per-endpoint circuit breaker management.
//!
//! Manages separate circuit breakers for each external endpoint,
//! providing isolation and independent failure handling.

use std::sync::Arc;

use dashmap::DashMap;

use super::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::ObserverError;
use crate::error::Result;

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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::CircuitBreakerOpen`] if the circuit for `endpoint`
    /// is open. Propagates any error returned by `f`.
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
