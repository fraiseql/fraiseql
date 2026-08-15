//! Tests for the `TypeScript` render module.

use super::{FieldType, field_type_ts, field_type_ts_nullable, parse_input_type};
use crate::client::common::{arg_graphql_type, is_leaf};

#[test]
fn scalars_map_to_ts() {
    assert_eq!(field_type_ts(&FieldType::String), "string");
    assert_eq!(field_type_ts(&FieldType::Int), "number");
    assert_eq!(field_type_ts(&FieldType::Boolean), "boolean");
    assert_eq!(field_type_ts(&FieldType::Id), "string");
    assert_eq!(field_type_ts(&FieldType::Json), "unknown");
    assert_eq!(field_type_ts(&FieldType::DateTime), "string");
    assert_eq!(field_type_ts(&FieldType::Vector), "number[]");
}

/// Every pgvector field type renders as the surface it actually has (#959).
///
/// The wildcard arm in `field_type_ts` exists to keep a future `#[non_exhaustive]`
/// scalar compiling, and that is exactly what let `HalfVector` and `SparseVector`
/// degrade to `unknown` when they were added: the generator kept building, the
/// generated client kept type-checking, and a caller's embedding lost its type.
#[test]
fn every_vector_type_maps_to_its_own_surface() {
    // `[Float!]!` in GraphQL, whatever the column's precision.
    assert_eq!(field_type_ts(&FieldType::Vector), "number[]");
    assert_eq!(field_type_ts(&FieldType::HalfVector), "number[]");
    // Each of these is a string in its own text form: a run of 0/1 for a bit
    // vector, `{1:0.5,7:0.25}/1000` for a sparse one.
    assert_eq!(field_type_ts(&FieldType::BitVector), "string");
    assert_eq!(field_type_ts(&FieldType::SparseVector), "string");
}

#[test]
fn references_pass_through_as_names() {
    assert_eq!(field_type_ts(&FieldType::Object("User".into())), "User");
    assert_eq!(field_type_ts(&FieldType::Enum("Role".into())), "Role");
    assert_eq!(
        field_type_ts(&FieldType::List(Box::new(FieldType::Object("Post".into())))),
        "Post[]"
    );
}

#[test]
fn nullability_wraps() {
    assert_eq!(field_type_ts_nullable(&FieldType::String, false), "string");
    assert_eq!(field_type_ts_nullable(&FieldType::String, true), "string | null");
}

#[test]
fn leaf_classification() {
    assert!(is_leaf(&FieldType::String));
    assert!(is_leaf(&FieldType::Enum("Role".into())));
    assert!(is_leaf(&FieldType::List(Box::new(FieldType::String))));
    assert!(!is_leaf(&FieldType::Object("User".into())));
    assert!(!is_leaf(&FieldType::List(Box::new(FieldType::Object("Post".into())))));
}

#[test]
fn input_string_parsing_preserves_nullability() {
    let required = parse_input_type("String!");
    assert_eq!(required.ts, "string");
    assert!(required.required);

    let optional = parse_input_type("String");
    assert_eq!(optional.ts, "string");
    assert!(!optional.required);

    assert_eq!(parse_input_type("[String!]!").ts, "string[]");
    assert_eq!(parse_input_type("[String]!").ts, "(string | null)[]");
    assert!(parse_input_type("[String!]!").required);
    assert!(!parse_input_type("[String!]").required);
    assert_eq!(parse_input_type("UserRole").ts, "UserRole");
}

#[test]
fn graphql_arg_types() {
    assert_eq!(arg_graphql_type(&FieldType::Id, false), "ID!");
    assert_eq!(arg_graphql_type(&FieldType::Input("UserFilter".into()), true), "UserFilter");
}
