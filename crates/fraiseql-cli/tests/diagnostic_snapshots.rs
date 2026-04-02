#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Compiler diagnostic snapshot tests using insta.
//!
//! These tests verify that validation error messages don't regress between releases.
//! Each test exercises a specific `validate()` error path in `TomlSchema` and snapshots
//! the diagnostic message so any wording or format change is caught in review.
//!
//! To update snapshots after intentional changes:
//! ```bash
//! INSTA_UPDATE=always cargo test -p fraiseql-cli --test diagnostic_snapshots
//! ```

use std::collections::BTreeMap;

use fraiseql_cli::config::{
    SecurityConfig,
    toml_schema::{
        AuthorizationPolicy, FederationCircuitBreakerConfig, FederationEntity, FieldAuthRule,
        MutationDefinition, PerDatabaseCircuitBreakerOverride, QueryDefinition, TomlSchema,
        TypeDefinition,
    },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal schema with one type `User` and no queries/mutations.
fn base_schema_with_user() -> TomlSchema {
    let mut types = BTreeMap::new();
    types.insert("User".to_string(), TypeDefinition::default());
    TomlSchema {
        types,
        ..TomlSchema::default()
    }
}

/// Schema with `User` and `Product` types and a federation entity for `Product`.
fn schema_with_federation_entity() -> TomlSchema {
    let mut types = BTreeMap::new();
    types.insert("User".to_string(), TypeDefinition::default());
    types.insert("Product".to_string(), TypeDefinition::default());
    let mut schema = TomlSchema {
        types,
        ..TomlSchema::default()
    };
    schema.federation.entities.push(FederationEntity {
        name:       "Product".to_string(),
        key_fields: vec!["id".to_string()],
    });
    schema
}

// ===========================================================================
// Schema Validation Errors (Cycle 1)
// ===========================================================================

// ---------------------------------------------------------------------------
// 1. Query referencing an undefined type (no close match)
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_query_references_undefined_type() {
    let mut schema = base_schema_with_user();
    schema.queries.insert(
        "getPlanet".to_string(),
        QueryDefinition {
            return_type: "Planet".to_string(),
            ..QueryDefinition::default()
        },
    );

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 2. Mutation referencing an undefined type (no close match)
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_mutation_references_undefined_type() {
    let mut schema = base_schema_with_user();
    schema.mutations.insert(
        "createOrder".to_string(),
        MutationDefinition {
            return_type: "Order".to_string(),
            ..MutationDefinition::default()
        },
    );

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 3. Query with a typo close to an existing type
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_query_type_typo_suggests_correction() {
    let mut schema = base_schema_with_user();
    // "Usar" is edit-distance 1 from "User"
    schema.queries.insert(
        "getUser".to_string(),
        QueryDefinition {
            return_type: "Usar".to_string(),
            ..QueryDefinition::default()
        },
    );

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 4. Field auth referencing an undefined policy
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_field_auth_undefined_policy() {
    let mut schema = base_schema_with_user();
    schema.security.field_auth.push(FieldAuthRule {
        type_name:  "User".to_string(),
        field_name: "email".to_string(),
        policy:     "admin_only".to_string(),
    });
    // No policies defined — should fail

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 5. Field auth policy typo (close to existing policy)
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_field_auth_policy_typo_suggests_correction() {
    let mut schema = base_schema_with_user();
    schema.security.policies.push(AuthorizationPolicy {
        name:              "admin_only".to_string(),
        policy_type:       "RBAC".to_string(),
        rule:              None,
        roles:             vec!["admin".to_string()],
        strategy:          None,
        attributes:        vec![],
        description:       None,
        cache_ttl_seconds: None,
    });
    // Typo: "admin_onyl" — edit distance 2 from "admin_only"
    schema.security.field_auth.push(FieldAuthRule {
        type_name:  "User".to_string(),
        field_name: "email".to_string(),
        policy:     "admin_onyl".to_string(),
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 6. Federation entity referencing an undefined type
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_federation_entity_undefined_type() {
    let mut schema = base_schema_with_user();
    schema.federation.entities.push(FederationEntity {
        name:       "Product".to_string(),
        key_fields: vec!["id".to_string()],
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 7. Malformed TOML — parse error diagnostic
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_malformed_toml_parse_error() {
    let bad_toml = r#"
[schema
name = "broken"
"#;
    let err = TomlSchema::parse_toml(bad_toml).unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 8. Empty schema with queries — queries reference default return_type "String" which is not a
//    defined type, so validation should fail
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_empty_schema_with_queries() {
    let mut schema = TomlSchema::default();
    schema.queries.insert(
        "getItems".to_string(),
        QueryDefinition {
            return_type: "Item".to_string(),
            return_array: true,
            ..QueryDefinition::default()
        },
    );

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 9. Circuit breaker: failure_threshold = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_zero_failure_threshold() {
    let mut schema = base_schema_with_user();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        failure_threshold: 0,
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 10. Database pool_min > pool_max
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_database_pool_min_exceeds_pool_max() {
    let mut schema = base_schema_with_user();
    schema.database.pool_min = 50;
    schema.database.pool_max = 10;

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ===========================================================================
// Configuration Validation Errors (Cycle 2)
// ===========================================================================

// ---------------------------------------------------------------------------
// 11. Unknown TOML key (deny_unknown_fields)
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_unknown_toml_key() {
    let bad_toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"
bogus_field = true
"#;
    let err = TomlSchema::parse_toml(bad_toml).unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 12. Server port = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_server_port_zero() {
    let mut schema = base_schema_with_user();
    schema.server.port = 0;

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 13. TLS enabled but cert_file missing
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_tls_missing_cert_file() {
    let mut schema = base_schema_with_user();
    schema.server.tls.enabled = true;
    schema.server.tls.key_file = "key.pem".to_string();
    // cert_file left empty

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 14. TLS enabled but key_file missing
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_tls_missing_key_file() {
    let mut schema = base_schema_with_user();
    schema.server.tls.enabled = true;
    schema.server.tls.cert_file = "cert.pem".to_string();
    // key_file left empty

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 15. TLS invalid min_version
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_tls_invalid_min_version() {
    let mut schema = base_schema_with_user();
    schema.server.tls.enabled = true;
    schema.server.tls.cert_file = "cert.pem".to_string();
    schema.server.tls.key_file = "key.pem".to_string();
    schema.server.tls.min_version = "1.0".to_string();

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 16. Database invalid ssl_mode
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_database_invalid_ssl_mode() {
    let mut schema = base_schema_with_user();
    schema.database.ssl_mode = "bogus".to_string();

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 17. Circuit breaker: recovery_timeout_secs = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_zero_recovery_timeout() {
    let mut schema = base_schema_with_user();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        recovery_timeout_secs: 0,
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 18. Circuit breaker: success_threshold = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_zero_success_threshold() {
    let mut schema = base_schema_with_user();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        success_threshold: 0,
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 19. Circuit breaker per_database: unknown entity
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_per_database_unknown_entity() {
    let mut schema = schema_with_federation_entity();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        per_database: vec![PerDatabaseCircuitBreakerOverride {
            database:              "NonExistent".to_string(),
            failure_threshold:     Some(3),
            recovery_timeout_secs: None,
            success_threshold:     None,
        }],
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 20. Circuit breaker per_database: failure_threshold = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_per_database_zero_failure_threshold() {
    let mut schema = schema_with_federation_entity();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        per_database: vec![PerDatabaseCircuitBreakerOverride {
            database:              "Product".to_string(),
            failure_threshold:     Some(0),
            recovery_timeout_secs: None,
            success_threshold:     None,
        }],
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 21. Circuit breaker per_database: recovery_timeout_secs = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_per_database_zero_recovery_timeout() {
    let mut schema = schema_with_federation_entity();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        per_database: vec![PerDatabaseCircuitBreakerOverride {
            database:              "Product".to_string(),
            failure_threshold:     None,
            recovery_timeout_secs: Some(0),
            success_threshold:     None,
        }],
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 22. Circuit breaker per_database: success_threshold = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_circuit_breaker_per_database_zero_success_threshold() {
    let mut schema = schema_with_federation_entity();
    schema.federation.circuit_breaker = Some(FederationCircuitBreakerConfig {
        per_database: vec![PerDatabaseCircuitBreakerOverride {
            database:              "Product".to_string(),
            failure_threshold:     None,
            recovery_timeout_secs: None,
            success_threshold:     Some(0),
        }],
        ..FederationCircuitBreakerConfig::default()
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ===========================================================================
// Security Configuration Errors (Cycle 2 continued)
// ===========================================================================

// ---------------------------------------------------------------------------
// 23. SecurityConfig: leak_sensitive_details = true
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_leak_sensitive_details() {
    let mut config = SecurityConfig::default();
    config.error_sanitization.leak_sensitive_details = true;

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 24. SecurityConfig: rate limit window = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_rate_limit_zero_window() {
    let mut config = SecurityConfig::default();
    config.rate_limiting.auth_start_window_secs = 0;

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 25. SecurityConfig: rate limit max_requests = 0
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_rate_limit_zero_max_requests() {
    let mut config = SecurityConfig::default();
    config.rate_limiting.auth_callback_max_requests = 0;

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 26. SecurityConfig: unsupported encryption algorithm
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_unsupported_encryption_algorithm() {
    let mut config = SecurityConfig::default();
    config.state_encryption.algorithm = "rot13".to_string();

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 27. SecurityConfig: invalid key_size
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_invalid_key_size() {
    let mut config = SecurityConfig::default();
    config.state_encryption.key_size = 20;

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 28. SecurityConfig: invalid nonce_size
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_invalid_nonce_size() {
    let mut config = SecurityConfig::default();
    config.state_encryption.nonce_size = 16;

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 29. SecurityConfig: role with empty name
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_role_empty_name() {
    let mut config = SecurityConfig::default();
    config
        .role_definitions
        .push(fraiseql_cli::config::security::RoleDefinitionConfig {
            name:        String::new(),
            description: None,
            scopes:      vec!["read:*".to_string()],
        });

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 30. SecurityConfig: role with no scopes
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_security_role_no_scopes() {
    let mut config = SecurityConfig::default();
    config
        .role_definitions
        .push(fraiseql_cli::config::security::RoleDefinitionConfig {
            name:        "viewer".to_string(),
            description: None,
            scopes:      vec![],
        });

    let err = config.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ===========================================================================
// Mutation typo with suggestion (Cycle 1 continued)
// ===========================================================================

// ---------------------------------------------------------------------------
// 31. Mutation with a typo close to an existing type
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_mutation_type_typo_suggests_correction() {
    let mut schema = base_schema_with_user();
    // "Uzer" is edit-distance 1 from "User"
    schema.mutations.insert(
        "updateUser".to_string(),
        MutationDefinition {
            return_type: "Uzer".to_string(),
            ..MutationDefinition::default()
        },
    );

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

// ---------------------------------------------------------------------------
// 32. Federation entity typo suggests correction
// ---------------------------------------------------------------------------

#[test]
fn diagnostic_federation_entity_typo_suggests_correction() {
    let mut schema = base_schema_with_user();
    // "Usar" is close to "User"
    schema.federation.entities.push(FederationEntity {
        name:       "Usar".to_string(),
        key_fields: vec!["id".to_string()],
    });

    let err = schema.validate().unwrap_err();
    insta::assert_snapshot!(err.to_string());
}
