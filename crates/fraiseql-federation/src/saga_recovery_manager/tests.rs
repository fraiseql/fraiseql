#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_recovery_config_default() {
    let config = RecoveryConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(5));
    assert_eq!(config.max_sagas_per_iteration, 50);
    assert_eq!(config.stale_age_hours, 24);
}

#[test]
fn test_recovery_manager_creation() {
    // Basic config check; the store-backed recovery loop is exercised end-to-end
    // against real PostgreSQL in `tests/saga_integration.rs` (mod `recovery_pg`).
    let config = RecoveryConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(5));
}
