//! Graceful service degradation under failure conditions.
//!
//! Manages graceful degradation levels based on circuit breaker state,
//! allowing the system to operate in a reduced-capacity mode while
//! maintaining stability.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::{CircuitBreaker, CircuitState};
use crate::error::Result;

/// Degradation level indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationLevel {
    /// System operating normally
    Normal,
    /// System in reduced-capacity mode
    Degraded,
    /// System in critical state, minimal operations
    Critical,
}

impl std::fmt::Display for DegradationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Degraded => write!(f, "degraded"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Manages graceful degradation
#[derive(Clone)]
pub struct GracefulDegradation {
    circuit_breaker: Arc<CircuitBreaker>,
    enabled:         Arc<AtomicBool>,
    degraded_mode:   Arc<AtomicBool>,
}

impl GracefulDegradation {
    /// Create a new graceful degradation manager
    #[must_use]
    pub fn new(circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            circuit_breaker,
            enabled: Arc::new(AtomicBool::new(true)),
            degraded_mode: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if system is in degraded mode
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        self.degraded_mode.load(Ordering::Relaxed)
    }

    /// Get current degradation level
    pub async fn get_degradation_level(&self) -> DegradationLevel {
        if !self.enabled.load(Ordering::Relaxed) {
            return DegradationLevel::Critical;
        }

        let state = self.circuit_breaker.get_state().await;
        match state {
            CircuitState::Closed => DegradationLevel::Normal,
            CircuitState::HalfOpen => DegradationLevel::Degraded,
            CircuitState::Open => DegradationLevel::Critical,
        }
    }

    /// Execute with degradation awareness
    pub async fn with_degradation<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn(
            DegradationLevel,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    {
        let level = self.get_degradation_level().await;
        self.degraded_mode.store(level != DegradationLevel::Normal, Ordering::Relaxed);
        f(level).await
    }

    /// Enable or disable degradation management
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Check if degradation management is enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_degradation_creation() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        assert!(!degradation.is_degraded());
        assert!(degradation.is_enabled());
    }

    #[tokio::test]
    async fn test_degradation_level_normal() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        let level = degradation.get_degradation_level().await;
        assert_eq!(level, DegradationLevel::Normal);
    }

    #[tokio::test]
    async fn test_degradation_disabled() {
        let config = crate::resilience::CircuitBreakerConfig::default();
        let breaker = Arc::new(CircuitBreaker::new(config));
        let degradation = GracefulDegradation::new(breaker);

        degradation.set_enabled(false);
        let level = degradation.get_degradation_level().await;

        assert_eq!(level, DegradationLevel::Critical);
    }

    #[tokio::test]
    async fn test_degradation_with_execution() {
        let config = crate::resilience::CircuitBreakerConfig::default();
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
