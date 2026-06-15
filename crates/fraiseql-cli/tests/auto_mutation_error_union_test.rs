#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::pedantic)]
//! Compiler tests for opt-in mutation-error-union synthesis
//! (`[fraiseql.mutations] auto_error_union`).
//!
//! Verifies that, when enabled, the converter:
//! - synthesizes one shared `MutationError` type (`is_error`, with status/message/
//!   httpStatus/errorClass fields),
//! - synthesizes a `<Mutation>Result` union of the success entity + `MutationError` for each
//!   object-returning mutation, and rewrites the mutation's return type,
//! - leaves mutations untouched when the flag is off, when they already return a union (explicit
//!   wins), or when they return a scalar.

use fraiseql_cli::schema::{
    ConvertOptions, SchemaConverter,
    intermediate::{IntermediateMutation, IntermediateSchema, IntermediateType, IntermediateUnion},
};
use fraiseql_core::schema::{CompiledSchema, InputStyle};
use indexmap::IndexMap;

fn object_type(name: &str) -> IntermediateType {
    IntermediateType {
        name: name.to_string(),
        ..Default::default()
    }
}

fn object_mutation(name: &str, return_type: &str) -> IntermediateMutation {
    IntermediateMutation {
        name:                    name.to_string(),
        return_type:             return_type.to_string(),
        returns_list:            false,
        nullable:                false,
        arguments:               Vec::new(),
        description:             None,
        sql_source:              Some(format!("fn_{name}")),
        operation:               Some("CUSTOM".to_string()),
        deprecated:              None,
        inject:                  IndexMap::new(),
        invalidates_fact_tables: Vec::new(),
        invalidates_views:       Vec::new(),
        changelog:               true,
        input_style:             InputStyle::Flatten,
        changelog_pre_image:     false,
    }
}

fn schema(
    types: Vec<IntermediateType>,
    mutations: Vec<IntermediateMutation>,
    unions: Vec<IntermediateUnion>,
) -> IntermediateSchema {
    IntermediateSchema {
        types,
        mutations,
        unions,
        ..Default::default()
    }
}

fn compile_with_synthesis(s: IntermediateSchema) -> CompiledSchema {
    SchemaConverter::convert_with_options(
        s,
        &ConvertOptions {
            auto_error_union: true,
        },
    )
    .expect("convert must succeed")
}

#[test]
fn synthesizes_error_union_for_object_mutation() {
    let compiled = compile_with_synthesis(schema(
        vec![object_type("User")],
        vec![object_mutation("createUser", "User")],
        vec![],
    ));

    // Shared MutationError type with the four documented fields.
    let err = compiled
        .types
        .iter()
        .find(|t| t.name.as_str() == "MutationError")
        .expect("MutationError type synthesized");
    assert!(err.is_error, "MutationError must be marked is_error");
    let fields: Vec<&str> = err.fields.iter().map(|f| f.name.as_str()).collect();
    for expected in ["status", "message", "httpStatus", "errorClass"] {
        assert!(fields.contains(&expected), "MutationError missing field {expected}");
    }

    // Per-mutation union of [success entity, MutationError].
    let union = compiled
        .unions
        .iter()
        .find(|u| u.name == "CreateUserResult")
        .expect("CreateUserResult union synthesized");
    assert_eq!(union.member_types, vec!["User".to_string(), "MutationError".to_string()],);

    // The mutation now returns the union, not the bare success type.
    let mutation = compiled.mutations.iter().find(|m| m.name == "createUser").unwrap();
    assert_eq!(mutation.return_type, "CreateUserResult");
}

#[test]
fn shares_one_mutation_error_type_across_mutations() {
    let compiled = compile_with_synthesis(schema(
        vec![object_type("User"), object_type("Order")],
        vec![
            object_mutation("createUser", "User"),
            object_mutation("createOrder", "Order"),
        ],
        vec![],
    ));

    let error_types = compiled.types.iter().filter(|t| t.name.as_str() == "MutationError").count();
    assert_eq!(error_types, 1, "exactly one shared MutationError");
    assert!(compiled.unions.iter().any(|u| u.name == "CreateUserResult"));
    assert!(compiled.unions.iter().any(|u| u.name == "CreateOrderResult"));
}

#[test]
fn flag_off_leaves_mutations_untouched() {
    let compiled = SchemaConverter::convert(schema(
        vec![object_type("User")],
        vec![object_mutation("createUser", "User")],
        vec![],
    ))
    .expect("convert must succeed");

    assert!(compiled.types.iter().all(|t| t.name.as_str() != "MutationError"));
    assert!(compiled.unions.iter().all(|u| u.name != "CreateUserResult"));
    let mutation = compiled.mutations.iter().find(|m| m.name == "createUser").unwrap();
    assert_eq!(mutation.return_type, "User");
}

#[test]
fn explicit_union_return_is_preserved() {
    // A mutation already returning a declared union must be left exactly as-is.
    let compiled = compile_with_synthesis(schema(
        vec![object_type("User"), object_type("EmailTakenError")],
        vec![object_mutation("createUser", "CreateUserResult")],
        vec![IntermediateUnion {
            name:         "CreateUserResult".to_string(),
            member_types: vec!["User".to_string(), "EmailTakenError".to_string()],
            description:  None,
        }],
    ));

    let union = compiled.unions.iter().find(|u| u.name == "CreateUserResult").unwrap();
    assert_eq!(
        union.member_types,
        vec!["User".to_string(), "EmailTakenError".to_string()],
        "explicit union members must be preserved",
    );
    // No shared MutationError forced when no mutation needed wrapping.
    assert!(compiled.types.iter().all(|t| t.name.as_str() != "MutationError"));
    let mutation = compiled.mutations.iter().find(|m| m.name == "createUser").unwrap();
    assert_eq!(mutation.return_type, "CreateUserResult");
}

#[test]
fn scalar_return_type_is_untouched() {
    let compiled = compile_with_synthesis(schema(
        vec![object_type("User")],
        vec![object_mutation("deleteUser", "Boolean")],
        vec![],
    ));

    let mutation = compiled.mutations.iter().find(|m| m.name == "deleteUser").unwrap();
    assert_eq!(mutation.return_type, "Boolean", "scalar return left unchanged");
    assert!(compiled.unions.iter().all(|u| u.name != "DeleteUserResult"));
}
