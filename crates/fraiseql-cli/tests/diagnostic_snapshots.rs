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

use fraiseql_cli::config::toml_schema::{
    AuthorizationPolicy, FederationCircuitBreakerConfig, FederationEntity,
    FieldAuthRule, MutationDefinition, QueryDefinition, TomlSchema, TypeDefinition,
};
use std::collections::BTreeMap;

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
// 8. Empty schema with queries — queries reference default return_type "String"
//    which is not a defined type, so validation should fail
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
