//! Chaos engineering utilities for subscriptions module
//!
//! Provides chaos controllers for orchestrating failures, circuit breaker patterns,
//! and recovery scenarios for comprehensive resilience testing.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Controls chaos scenarios - simulates infrastructure failures
#[derive(Debug, Clone)]
pub struct ChaosController {
    /// Is Redis unavailable
    redis_down: Arc<AtomicBool>,
    /// Is PostgreSQL unavailable
    postgres_down: Arc<AtomicBool>,
    /// Percentage of requests to fail (0-100)
    failure_percentage: Arc<AtomicU32>,
    /// Number of times chaos was triggered
    trigger_count: Arc<AtomicU32>,
    /// Time chaos started
    start_time: Arc<Mutex<Option<Instant>>>,
}

impl ChaosController {
    /// Create a new chaos controller
    pub fn new() -> Self {
        Self {
            redis_down: Arc::new(AtomicBool::new(false)),
            postgres_down: Arc::new(AtomicBool::new(false)),
            failure_percentage: Arc::new(AtomicU32::new(0)),
            trigger_count: Arc::new(AtomicU32::new(0)),
            start_time: Arc::new(Mutex::new(None)),
        }
    }

    /// Make Redis unavailable
    pub fn fail_redis(&self) {
        self.redis_down.store(true, Ordering::SeqCst);
        self.trigger_count.fetch_add(1, Ordering::Relaxed);
        self.mark_start();
    }

    /// Restore Redis availability
    pub fn restore_redis(&self) {
        self.redis_down.store(false, Ordering::SeqCst);
    }

    /// Make PostgreSQL unavailable
    pub fn fail_postgres(&self) {
        self.postgres_down.store(true, Ordering::SeqCst);
        self.trigger_count.fetch_add(1, Ordering::Relaxed);
        self.mark_start();
    }

    /// Restore PostgreSQL availability
    pub fn restore_postgres(&self) {
        self.postgres_down.store(false, Ordering::SeqCst);
    }

    /// Check if Redis is unavailable
    pub fn is_redis_down(&self) -> bool {
        self.redis_down.load(Ordering::SeqCst)
    }

    /// Check if PostgreSQL is unavailable
    pub fn is_postgres_down(&self) -> bool {
        self.postgres_down.load(Ordering::SeqCst)
    }

    /// Set failure injection percentage (0-100)
    pub fn set_failure_percentage(&self, percentage: u32) {
        let clamped = percentage.min(100);
        self.failure_percentage.store(clamped, Ordering::SeqCst);
        if clamped > 0 {
            self.trigger_count.fetch_add(1, Ordering::Relaxed);
            self.mark_start();
        }
    }

    /// Check if this operation should fail based on percentage
    pub fn should_fail(&self) -> bool {
        let percentage = self.failure_percentage.load(Ordering::SeqCst);
        if percentage == 0 {
            return false;
        }
        (rand::random::<u32>() % 100) < percentage
    }

    /// Get trigger count
    pub fn trigger_count(&self) -> u32 {
        self.trigger_count.load(Ordering::Relaxed)
    }

    /// Get elapsed time since first trigger
    pub fn elapsed(&self) -> Option<Duration> {
        self.start_time
            .lock()
            .and_then(|start| Some(start.elapsed()))
    }

    /// Reset all chaos state
    pub fn reset(&self) {
        self.redis_down.store(false, Ordering::SeqCst);
        self.postgres_down.store(false, Ordering::SeqCst);
        self.failure_percentage.store(0, Ordering::SeqCst);
        self.trigger_count.store(0, Ordering::Relaxed);
        *self.start_time.lock() = None;
    }

    /// Get description of current chaos state
    pub fn describe(&self) -> String {
        let mut parts = vec![];
        if self.is_redis_down() {
            parts.push("Redis DOWN".to_string());
        }
        if self.is_postgres_down() {
            parts.push("PostgreSQL DOWN".to_string());
        }
        let pct = self.failure_percentage.load(Ordering::SeqCst);
        if pct > 0 {
            parts.push(format!("{}% failures", pct));
        }
        if parts.is_empty() {
            "No chaos active".to_string()
        } else {
            parts.join(" + ")
        }
    }

    /// Mark when chaos started (internal)
    fn mark_start(&self) {
        let mut start = self.start_time.lock();
        if start.is_none() {
            *start = Some(Instant::now());
        }
    }
}

impl Default for ChaosController {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit breaker pattern implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (failures occurring, requests blocked)
    Open,
    /// Circuit is half-open (testing if system recovered)
    HalfOpen,
}

/// Circuit breaker for handling cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    failure_count: Arc<AtomicU32>,
    success_count: Arc<AtomicU32>,
    failure_threshold: u32,
    success_threshold: u32,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    timeout: Duration,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(AtomicU32::new(0)),
            failure_threshold,
            success_threshold,
            last_failure_time: Arc::new(Mutex::new(None)),
            timeout,
        }
    }

    /// Check if request should be allowed
    pub fn can_execute(&self) -> bool {
        let state = *self.state.lock();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.lock() {
                    if last_failure.elapsed() >= self.timeout {
                        // Try half-open
                        *self.state.lock() = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::Relaxed);
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful operation
    pub fn record_success(&self) {
        let mut state = self.state.lock();
        match *state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                self.success_count.fetch_add(1, Ordering::Relaxed);
                if self.success_count.load(Ordering::Relaxed) >= self.success_threshold {
                    *state = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        *self.last_failure_time.lock() = Some(Instant::now());
        let mut state = self.state.lock();
        match *state {
            CircuitState::Closed => {
                self.failure_count.fetch_add(1, Ordering::Relaxed);
                if self.failure_count.load(Ordering::Relaxed) >= self.failure_threshold {
                    *state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                *state = CircuitState::Open;
                self.success_count.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {
                self.failure_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        *self.state.lock()
    }

    /// Reset circuit breaker
    pub fn reset(&self) {
        let mut state = self.state.lock();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        *self.last_failure_time.lock() = None;
    }

    /// Get statistics
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: self.state(),
            failures: self.failure_count.load(Ordering::Relaxed),
            successes: self.success_count.load(Ordering::Relaxed),
            failure_threshold: self.failure_threshold,
            success_threshold: self.success_threshold,
        }
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            failure_count: Arc::clone(&self.failure_count),
            success_count: Arc::clone(&self.success_count),
            failure_threshold: self.failure_threshold,
            success_threshold: self.success_threshold,
            last_failure_time: Arc::clone(&self.last_failure_time),
            timeout: self.timeout,
        }
    }
}

/// Statistics from circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failures: u32,
    pub successes: u32,
    pub failure_threshold: u32,
    pub success_threshold: u32,
}

impl CircuitBreakerStats {
    /// Print formatted stats
    pub fn print(&self) {
        println!(
            "  Circuit: {:?} | Failures: {}/{} | Successes: {}/{}",
            self.state,
            self.failures,
            self.failure_threshold,
            self.successes,
            self.success_threshold
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_controller_redis_failure() {
        let chaos = ChaosController::new();
        assert!(!chaos.is_redis_down());
        chaos.fail_redis();
        assert!(chaos.is_redis_down());
        chaos.restore_redis();
        assert!(!chaos.is_redis_down());
    }

    #[test]
    fn test_chaos_controller_postgres_failure() {
        let chaos = ChaosController::new();
        assert!(!chaos.is_postgres_down());
        chaos.fail_postgres();
        assert!(chaos.is_postgres_down());
        chaos.restore_postgres();
        assert!(!chaos.is_postgres_down());
    }

    #[test]
    fn test_chaos_controller_failure_percentage() {
        let chaos = ChaosController::new();
        chaos.set_failure_percentage(50);
        assert_eq!(chaos.failure_percentage.load(Ordering::SeqCst), 50);

        // Clamping
        chaos.set_failure_percentage(150);
        assert_eq!(chaos.failure_percentage.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn test_chaos_controller_reset() {
        let chaos = ChaosController::new();
        chaos.fail_redis();
        chaos.fail_postgres();
        chaos.set_failure_percentage(50);

        assert!(chaos.is_redis_down());
        assert!(chaos.is_postgres_down());

        chaos.reset();
        assert!(!chaos.is_redis_down());
        assert!(!chaos.is_postgres_down());
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_secs(1));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_secs(1));
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_circuit_breaker_half_open_recovery() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_millis(10));
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let cb = CircuitBreaker::new(3, 2, Duration::from_secs(1));
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
