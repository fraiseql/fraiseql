//! Tests for the query projection field builder, co-located with
//! `runners/query_projection.rs`.
//!
//! Regression coverage for #418: an aliased query field (`myName: fullName`)
//! must read the *source* JSONB column (`full_name`), not a column derived from
//! the alias (`my_name`). This mirrors the #410 fix on the mutation projector —
//! the output key is the alias, but the source key is the `snake_case` of the
//! GraphQL field name.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::build_typed_projection_fields;
use crate::{
    db::projection_generator::PostgresProjectionGenerator,
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition},
};

/// Build a bare field selection (no arguments / directives).
fn field(name: &str, alias: Option<&str>, nested: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         alias.map(str::to_string),
        arguments:     Vec::new(),
        nested_fields: nested,
        directives:    Vec::new(),
    }
}

/// Run the selection set through the projection builder + the PostgreSQL
/// `jsonb_build_object` generator and return the rendered SQL fragment.
fn projection_sql(selections: &[FieldSelection], schema: &CompiledSchema, root: &str) -> String {
    let typed = build_typed_projection_fields(selections, schema, root, 0);
    PostgresProjectionGenerator::new()
        .generate_typed_projection_sql(&typed)
        .unwrap()
}

/// #418 RED: a scalar field selected under an alias must project the *source*
/// JSONB column, not a column named after the alias.
#[test]
fn aliased_scalar_field_projects_source_column() {
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("fullName", FieldType::String)),
    );

    // Query:  { myName: fullName }
    let sql = projection_sql(&[field("fullName", Some("myName"), vec![])], &schema, "User");

    // The output key is the alias…
    assert!(sql.contains("'myName',"), "output key must be the alias `myName`: {sql}");
    // …but the JSONB column read is the SOURCE field, snake_cased.
    assert!(
        sql.contains("->>'full_name'"),
        "aliased field must read source column `full_name`: {sql}"
    );
    // It must NOT read a column derived from the alias.
    assert!(
        !sql.contains("'my_name'"),
        "aliased field must not read alias-derived column `my_name`: {sql}"
    );
}

/// #418 RED: aliasing inside a nested object must also project the source
/// sub-column (covers the recursive `render_field` path).
#[test]
fn aliased_nested_subfield_projects_source_column() {
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("profile", FieldType::Object("Profile".to_string()))),
    );
    schema.types.push(
        TypeDefinition::new("Profile", "v_profile")
            .with_field(FieldDefinition::new("fullName", FieldType::String)),
    );

    // Query:  { profile { myName: fullName } }
    let sql = projection_sql(
        &[field(
            "profile",
            None,
            vec![field("fullName", Some("myName"), vec![])],
        )],
        &schema,
        "User",
    );

    assert!(sql.contains("'myName',"), "nested output key must be the alias: {sql}");
    assert!(
        sql.contains("->>'full_name'"),
        "nested aliased field must read source column `full_name`: {sql}"
    );
    assert!(
        !sql.contains("'my_name'"),
        "nested aliased field must not read alias-derived column `my_name`: {sql}"
    );
}

/// Regression guard: a non-aliased field is unaffected (source == output).
#[test]
fn non_aliased_field_projects_its_own_column() {
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("fullName", FieldType::String)),
    );

    let sql = projection_sql(&[field("fullName", None, vec![])], &schema, "User");

    assert!(sql.contains("'fullName',"), "output key is the field name: {sql}");
    assert!(sql.contains("->>'full_name'"), "reads its own snake_case column: {sql}");
}
