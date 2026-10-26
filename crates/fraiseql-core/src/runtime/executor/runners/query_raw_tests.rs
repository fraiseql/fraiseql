//! Which reads are answered as the bytes the database serialized, and the envelope they
//! are spliced into.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::{
    backend::{
        ComposedEmbed, ComposedLevel, EmbedShape, EmbedSource, LevelKeys,
        types::{DatabaseType, sql_hints::SqlProjectionHint},
    },
    graphql::GraphQLArgument,
    runtime::executor::runners::query_nested::SelectionAccess,
    schema::{FieldDefinition, FieldDenyPolicy, FieldEncryptionConfig, TypeDefinition},
};

fn rows(texts: &[&str]) -> RawRows {
    let mut rows = RawRows::new();
    for text in texts {
        rows.push(text.as_bytes());
    }
    rows
}

#[test]
fn a_list_is_every_row_in_brackets() {
    let body = envelope(&rows(&[r#"{"a" : 1}"#, r#"{"a" : 2}"#]), "users", true);
    assert_eq!(body, br#"{"data":{"users":[{"a":1},{"a":2}]}}"#);
    assert_eq!(envelope(&rows(&[]), "users", true), br#"{"data":{"users":[]}}"#);
}

#[test]
fn a_single_root_is_its_first_row_or_null() {
    let body = envelope(&rows(&[r#"{"a" : 1}"#, r#"{"a" : 2}"#]), "user", false);
    assert_eq!(body, br#"{"data":{"user":{"a":1}}}"#);
    assert_eq!(envelope(&rows(&[]), "user", false), br#"{"data":{"user":null}}"#);
}

fn select(name: &str, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name: name.to_string(),
        alias: None,
        arguments: Vec::new(),
        nested_fields,
        directives: Vec::new(),
    }
}

fn leaf(name: &str) -> FieldSelection {
    select(name, Vec::new())
}

fn field(name: &str, field_type: FieldType) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, field_type);
    field.nullable = true;
    field
}

/// `User`, one field of every kind the rule weighs, and `Profile` below it.
fn schema() -> CompiledSchema {
    let mut user = TypeDefinition::new("User", "v_user");
    let mut localized = field("title", FieldType::String);
    localized.localized = true;
    let mut encrypted = field("ssn", FieldType::String);
    encrypted.encryption = Some(FieldEncryptionConfig {
        key_reference: "k".to_string(),
        algorithm:     "aes-256-gcm".to_string(),
    });
    let mut authorized = field("salary", FieldType::Int);
    authorized.authorize = true;
    // A scalar the rule otherwise admits, so only the computed-field guard refuses it.
    let mut distance = field("similarity", FieldType::Int);
    distance.vector_distance = Some("embedding".to_string());
    let mut stored_alias = field("label", FieldType::String);
    stored_alias.alias = Some("stored_label".to_string());
    let mut hierarchy = field("path", FieldType::String);
    hierarchy.hierarchy = Some("ltree".to_string());
    let mut required = field("handle", FieldType::String);
    required.nullable = false;
    let mut masked = field("secret", FieldType::String);
    masked.requires_scope = Some("read:secret".to_string());
    masked.on_deny = FieldDenyPolicy::Mask;
    user.fields = vec![
        field("id", FieldType::Id),
        field("uuid", FieldType::Uuid),
        field("name", FieldType::String),
        field("age", FieldType::Int),
        field("active", FieldType::Boolean),
        field("score", FieldType::Float),
        field("price", FieldType::Decimal),
        field("meta", FieldType::Json),
        field("born", FieldType::DateTime),
        field("status", FieldType::Enum("Status".to_string())),
        field("tags", FieldType::list(FieldType::String)),
        field("profile", FieldType::Object("Profile".to_string())),
        localized,
        encrypted,
        authorized,
        distance,
        stored_alias,
        hierarchy,
        required,
        masked,
    ];
    let mut profile = TypeDefinition::new("Profile", "v_profile");
    profile.fields = vec![
        field("tier", FieldType::String),
        field("status", FieldType::Enum("Status".to_string())),
    ];
    let mut schema = CompiledSchema::new();
    schema.types.push(user);
    schema.types.push(profile);
    schema.build_indexes();
    schema
}

/// `Some(non_null)` when `selections` on `User` are admitted.
fn admitted(selections: &[FieldSelection]) -> Option<bool> {
    let mut shape = RawShape { non_null: false };
    shape.admits(selections, &schema(), "User").then_some(shape.non_null)
}

#[test]
fn stored_scalars_and_objects_built_from_their_sub_fields_are_admitted() {
    let selections = [
        leaf("id"),
        leaf("uuid"),
        leaf("name"),
        leaf("age"),
        leaf("active"),
        leaf("score"),
        leaf("price"),
        select("profile", vec![leaf("tier")]),
    ];
    assert_eq!(admitted(&selections), Some(false));
    let aliased = FieldSelection {
        alias: Some("who".to_string()),
        ..leaf("name")
    };
    assert_eq!(admitted(&[aliased]), Some(false), "a response alias is the projection's key");
}

#[test]
fn a_field_rust_would_reshape_is_refused() {
    for name in [
        "meta",
        "born",
        "status",
        "tags",
        "title",
        "ssn",
        "salary",
        "similarity",
        "label",
        "path",
    ] {
        assert_eq!(admitted(&[leaf(name)]), None, "{name}");
    }
    assert_eq!(admitted(&[leaf("__typename"), leaf("name")]), None, "__typename");
    assert_eq!(admitted(&[leaf("title_translations")]), None, "a translations sibling");
    assert_eq!(admitted(&[leaf("undeclared")]), None, "an undeclared field");
}

#[test]
fn an_object_is_admitted_only_through_admitted_sub_fields() {
    assert_eq!(admitted(&[leaf("profile")]), None, "an object with no sub-selection");
    assert_eq!(admitted(&[select("profile", vec![leaf("status")])]), None, "an enum below");
    assert_eq!(admitted(&[select("profile", vec![leaf("__typename")])]), None);
    assert_eq!(admitted(&[select("name", vec![leaf("x")])]), None, "a scalar with a selection");
}

#[test]
fn a_selection_the_projection_would_merge_or_read_with_arguments_is_refused() {
    let twice = [
        leaf("name"),
        FieldSelection {
            alias: Some("name".to_string()),
            ..leaf("id")
        },
    ];
    assert_eq!(admitted(&twice), None, "two fields under one response key");
    let with_argument = FieldSelection {
        arguments: vec![GraphQLArgument {
            name:       "locale".to_string(),
            value_type: "string".to_string(),
            value_json: "\"fr\"".to_string(),
        }],
        ..leaf("name")
    };
    assert_eq!(admitted(&[with_argument]), None, "a field with arguments");
}

#[test]
fn a_non_null_field_is_reported_at_any_depth() {
    assert_eq!(admitted(&[leaf("handle")]), Some(true));
    assert_eq!(admitted(&[leaf("name")]), Some(false));
}

/// A flat read of `User { name }`, no gate of any kind.
fn read(schema: &CompiledSchema, selections: &[FieldSelection]) -> RootRead {
    RootRead {
        sql_source:     "v_user".to_string(),
        source_args:    None,
        composed_where: None,
        order_by:       None,
        limit:          None,
        offset:         None,
        session_vars:   Vec::new(),
        leaf_fields:    selections.len(),
        selection:      SelectionAccess::classify(
            schema,
            "User",
            selections,
            selections.iter().map(|s| s.response_key().to_string()).collect(),
            None,
            None,
        )
        .unwrap(),
        projection:     Some(SqlProjectionHint::new(
            DatabaseType::PostgreSQL,
            "JSON_OBJECT('name' VALUE \"data\"->>'name' RETURNING json)".to_string(),
            0,
        )),
        nested:         Vec::new(),
        gated_present:  false,
        abstract_read:  false,
        response_bytes: None,
        nearest_k:      None,
        nearest_count:  None,
    }
}

#[test]
fn a_read_with_anything_to_do_after_the_database_is_not_flat() {
    let schema = schema();
    let name = [leaf("name")];
    assert!(RawShape::flat(&read(&schema, &name)));

    let refused: [(&str, fn(&mut RootRead)); 7] = [
        ("no projection", |r| r.projection = None),
        ("a function source", |r| r.source_args = Some(Vec::new())),
        ("a gated field", |r| r.gated_present = true),
        ("an abstract type", |r| r.abstract_read = true),
        ("a nearest search", |r| r.nearest_k = Some(3)),
        ("a counted nearest search", |r| r.nearest_count = Some(10)),
        ("a nested level", |r| {
            r.nested.push(ComposedEmbed {
                output_key: "orders".to_string(),
                shape:      EmbedShape::Many,
                source:     EmbedSource::Materialised {
                    keys: vec!["orders".to_string()],
                },
                level:      ComposedLevel {
                    view:         "v_order".to_string(),
                    projection:   None,
                    where_clause: None,
                    order_by:     None,
                    limit:        None,
                    offset:       None,
                    keyset:       None,
                    keys:         LevelKeys::Whole,
                    embeds:       Vec::new(),
                },
            });
        }),
    ];
    for (why, gate) in refused {
        let mut read = read(&schema, &name);
        gate(&mut read);
        assert!(!RawShape::flat(&read), "{why}");
    }
}

#[test]
fn a_masked_field_keeps_the_read_off_the_raw_path() {
    let schema = schema();
    let selections = [leaf("name"), leaf("secret")];
    let read = read(&schema, &selections);
    assert!(!read.selection.masks_nothing(), "the anonymous caller lacks the scope");
    assert!(!RawShape::flat(&read));
}
