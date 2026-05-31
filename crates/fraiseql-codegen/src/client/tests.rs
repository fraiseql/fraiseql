//! Tests for the client codegen module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::{CompiledSchema, QueryDefinition, TypeDefinition};

use super::schema_hash;

#[test]
fn schema_hash_is_stable_and_hex_encoded() {
    let schema = CompiledSchema::default();
    let first = schema_hash(&schema).unwrap();
    let second = schema_hash(&schema).unwrap();

    assert_eq!(first, second, "hashing the same schema must be deterministic");
    assert_eq!(first.len(), 64, "sha256 hex digest is 64 characters");
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn schema_hash_changes_when_schema_changes() {
    let empty = CompiledSchema::default();

    let mut populated = CompiledSchema::default();
    populated.types.push(TypeDefinition::new("User", "v_user"));
    populated.queries.push(QueryDefinition::new("users", "User"));

    assert_ne!(
        schema_hash(&empty).unwrap(),
        schema_hash(&populated).unwrap(),
        "different schemas must hash differently"
    );
}
