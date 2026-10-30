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
    backend::projection_generator::PostgresProjectionGenerator,
    graphql::{FieldSelection, selection_set::ProjectionKey},
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition},
};

/// The key of a request with no locale chain, for a caller nothing masks.
fn unmasked() -> ProjectionKey {
    ProjectionKey::new(None, [])
}

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
    let key = ProjectionKey::new(crate::runtime::localization_chain(schema), []);
    let typed = build_typed_projection_fields(selections, schema, root, &key);
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
    assert!(sql.contains("'myName' VALUE "), "output key must be the alias `myName`: {sql}");
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

    assert!(sql.contains("'myName' VALUE "), "nested output key must be the alias: {sql}");
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

    assert!(sql.contains("'fullName' VALUE "), "output key is the field name: {sql}");
    assert!(sql.contains("->>'full_name'"), "reads its own snake_case column: {sql}");
}

// ── #1117: the WHERE cast hint for pgvector fields ───────────────────────────

/// Every pgvector field type maps to `ScalarFieldType::Vector`, which is what
/// tells a threshold predicate that a native column exists to read.
///
/// ⚠ This asserts over the **list**, not over one member, on purpose.
/// `field_type_to_where_type` ends in a wildcard arm, so a fifth pgvector type
/// added to `FieldType` would fall through to `Text` and silently keep the
/// 122×-slower JSONB operand rather than failing to compile. The list here is
/// the thing a person has to extend, and the count assertion below is what makes
/// forgetting it visible.
#[test]
fn vector_field_types_are_not_absorbed_by_the_wildcard() {
    use super::field_type_to_where_type;
    use crate::{db::ScalarFieldType, schema::FieldType as FT};

    let vector_types = [FT::Vector, FT::BitVector, FT::HalfVector, FT::SparseVector];
    for ft in &vector_types {
        assert_eq!(
            field_type_to_where_type(ft),
            ScalarFieldType::Vector,
            "{ft:?} must resolve to the native column, not to data->>"
        );
    }

    // The count is pinned separately from the mapping: the loop above passes
    // over an empty list too, and it would pass over a list that quietly lost a
    // member. `FieldType` has exactly these four pgvector variants — grep it
    // when this fails, then extend both the match arm and this list.
    assert_eq!(vector_types.len(), 4, "FieldType's pgvector variants");

    // The complement, so "everything maps to Vector" cannot pass this test.
    assert_eq!(field_type_to_where_type(&FT::String), ScalarFieldType::Text);
    assert_eq!(field_type_to_where_type(&FT::Int), ScalarFieldType::Integer);
    assert_eq!(field_type_to_where_type(&FT::Id), ScalarFieldType::Uuid);
}

/// An enum is composite as far as `FieldType::is_scalar` goes, and it is its stored value:
/// an enum field reads `data->'kind'`, never a nested object built from no sub-fields.
#[test]
fn an_enum_field_projects_its_stored_value() {
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("Zone", "v_zone")
            .with_field(FieldDefinition::new("id", FieldType::Int))
            .with_field(FieldDefinition::new("kind", FieldType::Enum("Kind".to_string()))),
    );

    let sql =
        projection_sql(&[field("id", None, vec![]), field("kind", None, vec![])], &schema, "Zone");

    assert!(sql.contains("'kind' VALUE \"data\"->'kind'"), "{sql}");
    assert!(!sql.contains("JSON_OBJECT(RETURNING json)"), "{sql}");
}

/// `Ledger { total: Decimal, count: Int, entry: Entry, lines: [Entry] }` with
/// `Entry { amount: Decimal, memo: String }`, and the union `Party` of `Entry`.
fn decimal_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut ledger = TypeDefinition::new("Ledger", "v_ledger");
    ledger.fields = vec![
        FieldDefinition::new("total", FieldType::Decimal),
        FieldDefinition::new("count", FieldType::Int),
        FieldDefinition::nullable("entry", FieldType::Object("Entry".to_string())),
        FieldDefinition::new(
            "lines",
            FieldType::List {
                item:           Box::new(FieldType::Object("Entry".to_string())),
                nullable_items: true,
            },
        ),
    ];
    let mut entry = TypeDefinition::new("Entry", "v_entry");
    entry.fields = vec![
        FieldDefinition::new("amount", FieldType::Decimal),
        FieldDefinition::new("memo", FieldType::String),
        FieldDefinition::new(
            "splits",
            FieldType::List {
                item:           Box::new(FieldType::Decimal),
                nullable_items: true,
            },
        ),
    ];
    schema.types = vec![ledger, entry];
    let mut party = crate::schema::UnionDefinition::new("Party");
    party.member_types = vec!["Entry".to_string()];
    schema.unions.push(party);
    schema
}

/// Whether `selections`, built into projection fields of `parent` as a read builds them,
/// print every `Decimal` they can hold as text.
fn decimals_as_text(selections: &[FieldSelection], parent: &str) -> bool {
    let schema = decimal_schema();
    let fields = build_typed_projection_fields(selections, &schema, parent, &unmasked());
    super::projects_decimals_as_text(
        &fields,
        &schema,
        parent,
        &crate::runtime::DecimalIndex::new(&schema),
    )
}

#[test]
fn decimal_leaves_and_objects_built_from_them_are_printed_as_text() {
    let leaves = [field("total", None, vec![]), field("count", None, vec![])];
    assert!(decimals_as_text(&leaves, "Ledger"));
    let nested = [field("entry", None, vec![field("amount", None, vec![])])];
    assert!(decimals_as_text(&nested, "Ledger"));
    assert!(decimals_as_text(&[field("memo", None, vec![])], "Entry"));
}

#[test]
fn a_list_or_an_undeclared_parent_can_hold_a_decimal_as_a_number() {
    let list = [field("lines", None, vec![field("memo", None, vec![])])];
    assert!(!decimals_as_text(&list, "Ledger"), "a list is read as its stored array");
    let nested_list = [field("entry", None, vec![field("splits", None, vec![])])];
    assert!(
        !decimals_as_text(&nested_list, "Ledger"),
        "an object is as covered as its fields"
    );
    // Below a union, the builder finds no type, so a member's field is read with `->`.
    assert!(!decimals_as_text(&[field("amount", None, vec![])], "Party"));
    // An object read without sub-fields is the stored object whole.
    let schema = decimal_schema();
    let whole = vec![crate::backend::ProjectionField::composite("entry")];
    assert!(!super::projects_decimals_as_text(
        &whole,
        &schema,
        "Ledger",
        &crate::runtime::DecimalIndex::new(&schema),
    ));
}

/// `User { name kind: __typename profile { __typename } partner { __typename } }`, `partner`
/// an interface position.
fn typename_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("profile", FieldType::Object("Profile".to_string())))
            .with_field(FieldDefinition::new("partner", FieldType::Interface("Node".to_string()))),
    );
    schema.types.push(
        TypeDefinition::new("Profile", "v_profile")
            .with_field(FieldDefinition::new("tier", FieldType::String)),
    );
    schema
}

#[test]
fn a_typename_of_an_object_type_is_projected_as_its_name_where_it_was_selected() {
    let selections = [
        field("name", None, vec![]),
        field("__typename", Some("kind"), vec![]),
        field(
            "profile",
            None,
            vec![
                field("tier", None, vec![]),
                field("__typename", None, vec![]),
            ],
        ),
    ];

    let sql = projection_sql(&selections, &typename_schema(), "User");

    assert!(
        sql.contains("'name' VALUE \"data\"->>'name', 'kind' VALUE ('User'::text), "),
        "the root's, under its alias, after `name`: {sql}"
    );
    assert!(
        sql.contains("'__typename' VALUE ('Profile'::text)"),
        "the nested level's: {sql}"
    );
    assert!(!sql.contains("->'__typename'") && !sql.contains("->>'__typename'"), "{sql}");
}

#[test]
fn a_typename_below_an_abstract_position_is_left_to_the_walk() {
    let selections = [field(
        "partner",
        None,
        vec![field("__typename", None, vec![])],
    )];

    let typed = build_typed_projection_fields(&selections, &typename_schema(), "User", &unmasked());
    let partner = typed[0].sub_fields.as_deref().unwrap_or_default();
    assert!(partner.is_empty(), "an interface position writes no literal: {partner:?}");

    let at_an_abstract_root = build_typed_projection_fields(
        &[field("__typename", None, vec![])],
        &typename_schema(),
        "Node",
        &unmasked(),
    );
    assert!(at_an_abstract_root.is_empty(), "{at_an_abstract_root:?}");
}

/// A masked field is written as its masked value under its response key, its stored key
/// never read: `null`, or `[]` for a translations sibling, whose list has no `null`. A
/// field the key does not mask is read as stored.
#[test]
fn a_masked_field_is_projected_as_its_masked_value() {
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Int))
            .with_field(name),
    );
    let masked = |field: &str| ("User".to_string(), field.to_string());
    let key = ProjectionKey::new(None, [masked("name"), masked("nameTranslations")]);
    let selections = [
        field("id", None, vec![]),
        field("name", Some("label"), vec![]),
        field("nameTranslations", Some("labels"), vec![]),
    ];

    let typed = build_typed_projection_fields(&selections, &schema, "User", &key);
    let written: Vec<_> = typed
        .iter()
        .map(|f| (f.name.as_str(), f.computed.as_ref().map(|e| e.as_sql())))
        .collect();

    assert_eq!(
        written,
        [
            ("id", None),
            ("label", Some("NULL::json")),
            ("labels", Some("'[]'::json"))
        ]
    );
}
