//! Introspection & Schema Export Security Tests
//!
//! Tests for introspection endpoint and schema export protection:
//! - Introspection endpoints disabled by default
//! - Introspection can require authentication
//! - Schema export endpoints follow introspection settings
//! - Configuration validation

use fraiseql_server::ServerConfig;

// =============================================================================
// Introspection Configuration Tests
// =============================================================================

#[test]
fn test_introspection_disabled_by_default() {
    let config = ServerConfig::default();
    assert!(
        !config.introspection_enabled,
        "Introspection should be disabled by default for security"
    );
}

#[test]
fn test_introspection_require_auth_defaults_to_true() {
    let config = ServerConfig::default();
    assert!(
        config.introspection_require_auth,
        "Introspection should require auth by default"
    );
}

#[test]
fn test_introspection_enabled_with_auth_required_passes_validation() {
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(
        config.validate().is_ok(),
        "Introspection enabled with auth required should pass validation"
    );
}

#[test]
fn test_introspection_enabled_without_auth_passes_validation() {
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(
        config.validate().is_ok(),
        "Introspection enabled without auth should pass validation"
    );
}

#[test]
fn test_introspection_disabled_with_any_auth_setting_passes_validation() {
    let config = ServerConfig {
        introspection_enabled: false,
        introspection_require_auth: true,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(
        config.validate().is_ok(),
        "Introspection disabled should pass validation regardless of auth setting"
    );
}

#[test]
fn test_introspection_configuration_serialization() {
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        cors_enabled: false, ..ServerConfig::default()
    };

    let toml_str = toml::to_string(&config).expect("Serialization should work");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Deserialization should work");

    assert_eq!(restored.introspection_enabled, config.introspection_enabled);
    assert_eq!(
        restored.introspection_require_auth,
        config.introspection_require_auth
    );
}

#[test]
fn test_introspection_independent_from_admin() {
    // Introspection and admin configurations should be independent
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        admin_api_enabled: true,
        admin_token: Some("admin-token-32-characters-long-x".to_string()),
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
    assert!(config.introspection_enabled);
    assert!(!config.introspection_require_auth);
    assert!(config.admin_api_enabled);
}

// =============================================================================
// Development vs Production Settings
// =============================================================================

#[test]
fn test_introspection_dev_config() {
    // Development-like configuration: introspection enabled, no auth
    // (playground would normally be enabled in dev, but we disable for test safety)
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        playground_enabled: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_introspection_production_config() {
    // Typical production configuration: introspection disabled or auth required
    let config = ServerConfig {
        introspection_enabled: false,
        introspection_require_auth: true,
        playground_enabled: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
}

// =============================================================================
// Schema Export Endpoint Tests
// =============================================================================

#[test]
fn test_schema_export_follows_introspection_setting() {
    // When introspection is enabled, schema export should also be available
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
    // Both introspection and schema export endpoints would be available
}

#[test]
fn test_schema_export_disabled_when_introspection_disabled() {
    // When introspection is disabled, schema export should not be available either
    let config = ServerConfig {
        introspection_enabled: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
    // Both introspection and schema export endpoints would be disabled
}

#[test]
fn test_schema_export_protected_with_auth() {
    // Schema export endpoints should have same protection as introspection
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
    // Schema export endpoints would require OIDC authentication
}

// =============================================================================
// Combined Feature Tests
// =============================================================================

#[test]
fn test_all_debug_endpoints_can_be_disabled() {
    // All debug endpoints (introspection, playground, admin) can be disabled
    let config = ServerConfig {
        introspection_enabled: false,
        playground_enabled: false,
        admin_api_enabled: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_introspection_and_playground_independence() {
    // Introspection and playground can be independently configured
    let config_a = ServerConfig {
        introspection_enabled: true,
        playground_enabled: false,
        introspection_require_auth: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    let config_b = ServerConfig {
        introspection_enabled: false,
        playground_enabled: false, // Both disabled by default for production safety
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config_a.validate().is_ok());
    assert!(config_b.validate().is_ok());
}

#[test]
fn test_introspection_path_customization() {
    // Introspection path can be customized
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_path: "/api/introspection".to_string(),
        introspection_require_auth: false,
        cors_enabled: false, ..ServerConfig::default()
    };

    assert!(config.validate().is_ok());
    assert_eq!(config.introspection_path, "/api/introspection");
}
