#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_remote_database_config_defaults() {
    let config = RemoteDatabaseConfig::new("postgresql://localhost/db");
    assert_eq!(config.get_pool_size(), 5);
    assert_eq!(config.get_timeout_seconds(), 5);
}

#[test]
fn test_remote_database_config_custom() {
    let config = RemoteDatabaseConfig::new("postgresql://localhost/db")
        .with_pool_size(10)
        .with_timeout(30);

    assert_eq!(config.get_pool_size(), 10);
    assert_eq!(config.get_timeout_seconds(), 30);
}

#[test]
fn test_connection_manager_creation() {
    let _manager = ConnectionManager::new();
    // Should not panic
}

#[test]
fn test_connection_manager_default() {
    let _manager = ConnectionManager::default();
    // Should not panic
}

#[test]
fn test_connection_count_empty() {
    let manager = ConnectionManager::new();
    assert_eq!(manager.connection_count(), 0);
}

#[test]
fn test_close_all() {
    let manager = ConnectionManager::new();
    // Should not panic even with no connections
    manager.close_all();
}

#[test]
fn test_config_connection_string_not_in_debug() {
    let config = RemoteDatabaseConfig::new("postgresql://user:secret@host/db");
    let debug_output = format!("{config:?}");
    assert!(!debug_output.contains("secret"), "connection string must not appear in Debug");
    assert!(debug_output.contains("<redacted>"));
}

#[test]
fn test_config_connection_string_accessor() {
    let config = RemoteDatabaseConfig::new("postgresql://host/db");
    assert_eq!(config.connection_string(), "postgresql://host/db");
}

// ── Bounds validation tests ────────────────────────────────────────────────

#[cfg(feature = "unstable")]
#[test]
fn test_validate_accepts_valid_defaults() {
    let config = RemoteDatabaseConfig::new("postgresql://host/db");
    config
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for default config (no explicit values): {e}"));
}

#[cfg(feature = "unstable")]
#[test]
fn test_validate_accepts_pool_size_at_limits() {
    let lo = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MIN_POOL_SIZE);
    lo.validate()
        .unwrap_or_else(|e| panic!("expected Ok for pool_size=MIN_POOL_SIZE: {e}"));

    let hi = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MAX_POOL_SIZE);
    hi.validate()
        .unwrap_or_else(|e| panic!("expected Ok for pool_size=MAX_POOL_SIZE: {e}"));
}

#[cfg(feature = "unstable")]
#[test]
fn test_validate_rejects_pool_size_zero() {
    let config = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(0);
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("pool_size"), "error must mention pool_size: {err}");
}

#[cfg(feature = "unstable")]
#[test]
fn test_validate_rejects_pool_size_too_large() {
    let config =
        RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MAX_POOL_SIZE + 1);
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("pool_size"), "error must mention pool_size: {err}");
}

#[cfg(feature = "unstable")]
#[test]
fn test_validate_rejects_timeout_zero() {
    let config = RemoteDatabaseConfig::new("postgresql://host/db").with_timeout(0);
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("timeout_seconds"), "error must mention timeout: {err}");
}

#[cfg(feature = "unstable")]
#[test]
fn test_validate_rejects_timeout_too_large() {
    let config =
        RemoteDatabaseConfig::new("postgresql://host/db").with_timeout(MAX_TIMEOUT_SECS + 1);
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("timeout_seconds"), "error must mention timeout: {err}");
}
