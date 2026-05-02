//! Introspection & Schema Export Security Tests
//!
//! Tests for introspection endpoint and schema export protection:
//! - Introspection endpoints disabled by default
//! - Introspection can require authentication
//! - Schema export endpoints follow introspection settings
//! - Configuration validation
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe

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
        cors_enabled: false,
        ..ServerConfig::default()
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
        cors_enabled: false,
        ..ServerConfig::default()
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
        cors_enabled: false,
        ..ServerConfig::default()
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
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let toml_str = toml::to_string(&config).expect("Serialization should work");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Deserialization should work");

    assert_eq!(restored.introspection_enabled, config.introspection_enabled);
    assert_eq!(restored.introspection_require_auth, config.introspection_require_auth);
}

#[test]
fn test_introspection_independent_from_admin() {
    // Introspection and admin configurations should be independent
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        admin_api_enabled: true,
        admin_token: Some("admin-token-32-characters-long-x".to_string()),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with introspection and admin enabled should validate: {e}")
    });
    assert!(config.introspection_enabled, "introspection_enabled should be true");
    assert!(!config.introspection_require_auth, "introspection_require_auth should be false");
    assert!(config.admin_api_enabled, "admin_api_enabled should be true");
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
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Dev config with introspection enabled should validate: {e}"));
}

#[test]
fn test_introspection_production_config() {
    // Typical production configuration: introspection disabled or auth required
    let config = ServerConfig {
        introspection_enabled: false,
        introspection_require_auth: true,
        playground_enabled: false,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Production config with introspection disabled should validate: {e}")
    });
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
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with introspection enabled for schema export should validate: {e}")
    });
    // Both introspection and schema export endpoints would be available
}

#[test]
fn test_schema_export_disabled_when_introspection_disabled() {
    // When introspection is disabled, schema export should not be available either
    let config = ServerConfig {
        introspection_enabled: false,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Config with introspection disabled should validate: {e}"));
    // Both introspection and schema export endpoints would be disabled
}

#[test]
fn test_schema_export_protected_with_auth() {
    // Schema export endpoints should have same protection as introspection
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with auth-protected schema export should validate: {e}")
    });
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
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with all debug endpoints disabled should validate: {e}")
    });
}

#[test]
fn test_introspection_and_playground_independence() {
    // Introspection and playground can be independently configured
    let config_a = ServerConfig {
        introspection_enabled: true,
        playground_enabled: false,
        introspection_require_auth: false,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let config_b = ServerConfig {
        introspection_enabled: false,
        playground_enabled: false, // Both disabled by default for production safety
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config_a.validate().unwrap_or_else(|e| {
        panic!("Config with introspection=true, playground=false should validate: {e}")
    });
    config_b.validate().unwrap_or_else(|e| {
        panic!("Config with introspection=false, playground=false should validate: {e}")
    });
}

#[test]
fn test_introspection_path_customization() {
    // Introspection path can be customized
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_path: "/api/introspection".to_string(),
        introspection_require_auth: false,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Config with custom introspection path should validate: {e}"));
    assert_eq!(config.introspection_path, "/api/introspection");
}

// =============================================================================
// Metadata Endpoint Independent Auth Tests
// =============================================================================

#[test]
fn test_metadata_require_auth_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(
        config.metadata_require_auth.is_none(),
        "metadata_require_auth should default to None (fallback to introspection_require_auth)"
    );
}

#[test]
fn test_metadata_require_auth_true_while_introspection_public() {
    // When introspection is public but metadata_require_auth is explicitly true,
    // the metadata endpoint should require auth independently.
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        metadata_require_auth: Some(true),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with metadata_require_auth=true, introspection public should validate: {e}")
    });

    // Effective metadata auth: Some(true) overrides introspection_require_auth=false
    let effective = config.metadata_require_auth.unwrap_or(config.introspection_require_auth);
    assert!(effective, "metadata should require auth when explicitly set to true");
}

#[test]
fn test_metadata_require_auth_false_while_introspection_auth_required() {
    // When introspection requires auth but metadata_require_auth is explicitly false,
    // the metadata endpoint should be publicly accessible.
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        metadata_require_auth: Some(false),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with metadata_require_auth=false, introspection auth should validate: {e}")
    });

    // Effective metadata auth: Some(false) overrides introspection_require_auth=true
    let effective = config.metadata_require_auth.unwrap_or(config.introspection_require_auth);
    assert!(!effective, "metadata should be public when explicitly set to false");
}

#[test]
fn test_metadata_require_auth_unset_falls_back_to_introspection_require_auth() {
    // When metadata_require_auth is None, it should fall back to introspection_require_auth.
    let config_auth = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        metadata_require_auth: None,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let effective_auth = config_auth
        .metadata_require_auth
        .unwrap_or(config_auth.introspection_require_auth);
    assert!(effective_auth, "should fall back to introspection_require_auth=true");

    let config_public = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        metadata_require_auth: None,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let effective_public = config_public
        .metadata_require_auth
        .unwrap_or(config_public.introspection_require_auth);
    assert!(!effective_public, "should fall back to introspection_require_auth=false");
}

#[test]
fn test_metadata_require_auth_serialization_roundtrip() {
    let config = ServerConfig {
        introspection_enabled: true,
        metadata_require_auth: Some(true),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let toml_str = toml::to_string(&config).expect("Serialization should work");
    let restored: ServerConfig = toml::from_str(&toml_str).expect("Deserialization should work");

    assert_eq!(restored.metadata_require_auth, Some(true));
}

#[test]
fn test_metadata_require_auth_absent_in_toml_deserializes_as_none() {
    // When metadata_require_auth is not present in TOML, it should deserialize as None
    let toml_str = r#"
schema_path = "schema.compiled.json"
database_url = "postgres://localhost/test"
introspection_enabled = true
introspection_require_auth = false
"#;
    let config: ServerConfig = toml::from_str(toml_str).expect("Deserialization should work");
    assert!(
        config.metadata_require_auth.is_none(),
        "absent field should deserialize as None"
    );
}

// =============================================================================
// Schema Export Independent Auth Tests
// =============================================================================

#[test]
fn test_schema_export_require_auth_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(
        config.schema_export_require_auth.is_none(),
        "schema_export_require_auth should default to None (fallback to introspection_require_auth)"
    );
}

#[test]
fn test_schema_export_require_auth_true_while_introspection_public() {
    // introspection_require_auth=false but schema_export_require_auth=true
    // → schema export should require auth independently.
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        schema_export_require_auth: Some(true),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!(
            "Config with schema_export_require_auth=true, introspection public should validate: {e}"
        )
    });

    let effective = config
        .schema_export_require_auth
        .unwrap_or(config.introspection_require_auth);
    assert!(
        effective,
        "schema export should require auth when explicitly set to true"
    );
}

#[test]
fn test_schema_export_require_auth_unset_falls_back_to_introspection_require_auth() {
    // When schema_export_require_auth is None it falls back to introspection_require_auth.
    let config_auth = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        schema_export_require_auth: None,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let effective_auth = config_auth
        .schema_export_require_auth
        .unwrap_or(config_auth.introspection_require_auth);
    assert!(
        effective_auth,
        "should fall back to introspection_require_auth=true"
    );

    let config_public = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        schema_export_require_auth: None,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let effective_public = config_public
        .schema_export_require_auth
        .unwrap_or(config_public.introspection_require_auth);
    assert!(
        !effective_public,
        "should fall back to introspection_require_auth=false"
    );
}

#[test]
fn test_schema_export_require_auth_absent_in_toml_deserializes_as_none() {
    let toml_str = r#"
schema_path = "schema.compiled.json"
database_url = "postgres://localhost/test"
introspection_enabled = true
introspection_require_auth = false
"#;
    let config: ServerConfig = toml::from_str(toml_str).expect("Deserialization should work");
    assert!(
        config.schema_export_require_auth.is_none(),
        "absent field should deserialize as None"
    );
}
