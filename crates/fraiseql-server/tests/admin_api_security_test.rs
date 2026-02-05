//! Admin API Security Tests
//!
//! Tests for admin API endpoints protection:
//! - Admin endpoints disabled by default
//! - Admin endpoints require valid bearer token
//! - Configuration validation for admin settings

use fraiseql_server::ServerConfig;

#[test]
fn test_admin_api_disabled_by_default() {
    let config = ServerConfig::default();
    assert!(
        !config.admin_api_enabled,
        "Admin API should be disabled by default for security"
    );
    assert!(
        config.admin_token.is_none(),
        "Admin token should be None when disabled"
    );
}

#[test]
fn test_admin_api_enabled_without_token_fails_validation() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: None,
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "Validation should fail when admin API enabled without token");
    let error = result.unwrap_err();
    assert!(
        error.contains("admin_token is not set"),
        "Error message should mention missing token"
    );
}

#[test]
fn test_admin_api_enabled_with_short_token_fails_validation() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("short".to_string()), // Less than 32 characters
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "Validation should fail with short token");
    let error = result.unwrap_err();
    assert!(
        error.contains("at least 32 characters"),
        "Error message should mention minimum token length"
    );
}

#[test]
fn test_admin_api_enabled_with_valid_token_passes_validation() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some(
            "a-very-secure-admin-token-that-is-long-enough-for-security".to_string(),
        ),
        cors_enabled: false, // Disable CORS to avoid production mode validation
        ..ServerConfig::default()
    };

    assert!(
        config.validate().is_ok(),
        "Validation should pass with valid token (32+ chars)"
    );
}

#[test]
fn test_admin_api_disabled_with_valid_token_passes_validation() {
    // Token can be set even if admin_api_enabled is false (no-op)
    let config = ServerConfig {
        admin_api_enabled: false,
        admin_token: Some("a-very-secure-admin-token-that-is-long-enough".to_string()),
        cors_enabled: false, // Disable CORS to avoid production mode validation
        ..ServerConfig::default()
    };

    assert!(
        config.validate().is_ok(),
        "Validation should pass when admin API disabled (even with token)"
    );
}

#[test]
fn test_admin_token_minimum_length_is_32_characters() {
    // Test with exactly 31 characters (one below minimum)
    let config_31_chars = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("a".repeat(31)),
        cors_enabled: false, // Disable CORS to avoid production mode validation
        ..ServerConfig::default()
    };
    assert!(
        config_31_chars.validate().is_err(),
        "31-character token should fail"
    );

    // Test with exactly 32 characters (minimum)
    let config_32_chars = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("a".repeat(32)),
        cors_enabled: false, // Disable CORS to avoid production mode validation
        ..ServerConfig::default()
    };
    assert!(
        config_32_chars.validate().is_ok(),
        "32-character token should pass"
    );
}

#[test]
fn test_admin_config_serialization() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("test-admin-token-32-characters-long!".to_string()),
        ..ServerConfig::default()
    };

    // Serialize to TOML and back
    let toml_str = toml::to_string(&config).expect("Serialization should work");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Deserialization should work");

    assert_eq!(restored.admin_api_enabled, config.admin_api_enabled);
    assert_eq!(restored.admin_token, config.admin_token);
}

#[test]
fn test_admin_config_independence_from_metrics() {
    // Admin and metrics configurations should be independent
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("admin-token-32-characters-long-x!".to_string()),
        metrics_enabled: true,
        metrics_token: Some("metrics-token-16-chars-min!!".to_string()),
        cors_enabled: false, // Disable CORS to avoid production mode validation
        ..ServerConfig::default()
    };

    assert!(config.validate().is_ok(), "Both configurations can coexist");
    assert!(config.admin_api_enabled);
    assert!(config.metrics_enabled);
}

#[test]
fn test_admin_config_with_empty_token_string_fails() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("".to_string()),
        ..ServerConfig::default()
    };

    let result = config.validate();
    assert!(
        result.is_err(),
        "Empty token string should fail validation"
    );
}
