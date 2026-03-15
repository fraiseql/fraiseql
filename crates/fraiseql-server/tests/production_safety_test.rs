//! Production Safety Tests
//!
//! Tests for production mode safety validation:
//! - Playground disabled by default
//! - CORS must be explicitly configured in production
//! - Production mode detection via `FRAISEQL_ENV`
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns
#![allow(clippy::match_same_arms)] // Reason: test data clarity
#![allow(clippy::branches_sharing_code)] // Reason: test assertion clarity
#![allow(clippy::undocumented_unsafe_blocks)] // Reason: test exercises unsafe paths

use fraiseql_server::ServerConfig;

// =============================================================================
// Playground Default Tests
// =============================================================================

#[test]
fn test_playground_disabled_by_default() {
    let config = ServerConfig::default();
    assert!(
        !config.playground_enabled,
        "Playground should be disabled by default for security"
    );
}

#[test]
fn test_playground_can_be_enabled() {
    let config = ServerConfig {
        playground_enabled: true,
        ..ServerConfig::default()
    };

    assert!(config.playground_enabled);
}

// =============================================================================
// Production Mode Detection Tests
// =============================================================================

#[test]
fn test_production_mode_default() {
    // When FRAISEQL_ENV is not set, defaults to production mode.
    // When set to "development"/"dev", returns false.
    let original = std::env::var("FRAISEQL_ENV").ok();

    // Simulate unset: production
    std::env::remove_var("FRAISEQL_ENV");
    assert!(
        ServerConfig::is_production_mode(),
        "without FRAISEQL_ENV, must default to production mode"
    );

    // Simulate development mode
    std::env::set_var("FRAISEQL_ENV", "development");
    assert!(
        !ServerConfig::is_production_mode(),
        "FRAISEQL_ENV=development must not be production mode"
    );

    // Restore original env state
    match original {
        Some(v) => std::env::set_var("FRAISEQL_ENV", v),
        None => std::env::remove_var("FRAISEQL_ENV"),
    }
}

// =============================================================================
// CORS Configuration Tests
// =============================================================================

#[test]
fn test_cors_with_origins_passes_validation() {
    let config = ServerConfig {
        cors_enabled: true,
        cors_origins: vec!["http://localhost:3000".to_string()],
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for CORS config with origins: {e}"));
}

#[test]
fn test_cors_disabled_passes_validation() {
    let config = ServerConfig {
        cors_enabled: false,
        cors_origins: vec![], // Empty is OK when disabled
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for CORS disabled config: {e}"));
}

#[test]
fn test_cors_multiple_origins_passes_validation() {
    let config = ServerConfig {
        cors_enabled: true,
        cors_origins: vec![
            "http://localhost:3000".to_string(),
            "http://localhost:5173".to_string(),
            "https://example.com".to_string(),
        ],
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for CORS multiple origins: {e}"));
}

// =============================================================================
// Development Configuration Tests
// =============================================================================

#[test]
fn test_development_allows_playground_and_empty_cors() {
    // In development mode, playground=true with empty cors_origins must pass validation.
    let original = std::env::var("FRAISEQL_ENV").ok();
    std::env::set_var("FRAISEQL_ENV", "development");

    let config = ServerConfig {
        playground_enabled: true,
        cors_enabled: true,
        cors_origins: vec![], // Empty origins are acceptable in development
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!(
        "in development mode, playground + empty cors_origins must pass validation: {e}"
    ));

    match original {
        Some(v) => std::env::set_var("FRAISEQL_ENV", v),
        None => std::env::remove_var("FRAISEQL_ENV"),
    }
}

// =============================================================================
// Configuration Combinations Tests
// =============================================================================

#[test]
fn test_typical_development_config() {
    // Development-like config (but safe for production since playground is disabled)
    // To actually run this in dev mode, set FRAISEQL_ENV=development
    let config = ServerConfig {
        playground_enabled: false, // Would be true in actual dev
        cors_enabled: true,
        cors_origins: vec!["http://localhost:3000".to_string()],
        introspection_enabled: true,
        introspection_require_auth: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for typical development config: {e}"));
}

#[test]
fn test_minimal_production_config() {
    let config = ServerConfig {
        playground_enabled: false,
        cors_enabled: false,
        cors_origins: vec![],
        introspection_enabled: false,
        ..ServerConfig::default()
    };

    // Should pass even without FRAISEQL_ENV because playground is disabled
    // and CORS is disabled
    config.validate().unwrap_or_else(|e| panic!("expected Ok for minimal production config: {e}"));
}

#[test]
fn test_production_config_with_configured_cors() {
    let config = ServerConfig {
        playground_enabled: false,
        cors_enabled: true,
        cors_origins: vec!["https://api.example.com".to_string()],
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for production config with CORS: {e}"));
}

// =============================================================================
// Backward Compatibility Tests
// =============================================================================

#[test]
fn test_cors_default_settings() {
    let config = ServerConfig::default();
    assert!(config.cors_enabled);
    assert!(config.cors_origins.is_empty());
}

#[test]
fn test_subscriptions_defaults() {
    let config = ServerConfig::default();
    assert!(config.subscriptions_enabled);
}

#[test]
fn test_graphql_path_default() {
    let config = ServerConfig::default();
    assert_eq!(config.graphql_path, "/graphql");
}

#[test]
fn test_all_security_settings_can_be_configured() {
    let config = ServerConfig {
        playground_enabled: false,
        introspection_enabled: false,
        admin_api_enabled: false,
        cors_enabled: true,
        cors_origins: vec!["https://example.com".to_string()],
        metrics_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| panic!("expected Ok for all security settings configured: {e}"));
}

// =============================================================================
// Configuration Serialization Tests
// =============================================================================

#[test]
fn test_playground_disabled_serializes() {
    let config = ServerConfig {
        playground_enabled: false,
        ..ServerConfig::default()
    };

    let toml_str = toml::to_string(&config).expect("Should serialize");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Should deserialize");

    assert!(!restored.playground_enabled);
}

#[test]
fn test_cors_origins_serializes() {
    let config = ServerConfig {
        cors_enabled: true,
        cors_origins: vec![
            "https://example.com".to_string(),
            "https://api.example.com".to_string(),
        ],
        ..ServerConfig::default()
    };

    let toml_str = toml::to_string(&config).expect("Should serialize");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Should deserialize");

    assert_eq!(restored.cors_origins, config.cors_origins);
}
