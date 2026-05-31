//! Sanity check: the reference fixture deserializes into a `CompiledSchema`.
//!
//! This is the input every client-generator test depends on, so a malformed
//! fixture should fail loudly and early rather than inside a generator test.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

#[test]
fn tutorial_fixture_deserializes() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();

    assert_eq!(schema.types.len(), 5, "Tenant, User, Post, Comment, EmailTakenError");
    assert_eq!(schema.enums.len(), 1);
    assert_eq!(schema.input_types.len(), 3);
    assert_eq!(schema.interfaces.len(), 1);
    assert_eq!(schema.unions.len(), 1);
    assert_eq!(schema.queries.len(), 3);
    assert_eq!(schema.mutations.len(), 3);

    // The error type is flagged so the generator can add the injected `status`.
    let err = schema.types.iter().find(|t| t.name.as_str() == "EmailTakenError").unwrap();
    assert!(err.is_error);

    // Relay query carries the cursor column.
    let conn = schema.queries.iter().find(|q| q.name == "postsConnection").unwrap();
    assert!(conn.relay);

    // Relationships round-trip with their cardinality.
    let user = schema.types.iter().find(|t| t.name.as_str() == "User").unwrap();
    assert_eq!(user.relationships.len(), 2);
}
