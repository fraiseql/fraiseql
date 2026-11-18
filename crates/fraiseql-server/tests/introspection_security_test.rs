//! Introspection & Schema Export Security Tests
//!
//! Tests for introspection endpoint and schema export protection:
//! - Introspection endpoints disabled by default
//! - Introspection can require authentication
//! - Schema export, metadata and the service SDL require auth on their own switches
//! - Configuration validation
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::panic)] // Reason: test code, panics acceptable
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
    // When introspection is enabled, schema export is mounted too, behind its own switch
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with introspection enabled for schema export should validate: {e}")
    });
    // Introspection is public; schema export still requires auth (its own switch)
    assert!(config.schema_export_requires_auth());
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
    // Schema export endpoints require auth unless their own switch says otherwise
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: true,
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!("Config with auth-protected schema export should validate: {e}")
    });
    assert!(config.schema_export_requires_auth());
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
        "metadata_require_auth should default to None (required unless set)"
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

    assert!(
        config.metadata_requires_auth(),
        "metadata should require auth when explicitly set to true"
    );
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

    assert!(
        !config.metadata_requires_auth(),
        "metadata should be public when explicitly set to false"
    );
}

#[test]
fn test_metadata_require_auth_unset_is_required_whatever_introspection_says() {
    // Unset, it requires auth: it serves the schema, and opening introspection does
    // not open it.
    for introspection_require_auth in [true, false] {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth,
            metadata_require_auth: None,
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(
            config.metadata_requires_auth(),
            "introspection_require_auth = {introspection_require_auth}"
        );
    }
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
        "schema_export_require_auth should default to None (required unless set)"
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

    assert!(
        config.schema_export_requires_auth(),
        "schema export should require auth when explicitly set to true"
    );
}

#[test]
fn test_schema_export_require_auth_unset_is_required_whatever_introspection_says() {
    // Unset, it requires auth: it serves the schema, and opening introspection does
    // not open it.
    for introspection_require_auth in [true, false] {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth,
            schema_export_require_auth: None,
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(
            config.schema_export_requires_auth(),
            "introspection_require_auth = {introspection_require_auth}"
        );
    }
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

// =============================================================================
// Playground Independent Auth Tests
// =============================================================================

#[test]
fn test_playground_require_auth_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(
        config.playground_require_auth.is_none(),
        "playground_require_auth should default to None (follows /graphql unless set)"
    );
}

#[test]
fn test_playground_require_auth_true_overrides_public_introspection() {
    // When introspection is public but playground_require_auth is explicitly true,
    // the playground should require auth independently.
    // playground_enabled is false here — we're testing the option logic, not routing.
    let config = ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        playground_require_auth: Some(true),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!(
            "Config with playground_require_auth=true, introspection public should validate: {e}"
        )
    });

    // Some(true) wins even where /graphql admits anonymous callers
    assert!(
        config.playground_requires_auth(false),
        "playground should require auth when explicitly set to true"
    );
}

#[test]
fn test_playground_require_auth_unset_follows_graphql() {
    // Unset, it admits exactly the callers /graphql admits, whatever introspection says.
    for introspection_require_auth in [true, false] {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth,
            playground_require_auth: None,
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.playground_requires_auth(true), "/graphql requires auth");
        assert!(!config.playground_requires_auth(false), "/graphql admits anonymous callers");
    }
}

#[test]
fn test_playground_require_auth_absent_in_toml_deserializes_as_none() {
    let toml_str = r#"
schema_path = "schema.compiled.json"
database_url = "postgres://localhost/test"
introspection_enabled = true
introspection_require_auth = false
"#;
    let config: ServerConfig = toml::from_str(toml_str).expect("Deserialization should work");
    assert!(
        config.playground_require_auth.is_none(),
        "absent playground_require_auth field should deserialize as None"
    );
}

// =============================================================================
// Subscription Independent Auth Tests
// =============================================================================

#[test]
fn test_subscription_require_auth_defaults_to_none() {
    let config = ServerConfig::default();
    assert!(
        config.subscription_require_auth.is_none(),
        "subscription_require_auth should default to None (follows /graphql unless set)"
    );
}

#[test]
fn test_subscription_require_auth_true_overrides_public_introspection() {
    // When introspection is public but subscription_require_auth is explicitly true,
    // the subscription endpoint should require auth independently.
    let config = ServerConfig {
        subscriptions_enabled: true,
        introspection_require_auth: false,
        subscription_require_auth: Some(true),
        cors_enabled: false,
        ..ServerConfig::default()
    };

    config.validate().unwrap_or_else(|e| {
        panic!(
            "Config with subscription_require_auth=true, introspection public should validate: {e}"
        )
    });

    // Some(true) wins even where /graphql admits anonymous callers
    assert!(
        config.subscription_requires_auth(false),
        "subscription should require auth when explicitly set to true"
    );
}

#[test]
fn test_subscription_require_auth_unset_follows_graphql() {
    // Unset, it admits exactly the callers /graphql admits, whatever introspection says.
    for introspection_require_auth in [true, false] {
        let config = ServerConfig {
            subscriptions_enabled: true,
            introspection_require_auth,
            subscription_require_auth: None,
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.subscription_requires_auth(true), "/graphql requires auth");
        assert!(!config.subscription_requires_auth(false), "/graphql admits anonymous callers");
    }
}

#[test]
fn test_subscription_require_auth_absent_in_toml_deserializes_as_none() {
    let toml_str = r#"
schema_path = "schema.compiled.json"
database_url = "postgres://localhost/test"
subscriptions_enabled = true
introspection_require_auth = false
"#;
    let config: ServerConfig = toml::from_str(toml_str).expect("Deserialization should work");
    assert!(
        config.subscription_require_auth.is_none(),
        "absent subscription_require_auth field should deserialize as None"
    );
}
