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
    // This is a basic test - full integration tests use the background_loop test file
    let config = RecoveryConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(5));
}
