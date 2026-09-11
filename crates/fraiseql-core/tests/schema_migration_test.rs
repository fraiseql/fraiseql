#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! What a compiled schema from an earlier release meets on this runtime (#1304).
//!
//! The guarantee this file used to assert was the opposite one: that artifacts
//! produced by earlier `fraiseql-cli` versions keep loading. They do not, and
//! deliberately — a compiled schema is a build artifact of the release that
//! wrote it, and this runtime reads fields an earlier compiler never wrote,
//! reading their absence as a setting rather than as a gap. `pagination_order`
//! is the worked example (#1303): a 2.14 artifact leaves every paginated read
//! unordered on a 2.15 runtime, under a `200`.
//!
//! What is asserted instead is that the refusal is a *good* one. The artifact
//! must still parse, so an operator meets the producer check's message — which
//! names both builds and the recompile — rather than a serde error about a key.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe

use fraiseql_core::{runtime::RuntimeConfig, schema::CompiledSchema};

fn v2_0_artifact() -> CompiledSchema {
    CompiledSchema::from_json(include_str!("fixtures/schemas/compiled_v2_0.json"), false)
        .expect("a v2.0 artifact must still parse, or the refusal is a serde error")
}

/// The parse survives. Everything below depends on it: a refusal an operator can
/// act on is only reachable if the artifact got as far as the producer check.
#[test]
fn test_v2_0_schema_still_parses() {
    let schema = v2_0_artifact();
    assert!(schema.fraiseql_version.is_unstamped(), "a v2.0 artifact carries no stamp");
}

/// And then it is refused, by name, with the remedy.
#[test]
fn test_v2_0_schema_is_refused_by_the_producer_check() {
    let err = v2_0_artifact()
        .validate_producer_version()
        .expect_err("an artifact from an earlier release must be refused");

    assert!(err.contains("fraiseql_version"), "the message names the missing stamp: {err}");
    assert!(
        err.contains(env!("CARGO_PKG_VERSION")),
        "and this runtime's build, so an operator knows which CLI to use: {err}"
    );
    assert!(err.contains("Recompile"), "and the remedy: {err}");
}

/// The refusal reaches the seam every server constructor routes through, rather
/// than living only on the method nothing would have to call.
#[test]
fn test_v2_0_schema_cannot_build_a_runtime_config() {
    let err = RuntimeConfig::from_compiled_schema(&v2_0_artifact())
        .expect_err("an artifact from an earlier release must not build a config");
    assert!(err.contains("Recompile"), "{err}");
}

/// All types defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_types_accessible() {
    let schema = v2_0_artifact();

    assert_eq!(schema.types.len(), 2, "fixture has 2 types: User and Post");
    assert!(schema.find_type("User").is_some(), "User type must be findable");
    assert!(schema.find_type("Post").is_some(), "Post type must be findable");
    assert!(schema.find_type("Nonexistent").is_none(), "unknown type returns None");
}

/// All queries defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_queries_accessible() {
    let schema = v2_0_artifact();

    assert_eq!(schema.queries.len(), 3, "fixture has 3 queries");
    assert!(schema.find_query("users").is_some(), "users query must be findable");
    assert!(schema.find_query("user").is_some(), "user query must be findable");
    assert!(schema.find_query("posts").is_some(), "posts query must be findable");
    assert!(schema.find_query("nonexistent").is_none(), "unknown query returns None");
}

/// All mutations defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_mutations_accessible() {
    let schema = v2_0_artifact();

    assert_eq!(schema.mutations.len(), 3, "fixture has 3 mutations");
    assert!(schema.find_mutation("createUser").is_some(), "createUser must be findable");
    assert!(schema.find_mutation("updateUser").is_some(), "updateUser must be findable");
    assert!(schema.find_mutation("deleteUser").is_some(), "deleteUser must be findable");
}

/// Optional fields absent in the v2.0 schema deserialize to None, not errors —
/// which is exactly why the absence of a field cannot be read as a choice, and
/// why the producer check exists.
#[test]
fn test_v2_0_schema_optional_fields_are_none() {
    let schema = v2_0_artifact();

    assert!(schema.federation.is_none(), "no federation config in v2.0");
    assert!(schema.security.is_none(), "no security config in v2.0");
    assert!(schema.mcp_config.is_none(), "no MCP config in v2.0");
    assert!(schema.observers_config.is_none(), "no observers config in v2.0");
    assert!(
        schema.queries.iter().all(|q| q.pagination_order.is_none()),
        "and no page order on any query — the #1303 field this runtime would read"
    );
}

/// Enum types defined in the v2.0 fixture are accessible after loading.
#[test]
fn test_v2_0_schema_enums_accessible() {
    let schema = v2_0_artifact();

    // The fixture declares one enum. Loading also *derives* `SortDirection`,
    // because the fixture's queries enable `orderBy` (#1154) — so this asserts
    // the authored enum survived rather than counting the whole list, which
    // would be a count of the derivation and not of the fixture.
    let authored: Vec<&str> = schema
        .enums
        .iter()
        .map(|e| e.name.as_str())
        .filter(|name| *name != fraiseql_core::schema::derived_inputs::SORT_DIRECTION_ENUM)
        .collect();
    assert_eq!(authored, ["UserRole"], "fixture has 1 authored enum: UserRole");
    assert!(schema.find_enum("UserRole").is_some(), "UserRole enum must be findable");
    let role = schema.find_enum("UserRole").unwrap();
    assert_eq!(role.values.len(), 3, "UserRole has 3 values: ADMIN, EDITOR, VIEWER");
}

/// An artifact that *does* name its build, but not this one, is refused with the
/// producing build named — the case an operator hits after upgrading the server
/// without recompiling.
#[test]
fn test_an_artifact_from_a_named_other_build_is_refused() {
    let schema_json = r#"{
        "types": [],
        "queries": [],
        "mutations": [],
        "subscriptions": [],
        "fraiseql_version": "2.14.0"
    }"#;
    let schema = CompiledSchema::from_json(schema_json, false).expect("JSON is valid — must parse");

    let err = schema.validate_producer_version().expect_err("another build must be refused");
    assert!(err.contains("2.14.0"), "error must name the producing build: {err}");
    assert!(err.contains(env!("CARGO_PKG_VERSION")), "and this runtime's: {err}");
}
