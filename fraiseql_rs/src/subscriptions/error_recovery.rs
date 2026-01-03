//! Error recovery and fallback logic for subscriptions
//!
//! Implements resilience patterns including exponential backoff retry,
//! circuit breaker, and graceful degradation to fallback services.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Initial backoff duration
    pub initial_backoff: Duration,

    /// Maximum backoff duration
    pub max_backoff: Duration,

    /// Backoff multiplier (for exponential backoff)
    pub backoff_multiplier: f64,

    /// Jitter to add to backoff
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed (operating normally)
    Closed,

    /// Circuit is open (failing fast)
    Open,

    /// Circuit is half-open (testing recovery)
    HalfOpen,
}

/// Circuit breaker for fault tolerance
pub struct CircuitBreaker {
    /// Current circuit state
    state: Arc<tokio::sync::Mutex<CircuitState>>,

    /// Failure count in closed state
    failure_count: Arc<AtomicU32>,

    /// Last failure time
    last_failure_time: Arc<tokio::sync::Mutex<Option<Instant>>>,

    /// Failure threshold before opening circuit
    failure_threshold: u32,

    /// Timeout before attempting to half-open
    timeout: Duration,

    /// Total failures tracked
    total_failures: Arc<AtomicU64>,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(failure_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(tokio::sync::Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU32::new(0)),
            last_failure_time: Arc::new(tokio::sync::Mutex::new(None)),
            failure_threshold,
            timeout,
            total_failures: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a failure
    pub async fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        self.total_failures.fetch_add(1, Ordering::Relaxed);
        *self.last_failure_time.lock().await = Some(Instant::now());

        let current_state = *self.state.lock().await;

        // Open circuit if failures exceed threshold
        if current_state == CircuitState::Closed
            && self.failure_count.load(Ordering::Relaxed) >= self.failure_threshold
        {
            *self.state.lock().await = CircuitState::Open;
        }
    }

    /// Record a success
    pub async fn record_success(&self) {
        let current_state = *self.state.lock().await;

        if current_state == CircuitState::HalfOpen {
            // Full recovery after successful test
            *self.state.lock().await = CircuitState::Closed;
            self.failure_count.store(0, Ordering::Relaxed);
        }
    }

    /// Check if circuit is available (can attempt request)
    pub async fn can_attempt(&self) -> bool {
        let current_state = *self.state.lock().await;

        match current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed to try half-open
                if let Some(last_failure) = *self.last_failure_time.lock().await {
                    if Instant::now() - last_failure >= self.timeout {
                        // Allow transition to half-open
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        *self.state.lock().await
    }

    /// Get failure statistics
    pub async fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.lock().await,
            current_failures: self.failure_count.load(Ordering::Relaxed),
            total_failures: self.total_failures.load(Ordering::Relaxed),
        }
    }

    /// Reset circuit breaker
    pub async fn reset(&self) {
        *self.state.lock().await = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        *self.last_failure_time.lock().await = None;
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub current_failures: u32,
    pub total_failures: u64,
}

/// Fallback service registry
pub struct FallbackRegistry {
    /// Map of service -> fallback service
    fallbacks: Arc<dashmap::DashMap<String, String>>,

    /// Availability of fallback services
    availability: Arc<dashmap::DashMap<String, bool>>,
}

impl FallbackRegistry {
    /// Create new fallback registry
    pub fn new() -> Self {
        Self {
            fallbacks: Arc::new(dashmap::DashMap::new()),
            availability: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Register a fallback service
    pub fn register_fallback(&self, service: &str, fallback: &str) {
        self.fallbacks
            .insert(service.to_string(), fallback.to_string());
        self.availability.insert(fallback.to_string(), true);
    }

    /// Get fallback for service
    pub fn get_fallback(&self, service: &str) -> Option<String> {
        self.fallbacks.get(service).map(|entry| entry.clone())
    }

    /// Mark service as unavailable
    pub fn mark_unavailable(&self, service: &str) {
        self.availability.insert(service.to_string(), false);
    }

    /// Mark service as available
    pub fn mark_available(&self, service: &str) {
        self.availability.insert(service.to_string(), true);
    }

    /// Check if service is available
    pub fn is_available(&self, service: &str) -> bool {
        self.availability
            .get(service)
            .map(|entry| *entry)
            .unwrap_or(true) // Default to available if not registered
    }

    /// Get available fallback
    pub fn get_available_fallback(&self, service: &str) -> Option<String> {
        if let Some(fallback) = self.get_fallback(service) {
            if self.is_available(&fallback) {
                return Some(fallback);
            }
        }
        None
    }
}

impl Default for FallbackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Recovery strategy for handling errors
pub struct RecoveryStrategy {
    /// Retry configuration
    retry_config: Arc<RetryConfig>,

    /// Circuit breaker
    circuit_breaker: Arc<CircuitBreaker>,

    /// Fallback registry
    fallbacks: Arc<FallbackRegistry>,

    /// Total recovery attempts
    recovery_attempts: Arc<AtomicU64>,

    /// Successful recoveries
    successful_recoveries: Arc<AtomicU64>,
}

impl RecoveryStrategy {
    /// Create new recovery strategy
    pub fn new(retry_config: RetryConfig) -> Self {
        Self {
            retry_config: Arc::new(retry_config),
            circuit_breaker: Arc::new(CircuitBreaker::new(5, Duration::from_secs(30))),
            fallbacks: Arc::new(FallbackRegistry::new()),
            recovery_attempts: Arc::new(AtomicU64::new(0)),
            successful_recoveries: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Calculate backoff duration for retry
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        let base_backoff = self.retry_config.initial_backoff.as_millis() as f64
            * self.retry_config.backoff_multiplier.powi(attempt as i32);

        let capped_backoff = base_backoff.min(self.retry_config.max_backoff.as_millis() as f64);

        // Add jitter
        let jitter = capped_backoff * self.retry_config.jitter_factor;
        let jittered = capped_backoff + (jitter * 0.5); // Random between +/- jitter/2

        Duration::from_millis(jittered as u64)
    }

    /// Check if should retry
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.retry_config.max_retries
    }

    /// Record recovery attempt
    pub fn record_attempt(&self) {
        self.recovery_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record successful recovery
    pub fn record_success(&self) {
        self.successful_recoveries.fetch_add(1, Ordering::Relaxed);
    }

    /// Get circuit breaker
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    /// Get fallback registry
    pub fn fallbacks(&self) -> &FallbackRegistry {
        &self.fallbacks
    }

    /// Get recovery statistics
    pub fn stats(&self) -> RecoveryStats {
        RecoveryStats {
            total_attempts: self.recovery_attempts.load(Ordering::Relaxed),
            successful_recoveries: self.successful_recoveries.load(Ordering::Relaxed),
            success_rate: self.calculate_success_rate(),
        }
    }

    fn calculate_success_rate(&self) -> f64 {
        let total = self.recovery_attempts.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let successful = self.successful_recoveries.load(Ordering::Relaxed);
        (successful as f64 / total as f64) * 100.0
    }
}

impl Default for RecoveryStrategy {
    fn default() -> Self {
        Self::new(RetryConfig::default())
    }
}

/// Recovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStats {
    pub total_attempts: u64,
    pub successful_recoveries: u64,
    pub success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_circuit_breaker_creation() {
        let breaker = CircuitBreaker::new(5, Duration::from_secs(10));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert_eq!(breaker.state().await, CircuitState::Closed);
        });
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure_threshold() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(10));

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(10));
        breaker.record_failure().await;
        breaker.record_failure().await;

        assert_eq!(breaker.state().await, CircuitState::Open);

        breaker.record_success().await;
        // Half-open doesn't transition back without success in half-open state
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[test]
    fn test_fallback_registry() {
        let registry = FallbackRegistry::new();
        registry.register_fallback("redis", "postgresql");

        assert_eq!(
            registry.get_fallback("redis"),
            Some("postgresql".to_string())
        );
    }

    #[test]
    fn test_fallback_availability() {
        let registry = FallbackRegistry::new();
        registry.register_fallback("redis", "postgresql");

        assert!(registry.is_available("postgresql"));

        registry.mark_unavailable("postgresql");
        assert!(!registry.is_available("postgresql"));

        registry.mark_available("postgresql");
        assert!(registry.is_available("postgresql"));
    }

    #[test]
    fn test_recovery_strategy_backoff() {
        let strategy = RecoveryStrategy::new(RetryConfig::default());
        let backoff1 = strategy.calculate_backoff(0);
        let backoff2 = strategy.calculate_backoff(1);

        // Backoff should increase with attempt
        assert!(backoff2 >= backoff1);
    }

    #[test]
    fn test_recovery_strategy_should_retry() {
        let strategy = RecoveryStrategy::new(RetryConfig::default());
        assert!(strategy.should_retry(0));
        assert!(strategy.should_retry(1));
        assert!(strategy.should_retry(2));
        assert!(!strategy.should_retry(3));
    }

    #[test]
    fn test_recovery_stats() {
        let strategy = RecoveryStrategy::new(RetryConfig::default());
        strategy.record_attempt();
        strategy.record_success();

        let stats = strategy.stats();
        assert_eq!(stats.total_attempts, 1);
        assert_eq!(stats.successful_recoveries, 1);
        assert_eq!(stats.success_rate, 100.0);
    }
}
