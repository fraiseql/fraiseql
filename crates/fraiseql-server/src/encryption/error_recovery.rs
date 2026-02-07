//! Error recovery for encryption operations including Vault outages,
//! key expiry, network partitions, and graceful degradation strategies.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use chrono::{DateTime, Duration, Utc};

/// Recovery strategy for encryption failures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Use cached value if available
    UseCache,
    /// Retry with exponential backoff
    Retry,
    /// Fail fast
    FailFast,
    /// Degrade to read-only mode
    ReadOnly,
}

impl std::fmt::Display for RecoveryStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UseCache => write!(f, "use_cache"),
            Self::Retry => write!(f, "retry"),
            Self::FailFast => write!(f, "fail_fast"),
            Self::ReadOnly => write!(f, "read_only"),
        }
    }
}

/// Error category for diagnostics and recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Network connectivity issue
    NetworkError,
    /// Vault service unavailable
    VaultUnavailable,
    /// Encryption key not found
    KeyNotFound,
    /// Encryption key expired
    KeyExpired,
    /// Permission denied
    PermissionDenied,
    /// Encryption operation failed
    EncryptionFailed,
    /// Decryption operation failed
    DecryptionFailed,
    /// Cache miss
    CacheMiss,
    /// Unknown error
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkError => write!(f, "network_error"),
            Self::VaultUnavailable => write!(f, "vault_unavailable"),
            Self::KeyNotFound => write!(f, "key_not_found"),
            Self::KeyExpired => write!(f, "key_expired"),
            Self::PermissionDenied => write!(f, "permission_denied"),
            Self::EncryptionFailed => write!(f, "encryption_failed"),
            Self::DecryptionFailed => write!(f, "decryption_failed"),
            Self::CacheMiss => write!(f, "cache_miss"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Error with context and recovery information
#[derive(Debug, Clone)]
pub struct RecoveryError {
    /// Error category
    pub category:    ErrorCategory,
    /// Human-readable message
    pub message:     String,
    /// Recovery strategy recommendation
    pub strategy:    RecoveryStrategy,
    /// Recovery suggestion for user
    pub suggestion:  String,
    /// Timestamp of error
    pub timestamp:   DateTime<Utc>,
    /// Retry count so far
    pub retry_count: u32,
    /// Can retry
    pub retryable:   bool,
}

impl RecoveryError {
    /// Create new recovery error
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        let suggestion = match category {
            ErrorCategory::NetworkError => "Check network connectivity and retry".to_string(),
            ErrorCategory::VaultUnavailable => "Vault is unavailable. Check Vault status and retry after 30s".to_string(),
            ErrorCategory::KeyNotFound => "Encryption key not found. Check key reference in configuration".to_string(),
            ErrorCategory::KeyExpired => "Encryption key has expired. Key will be refreshed automatically. Retry the operation".to_string(),
            ErrorCategory::PermissionDenied => "Permission denied accessing encryption key. Check authentication credentials".to_string(),
            ErrorCategory::EncryptionFailed => "Encryption operation failed. Check input data and retry".to_string(),
            ErrorCategory::DecryptionFailed => "Decryption operation failed. Data may be corrupted".to_string(),
            ErrorCategory::CacheMiss => "Key not in cache. Vault fetch will be attempted".to_string(),
            ErrorCategory::Unknown => "An unknown error occurred. Check logs for details".to_string(),
        };

        let (strategy, retryable) = match category {
            ErrorCategory::NetworkError => (RecoveryStrategy::Retry, true),
            ErrorCategory::VaultUnavailable => (RecoveryStrategy::UseCache, true),
            ErrorCategory::KeyExpired => (RecoveryStrategy::Retry, true),
            ErrorCategory::KeyNotFound => (RecoveryStrategy::FailFast, false),
            ErrorCategory::PermissionDenied => (RecoveryStrategy::FailFast, false),
            ErrorCategory::EncryptionFailed => (RecoveryStrategy::FailFast, false),
            ErrorCategory::DecryptionFailed => (RecoveryStrategy::FailFast, false),
            ErrorCategory::CacheMiss => (RecoveryStrategy::Retry, true),
            ErrorCategory::Unknown => (RecoveryStrategy::FailFast, false),
        };

        Self {
            category,
            message: message.into(),
            strategy,
            suggestion,
            timestamp: Utc::now(),
            retry_count: 0,
            retryable,
        }
    }

    /// Increment retry count
    pub fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    /// Get time since error
    pub fn age(&self) -> Duration {
        Utc::now() - self.timestamp
    }

    /// Check if error is fresh (less than 1 minute old)
    pub fn is_fresh(&self) -> bool {
        self.age() < Duration::minutes(1)
    }

    /// Check if this error suggests a transient issue (can retry)
    pub fn is_transient(&self) -> bool {
        self.retryable
    }

    /// Check if cache fallback is appropriate for this error
    pub fn should_use_cache(&self) -> bool {
        matches!(self.strategy, RecoveryStrategy::UseCache | RecoveryStrategy::ReadOnly)
    }

    /// Estimate milliseconds to wait before retry based on retry count
    pub fn retry_delay_ms(&self, config: &RetryConfig) -> u64 {
        config.backoff_delay_ms(self.retry_count)
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries:        u32,
    /// Initial backoff delay in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds
    pub max_backoff_ms:     u64,
    /// Backoff multiplier for exponential growth
    pub backoff_multiplier: f64,
}

impl RetryConfig {
    /// Create new retry config
    pub fn new() -> Self {
        Self {
            max_retries:        3,
            initial_backoff_ms: 100,
            max_backoff_ms:     5000,
            backoff_multiplier: 2.0,
        }
    }

    /// Set maximum retries
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Calculate backoff delay for retry attempt
    pub fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        let delay = self.initial_backoff_ms as f64 * self.backoff_multiplier.powi(attempt as i32);
        (delay as u64).min(self.max_backoff_ms)
    }

    /// Check if we should attempt retry based on attempt count
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }

    /// Add jitter to delay to prevent thundering herd
    pub fn backoff_delay_with_jitter_ms(&self, attempt: u32) -> u64 {
        use rand::Rng;
        let base_delay = self.backoff_delay_ms(attempt);
        // Add ±10% jitter
        let jitter_percent = base_delay / 10;
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0..=jitter_percent);
        let use_add = rng.gen_bool(0.5);
        if use_add {
            base_delay + jitter
        } else {
            base_delay.saturating_sub(jitter)
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit breaker for fault tolerance
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Failure threshold before opening
    failure_threshold: u32,
    /// Success threshold to close
    success_threshold: u32,
    /// Current failure count
    failure_count:     Arc<AtomicU64>,
    /// Current success count
    success_count:     Arc<AtomicU64>,
    /// Circuit state
    state:             Arc<atomic::AtomicUsize>,
    /// Last state change time
    last_change:       Arc<std::sync::Mutex<DateTime<Utc>>>,
}

mod atomic {
    use std::sync::atomic::AtomicUsize as StdAtomicUsize;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(usize)]
    pub enum CircuitState {
        Closed   = 0,
        Open     = 1,
        HalfOpen = 2,
    }

    impl CircuitState {
        pub fn from_usize(val: usize) -> Self {
            match val {
                0 => CircuitState::Closed,
                1 => CircuitState::Open,
                2 => CircuitState::HalfOpen,
                _ => CircuitState::Closed,
            }
        }

        pub fn to_usize(self) -> usize {
            self as usize
        }
    }

    pub struct AtomicUsize(StdAtomicUsize);

    impl AtomicUsize {
        pub fn new(val: CircuitState) -> Self {
            AtomicUsize(StdAtomicUsize::new(val.to_usize()))
        }

        pub fn load(&self) -> CircuitState {
            CircuitState::from_usize(self.0.load(std::sync::atomic::Ordering::Relaxed))
        }

        pub fn store(&self, val: CircuitState) {
            self.0.store(val.to_usize(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    impl std::fmt::Debug for AtomicUsize {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.load())
        }
    }
}

use atomic::{AtomicUsize, CircuitState};

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(failure_threshold: u32, success_threshold: u32) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            state: Arc::new(AtomicUsize::new(CircuitState::Closed)),
            last_change: Arc::new(std::sync::Mutex::new(Utc::now())),
        }
    }

    /// Record successful operation
    pub fn record_success(&self) {
        let state = self.state.load();
        match state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            },
            CircuitState::HalfOpen => {
                let success = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if success >= self.success_threshold as u64 {
                    self.state.store(CircuitState::Closed);
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    if let Ok(mut last) = self.last_change.lock() {
                        *last = Utc::now();
                    }
                }
            },
            CircuitState::Open => {},
        }
    }

    /// Record failed operation
    pub fn record_failure(&self) {
        let state = self.state.load();
        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= self.failure_threshold as u64 {
                    self.state.store(CircuitState::Open);
                    if let Ok(mut last) = self.last_change.lock() {
                        *last = Utc::now();
                    }
                }
            },
            CircuitState::Open => {
                // Could transition to HalfOpen after timeout
            },
            CircuitState::HalfOpen => {
                self.state.store(CircuitState::Open);
                self.success_count.store(0, Ordering::Relaxed);
                if let Ok(mut last) = self.last_change.lock() {
                    *last = Utc::now();
                }
            },
        }
    }

    /// Check if operation allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self.state.load(), CircuitState::Closed | CircuitState::HalfOpen)
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.state.load()
    }

    /// Reset circuit breaker
    pub fn reset(&self) {
        self.state.store(CircuitState::Closed);
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
    }

    /// Get time since last state change
    pub fn time_since_last_change(&self) -> Duration {
        if let Ok(last) = self.last_change.lock() {
            Utc::now() - *last
        } else {
            Duration::zero()
        }
    }

    /// Check if circuit should attempt recovery from Open state
    pub fn should_attempt_recovery(&self, recovery_timeout_ms: u64) -> bool {
        matches!(self.state.load(), CircuitState::Open)
            && self.time_since_last_change().num_milliseconds() as u64 >= recovery_timeout_ms
    }

    /// Attempt to transition from Open to HalfOpen after timeout
    pub fn attempt_recovery(&self, recovery_timeout_ms: u64) {
        if self.should_attempt_recovery(recovery_timeout_ms) {
            self.state.store(CircuitState::HalfOpen);
            self.success_count.store(0, Ordering::Relaxed);
            if let Ok(mut last) = self.last_change.lock() {
                *last = Utc::now();
            }
        }
    }

    /// Get current failure and success counts
    pub fn get_counts(&self) -> (u64, u64) {
        let failures = self.failure_count.load(Ordering::Relaxed);
        let successes = self.success_count.load(Ordering::Relaxed);
        (failures, successes)
    }

    /// Check if circuit is fully open (no operations allowed)
    pub fn is_open(&self) -> bool {
        matches!(self.state.load(), CircuitState::Open)
    }

    /// Check if circuit is half-open (limited operations for recovery)
    pub fn is_half_open(&self) -> bool {
        matches!(self.state.load(), CircuitState::HalfOpen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_display() {
        assert_eq!(RecoveryStrategy::UseCache.to_string(), "use_cache");
        assert_eq!(RecoveryStrategy::Retry.to_string(), "retry");
        assert_eq!(RecoveryStrategy::FailFast.to_string(), "fail_fast");
        assert_eq!(RecoveryStrategy::ReadOnly.to_string(), "read_only");
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::NetworkError.to_string(), "network_error");
        assert_eq!(ErrorCategory::VaultUnavailable.to_string(), "vault_unavailable");
        assert_eq!(ErrorCategory::KeyNotFound.to_string(), "key_not_found");
    }

    #[test]
    fn test_recovery_error_network() {
        let error = RecoveryError::new(ErrorCategory::NetworkError, "Connection timeout");
        assert_eq!(error.category, ErrorCategory::NetworkError);
        assert_eq!(error.strategy, RecoveryStrategy::Retry);
        assert!(error.retryable);
    }

    #[test]
    fn test_recovery_error_vault_unavailable() {
        let error = RecoveryError::new(ErrorCategory::VaultUnavailable, "Vault unreachable");
        assert_eq!(error.category, ErrorCategory::VaultUnavailable);
        assert_eq!(error.strategy, RecoveryStrategy::UseCache);
        assert!(error.retryable);
    }

    #[test]
    fn test_recovery_error_key_not_found() {
        let error = RecoveryError::new(ErrorCategory::KeyNotFound, "Key missing");
        assert!(!error.retryable);
        assert_eq!(error.strategy, RecoveryStrategy::FailFast);
    }

    #[test]
    fn test_recovery_error_with_retry_count() {
        let error = RecoveryError::new(ErrorCategory::NetworkError, "Timeout").with_retry_count(2);
        assert_eq!(error.retry_count, 2);
    }

    #[test]
    fn test_recovery_error_is_fresh() {
        let error = RecoveryError::new(ErrorCategory::NetworkError, "Timeout");
        assert!(error.is_fresh());
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff_ms, 100);
        assert_eq!(config.max_backoff_ms, 5000);
    }

    #[test]
    fn test_retry_config_backoff_calculation() {
        let config = RetryConfig::new();
        assert_eq!(config.backoff_delay_ms(0), 100);
        assert_eq!(config.backoff_delay_ms(1), 200);
        assert_eq!(config.backoff_delay_ms(2), 400);
        assert_eq!(config.backoff_delay_ms(3), 800);
    }

    #[test]
    fn test_retry_config_max_backoff() {
        let config = RetryConfig::new().with_max_retries(10);
        let delay = config.backoff_delay_ms(10);
        assert_eq!(delay, config.max_backoff_ms);
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let breaker = CircuitBreaker::new(3, 2);
        assert!(breaker.is_allowed());
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_on_failure() {
        let breaker = CircuitBreaker::new(3, 2);
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert!(!breaker.is_allowed());
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_half_open_on_success() {
        let breaker = CircuitBreaker::new(3, 2);
        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();

        // Manually transition to half-open for testing
        breaker.state.store(CircuitState::HalfOpen);
        breaker.record_success();
        breaker.record_success();

        assert!(breaker.is_allowed());
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let breaker = CircuitBreaker::new(3, 2);
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert!(!breaker.is_allowed());

        breaker.reset();
        assert!(breaker.is_allowed());
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_recovery_error_is_transient() {
        let transient = RecoveryError::new(ErrorCategory::NetworkError, "Network issue");
        assert!(transient.is_transient());

        let not_transient = RecoveryError::new(ErrorCategory::KeyNotFound, "Key missing");
        assert!(!not_transient.is_transient());
    }

    #[test]
    fn test_recovery_error_should_use_cache() {
        let vault_error = RecoveryError::new(ErrorCategory::VaultUnavailable, "Vault down");
        assert!(vault_error.should_use_cache());

        let key_error = RecoveryError::new(ErrorCategory::KeyExpired, "Key expired");
        assert!(!key_error.should_use_cache());
    }

    #[test]
    fn test_retry_config_should_retry() {
        let config = RetryConfig::new().with_max_retries(3);
        assert!(config.should_retry(0));
        assert!(config.should_retry(2));
        assert!(!config.should_retry(3));
        assert!(!config.should_retry(5));
    }

    #[test]
    fn test_circuit_breaker_get_counts() {
        let breaker = CircuitBreaker::new(5, 3);
        let (failures, successes) = breaker.get_counts();
        assert_eq!(failures, 0);
        assert_eq!(successes, 0);

        breaker.record_failure();
        breaker.record_failure();
        let (failures, _) = breaker.get_counts();
        assert_eq!(failures, 2);
    }

    #[test]
    fn test_circuit_breaker_is_open() {
        let breaker = CircuitBreaker::new(2, 2);
        assert!(!breaker.is_open());

        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_open());
    }

    #[test]
    fn test_circuit_breaker_is_half_open() {
        let breaker = CircuitBreaker::new(2, 2);
        assert!(!breaker.is_half_open());

        breaker.record_failure();
        breaker.record_failure();
        breaker.state.store(CircuitState::HalfOpen);
        assert!(breaker.is_half_open());
    }

    #[test]
    fn test_circuit_breaker_attempt_recovery() {
        let breaker = CircuitBreaker::new(1, 2);
        breaker.record_failure();
        assert!(breaker.is_open());

        // Attempt recovery immediately (timeout not elapsed)
        breaker.attempt_recovery(1000);
        assert!(breaker.is_open());

        // Should transition to HalfOpen after timeout
        breaker.attempt_recovery(0);
        assert!(breaker.is_half_open());
    }
}
