#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Behavioral tests for GraphQL query parsing and schema validation.
//!
//! Exercises the real `parse_query` and `CompiledSchema` APIs
//! rather than hand-constructing JSON values.

use fraiseql_core::{graphql::parse_query, schema::FieldType};
use fraiseql_test_utils::schema_builder::{TestSchemaBuilder, TestTypeBuilder};

// ============================================================================
// PARSE QUERY
// ============================================================================

#[test]
fn parse_query_extracts_operation_type() {
    let parsed = parse_query("query { users { id name } }").unwrap();
    assert_eq!(parsed.operation_type, "query");
}

#[test]
fn parse_query_extracts_root_field() {
    let parsed = parse_query("{ users { id name } }").unwrap();
    assert_eq!(parsed.root_field, "users");
}

#[test]
fn parse_query_extracts_named_operation() {
    let parsed = parse_query("query GetUsers { users { id } }").unwrap();
    assert_eq!(parsed.operation_name, Some("GetUsers".to_string()));
}

#[test]
fn parse_query_anonymous_has_no_operation_name() {
    let parsed = parse_query("{ users { id } }").unwrap();
    assert!(parsed.operation_name.is_none());
}

#[test]
fn parse_query_rejects_malformed_input() {
    let result = parse_query("{ users { id");
    assert!(result.is_err());
}

#[test]
fn parse_query_extracts_selections() {
    let parsed = parse_query("query { users { id name email } }").unwrap();
    // The parser should capture field selections under the root field
    assert!(!parsed.selections.is_empty());
}

#[test]
fn parse_query_extracts_variables() {
    let parsed = parse_query("query GetUser($id: ID!) { user(id: $id) { name } }").unwrap();
    assert_eq!(parsed.operation_name, Some("GetUser".to_string()));
    assert!(!parsed.variables.is_empty());
    assert_eq!(parsed.variables[0].name, "id");
}

// ============================================================================
// COMPILED SCHEMA
// ============================================================================

#[test]
fn compiled_schema_new_is_empty() {
    let schema = TestSchemaBuilder::new().build();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
    assert!(schema.mutations.is_empty());
}

// Migration 8: compiled_schema_from_json_roundtrip
#[test]
fn compiled_schema_from_json_roundtrip() {
    let schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_simple_field("id", FieldType::Int)
                .with_simple_field("name", FieldType::String)
                .build(),
        )
        .with_simple_query("getUser", "User", false)
        .build();

    let json = serde_json::to_string(&schema).unwrap();
    let restored = fraiseql_core::schema::CompiledSchema::from_json(&json).unwrap();

    assert_eq!(restored.types.len(), 1);
    assert_eq!(restored.types[0].name, "User");
    assert_eq!(restored.types[0].fields.len(), 2);
    assert_eq!(restored.queries.len(), 1);
    assert_eq!(restored.queries[0].name, "getUser");
    assert_eq!(restored.queries[0].return_type, "User");
}

// Migration 9: compiled_schema_type_lookup
#[test]
fn compiled_schema_type_lookup() {
    let schema = TestSchemaBuilder::new()
        .with_empty_type("User", "v_user")
        .with_empty_type("Post", "v_post")
        .build();

    let user = schema.types.iter().find(|t| t.name == "User");
    assert!(user.is_some());

    let missing = schema.types.iter().find(|t| t.name == "NonExistent");
    assert!(missing.is_none());
}

// Migration 10: query_return_type_references_existing_type
#[test]
fn query_return_type_references_existing_type() {
    let schema = TestSchemaBuilder::new()
        .with_empty_type("User", "v_user")
        .with_simple_query("getUser", "User", false)
        .build();

    let query = &schema.queries[0];
    let type_exists = schema.types.iter().any(|t| t.name == query.return_type);
    assert!(type_exists, "Query return type should reference a defined type");
}
