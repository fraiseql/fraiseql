#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Schema backward-compatibility migration tests.
//!
//! Verifies that compiled schemas produced by earlier fraiseql-cli versions can be
//! loaded by the current runtime without error. A CI failure here means a schema
//! format change broke backward compatibility and existing deployments would fail
//! on upgrade.

use fraiseql_core::schema::CompiledSchema;

/// v2.0 schema (no `schema_format_version` field) must load on the v2.1+ runtime.
///
/// Backward compatibility guarantee: schemas compiled before `schema_format_version`
/// was introduced are accepted silently. Only schemas with an *explicit* mismatched
/// version are rejected.
#[test]
fn test_v2_0_schema_loads_on_current_runtime() {
    let schema_json = include_str!("fixtures/schemas/compiled_v2_0.json");
    let result = CompiledSchema::from_json(schema_json);
    assert!(
        result.is_ok(),
        "v2.0 compiled schema failed to load on current runtime: {:?}",
        result.err()
    );
}

/// `validate_format_version()` accepts schemas without a version field (pre-v2.1).
#[test]
fn test_v2_0_schema_passes_format_version_check() {
    let schema_json = include_str!("fixtures/schemas/compiled_v2_0.json");
    let schema = CompiledSchema::from_json(schema_json).expect("schema must load");
    assert!(
        schema.schema_format_version.is_none(),
        "v2.0 fixture must not carry a schema_format_version"
    );
    assert!(
        schema.validate_format_version().is_ok(),
        "pre-versioning schema must pass validate_format_version()"
    );
}

/// All types defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_types_accessible() {
    let schema = CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"))
        .expect("schema must load");

    assert_eq!(schema.types.len(), 2, "fixture has 2 types: User and Post");
    assert!(schema.find_type("User").is_some(), "User type must be findable");
    assert!(schema.find_type("Post").is_some(), "Post type must be findable");
    assert!(schema.find_type("Nonexistent").is_none(), "unknown type returns None");
}

/// All queries defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_queries_accessible() {
    let schema = CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"))
        .expect("schema must load");

    assert_eq!(schema.queries.len(), 3, "fixture has 3 queries");
    assert!(schema.find_query("users").is_some(), "users query must be findable");
    assert!(schema.find_query("user").is_some(), "user query must be findable");
    assert!(schema.find_query("posts").is_some(), "posts query must be findable");
    assert!(schema.find_query("nonexistent").is_none(), "unknown query returns None");
}

/// All mutations defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_mutations_accessible() {
    let schema = CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"))
        .expect("schema must load");

    assert_eq!(schema.mutations.len(), 3, "fixture has 3 mutations");
    assert!(schema.find_mutation("createUser").is_some(), "createUser must be findable");
    assert!(schema.find_mutation("updateUser").is_some(), "updateUser must be findable");
    assert!(schema.find_mutation("deleteUser").is_some(), "deleteUser must be findable");
}

/// Optional fields absent in v2.0 schema deserialize to None, not errors.
#[test]
fn test_v2_0_schema_optional_fields_are_none() {
    let schema = CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"))
        .expect("schema must load");

    assert!(schema.federation.is_none(), "no federation config in v2.0");
    assert!(schema.security.is_none(), "no security config in v2.0");
    assert!(schema.mcp_config.is_none(), "no MCP config in v2.0");
    assert!(schema.observers_config.is_none(), "no observers config in v2.0");
    assert!(schema.schema_format_version.is_none(), "no version field in v2.0");
}

/// Enum types defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_enums_accessible() {
    let schema = CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"))
        .expect("schema must load");

    assert_eq!(schema.enums.len(), 1, "fixture has 1 enum: UserRole");
    assert!(schema.find_enum("UserRole").is_some(), "UserRole enum must be findable");
    let role = schema.find_enum("UserRole").unwrap();
    assert_eq!(role.values.len(), 3, "UserRole has 3 values: ADMIN, EDITOR, VIEWER");
}

/// A schema with an explicit mismatched format version is rejected.
///
/// This is the forward-incompatibility guarantee: future schema versions with
/// structural changes are rejected rather than silently misinterpreted.
#[test]
fn test_mismatched_format_version_is_rejected() {
    let schema_json = r#"{
        "types": [],
        "queries": [],
        "mutations": [],
        "subscriptions": [],
        "schema_format_version": 9999
    }"#;
    let schema = CompiledSchema::from_json(schema_json).expect("JSON is valid — must parse");
    let version_check = schema.validate_format_version();
    assert!(
        version_check.is_err(),
        "schema with future format version 9999 must be rejected"
    );
    let err = version_check.unwrap_err();
    assert!(
        err.contains("9999"),
        "error message must mention the mismatched version: {err}"
    );
}
