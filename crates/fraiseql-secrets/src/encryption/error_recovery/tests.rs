#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
