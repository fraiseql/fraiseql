//! Tests for the client codegen module.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::path::PathBuf;

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition,
    FieldType, MutationDefinition, QueryDefinition, TypeDefinition,
};

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

/// The load-bearing shared-core guarantee: the `GraphQL` documents inside every
/// generated client are byte-identical, so no target language can drift into a
/// different query.
///
/// This is what `client::common` exists for. It lives here rather than under one
/// language because it is a claim about all of them at once — a copy owned by
/// the Python generator would go on passing after a fifth language shipped its
/// own document renderer.
#[test]
fn documents_are_identical_across_languages() {
    let schema = document_fixture();

    let py = super::python::generate(&schema).unwrap();
    let ts = super::typescript::generate(&schema).unwrap();
    let go = super::go::generate(&schema).unwrap();
    let rs = super::rust::generate(&schema).unwrap();

    let py_docs = docs(&py, &["queries.py", "mutations.py"], "\"\"\"", "\"\"\"");
    let ts_docs = docs(&ts, &["queries.ts", "mutations.ts"], "`", "`");
    let go_docs = docs(&go, &["queries.go", "mutations.go"], "`", "`");
    let rs_docs = docs(&rs, &["queries.rs", "mutations.rs"], "r#\"", "\"#");

    assert_eq!(py_docs.len(), 4, "fixture has 3 queries + 1 mutation: {py_docs:?}");
    assert_eq!(py_docs, ts_docs, "Python and TypeScript documents drifted");
    assert_eq!(py_docs, go_docs, "Python and Go documents drifted");
    assert_eq!(py_docs, rs_docs, "Python and Rust documents drifted");
}

/// A schema exercising every document shape the renderers can produce: required
/// and optional arguments, a list return, a Relay connection, and a mutation.
fn document_fixture() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    user.fields.push(FieldDefinition::new("email", FieldType::String));
    user.fields
        .push(FieldDefinition::new("role", FieldType::Enum("UserRole".to_string())));
    schema.types.push(user);

    schema.enums.push(EnumDefinition {
        name:        "UserRole".to_string(),
        values:      vec![EnumValueDefinition {
            name:        "ADMIN".to_string(),
            description: None,
            deprecation: None,
        }],
        description: None,
    });

    let mut get_user = QueryDefinition::new("getUser", "User");
    get_user.nullable = true;
    get_user.arguments.push(ArgumentDefinition::new("id", FieldType::Id));
    schema.queries.push(get_user);

    let mut users = QueryDefinition::new("users", "User").returning_list();
    let mut limit = ArgumentDefinition::new("limit", FieldType::Int);
    limit.nullable = true;
    users.arguments.push(limit);
    schema.queries.push(users);

    let mut relay = QueryDefinition::new("usersConnection", "User");
    relay.relay = true;
    schema.queries.push(relay);

    let mut create = MutationDefinition::new("createUser", "User");
    create.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    schema.mutations.push(create);

    schema
}

/// Extract every operation document from a generated client's operation files.
fn docs(files: &crate::Generated, names: &[&str], open: &str, close: &str) -> Vec<String> {
    names
        .iter()
        .flat_map(|name| {
            let content = files
                .get(&PathBuf::from(*name))
                .unwrap_or_else(|| panic!("missing generated file {name}"));
            extract_delimited(content, open, close)
        })
        .filter(|doc| doc.starts_with("query ") || doc.starts_with("mutation "))
        .collect()
}

/// Extract delimited blocks (documents) from generated source.
fn extract_delimited(source: &str, open: &str, close: &str) -> Vec<String> {
    let mut docs = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(close) else { break };
        docs.push(after[..end].to_string());
        rest = &after[end + close.len()..];
    }
    docs
}
