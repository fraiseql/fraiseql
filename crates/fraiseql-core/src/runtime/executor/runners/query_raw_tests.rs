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
        field("profiles", FieldType::list(FieldType::Object("Profile".to_string()))),
        field("partners", FieldType::list(FieldType::Interface("Node".to_string()))),
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
    let schema = schema();
    let mut shape = RawShape::new();
    shape.admits(selections, &schema, "User").then_some(shape.non_null)
}

#[test]
fn a_typename_of_an_object_type_is_admitted_at_every_depth_and_under_an_alias() {
    let kind = FieldSelection {
        alias: Some("kind".to_string()),
        ..leaf("__typename")
    };
    assert_eq!(admitted(&[leaf("__typename"), leaf("name")]), Some(false));
    assert_eq!(admitted(&[leaf("name"), kind.clone()]), Some(false));
    assert_eq!(admitted(&[select("profile", vec![kind, leaf("tier")])]), Some(false));
}

#[test]
fn a_typename_below_an_abstract_position_is_refused() {
    let mut schema = schema();
    let user = schema.types.iter_mut().find(|t| t.name == "User").unwrap();
    user.fields.push(field("partner", FieldType::Interface("Node".to_string())));
    user.fields.push(field("pet", FieldType::Union("Pet".to_string())));
    schema.build_indexes();
    for position in ["partner", "pet"] {
        let mut shape = RawShape::new();
        let selections = [select(position, vec![leaf("__typename")])];
        assert!(!shape.admits(&selections, &schema, "User"), "{position}");
    }
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
    assert_eq!(admitted(&[leaf("title_translations")]), None, "a translations sibling");
    assert_eq!(admitted(&[leaf("undeclared")]), None, "an undeclared field");
}

#[test]
fn an_object_is_admitted_only_through_admitted_sub_fields() {
    assert_eq!(admitted(&[leaf("profile")]), None, "an object with no sub-selection");
    assert_eq!(admitted(&[select("profile", vec![leaf("status")])]), None, "an enum below");
    assert_eq!(admitted(&[select("name", vec![leaf("x")])]), None, "a scalar with a selection");
}

#[test]
fn a_list_of_objects_is_admitted_only_through_admitted_sub_fields() {
    assert_eq!(admitted(&[select("profiles", vec![leaf("tier")])]), Some(false));
    assert_eq!(admitted(&[leaf("profiles")]), None, "a list with no sub-selection");
    assert_eq!(admitted(&[select("profiles", vec![leaf("status")])]), None, "an enum below");
    assert_eq!(admitted(&[select("partners", vec![leaf("id")])]), None, "an abstract element");
    assert_eq!(admitted(&[leaf("tags")]), None, "a list of scalars");
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
        projection:     Some(std::sync::Arc::new(SqlProjectionHint::new(
            DatabaseType::PostgreSQL,
            "JSON_OBJECT('name' VALUE \"data\"->>'name' RETURNING json)".to_string(),
            0,
        ))),
        built_in_sql:   false,
        nested:         Vec::new(),
        gated_present:  false,
        abstract_read:  false,
        response_bytes: None,
        nearest_k:      None,
        nearest_count:  None,
        decimals:       None,
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
                    decimals:     None,
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

/// A mask the projection did not write keeps the read off the raw path; one it wrote is in
/// the rows already.
#[test]
fn a_mask_is_served_raw_only_when_the_projection_wrote_it() {
    let schema = schema();
    let selections = [leaf("name"), leaf("secret")];
    let mut read = read(&schema, &selections);
    assert!(!read.selection.masks_nothing(), "the anonymous caller lacks the scope");
    assert!(!RawShape::flat(&read), "a mask left to the walk after the read");
    read.built_in_sql = true;
    assert!(RawShape::flat(&read), "a mask the projection wrote");
    read.gated_present = true;
    assert!(!RawShape::flat(&read), "a gated field needs the row, written mask or not");
}

/// A non-null field is found under its response key at any depth, and under its alias.
#[test]
fn a_non_null_field_is_found_under_its_response_key_at_any_depth() {
    let mut schema = schema();
    let profile = schema.types.iter_mut().find(|t| t.name == "Profile").unwrap();
    let mut code = field("code", FieldType::String);
    code.nullable = false;
    profile.fields.push(code);
    schema.build_indexes();
    let under = |selections: &[FieldSelection], key: &str| {
        selects_non_null_under(selections, &schema, "User", key.as_bytes())
    };
    let tier = FieldSelection {
        alias: Some("tier".to_string()),
        ..leaf("handle")
    };

    assert!(under(&[leaf("handle")], "handle"));
    assert!(under(&[tier.clone(), select("profile", vec![leaf("tier")])], "tier"));
    assert!(under(&[select("profile", vec![leaf("code")])], "code"), "below the root");
    assert!(under(&[select("profiles", vec![leaf("code")])], "code"), "in a list's elements");
    assert!(!under(&[leaf("name"), leaf("secret")], "secret"), "a nullable field");
    assert!(!under(&[select("profile", vec![leaf("tier")])], "tier"), "nullable below");
    assert!(!under(&[tier], "handle"), "a field is found under its alias, not its name");
}

/// A `null` under a non-null field's key, or under no key, may need completion; one under a
/// nullable field's key alone does not, nor any `null` of a read with no non-null field.
#[test]
fn a_null_needs_completion_under_a_non_null_key_or_none() {
    let schema = schema();
    let selections = [
        leaf("handle"),
        leaf("secret"),
        select("profile", vec![leaf("tier")]),
    ];
    let needs = |non_null: bool, texts: &[&str]| {
        RawShape {
            non_null,
            ..RawShape::new()
        }
        .may_hold_null_at_non_null(&rows(texts), &selections, &schema, "User")
    };
    assert!(!needs(true, &[r#"{"handle":"h","secret":null}"#]));
    assert!(!needs(true, &[r#"{"handle":"h","profile":{"tier":null}}"#]));
    assert!(needs(true, &[r#"{"handle":null}"#]));
    assert!(needs(true, &[r#"{"secret":null}"#, r#"{"handle":null}"#]));
    assert!(needs(true, &[r#"{"tags":["a",null]}"#]), "an element no key names");
    assert!(needs(true, &["null"]), "a row that is null");
    assert!(!needs(false, &[r#"{"handle":null}"#]), "no non-null field is selected");
}

/// A value its scalar does not hold as served (§ 3.5), under the response key of a selected
/// `Int`, `Float`, `String` or `Boolean` field, may need completion: a number no `Int` holds
/// as written, a string under an `Int`, a number under a `String`, an object under any of
/// them. One under a `Decimal`'s key does not, nor any value of a read selecting none of
/// the four. Rows copied knowing the keys and rows an adapter copied without them answer
/// alike.
#[test]
fn a_value_of_another_kind_needs_completion_under_a_scalar_key() {
    let schema = schema();
    let years = FieldSelection {
        alias: Some("years".to_string()),
        ..leaf("age")
    };
    let selections = [
        years,
        leaf("score"),
        leaf("price"),
        leaf("name"),
        leaf("active"),
    ];
    let needs = |scalars: bool, texts: &[&str]| {
        let selected = if scalars {
            &selections[..]
        } else {
            &selections[2..3]
        };
        wrong_kind(&schema, selected, texts)
    };
    let clean = r#"{"years":7,"score":1.5,"price":3000000000,"name":"x","active":true}"#;
    assert!(!needs(true, &[clean]), "every value of its own kind; a Decimal as read");
    assert!(!needs(true, &[r#"{"years":2147483647}"#]), "the top of the range");
    assert!(needs(true, &[r#"{"years":2147483648}"#]), "past it");
    assert!(needs(true, &[r#"{"years":3.0}"#]), "a whole decimal, coerced by completion");
    assert!(needs(true, &[r#"{"years":1}"#, r#"{"years":-2147483649}"#]), "a later row");
    assert!(needs(true, &[r#"{"years":"42"}"#]), "a numeric string, coerced by completion");
    assert!(needs(true, &[r#"{"score":"1.5"}"#]), "a string under a Float");
    assert!(needs(true, &[r#"{"name":7}"#]), "a number under a String");
    assert!(needs(true, &[r#"{"name":{"s":1}}"#]), "an object under a String");
    assert!(needs(true, &[r#"{"active":"true"}"#]), "a string under a Boolean");
    assert!(!needs(true, &[r#"{"price":"1.50"}"#]), "a Decimal is not adjudicated");
    assert!(
        !needs(true, &[r#"{"age":3000000000}"#]),
        "the field's name is not its response key"
    );
    assert!(!needs(false, &[r#"{"years":"x"}"#]), "no scalar field is selected");
}

/// Whether a read of `selections` on `User` over rows `texts` needs completion for a value
/// of another kind, asked as the read asks it, of rows copied knowing the keys or not as the
/// read tells them; both must answer as rows copied knowing them and rows copied without.
fn wrong_kind(schema: &CompiledSchema, selections: &[FieldSelection], texts: &[&str]) -> bool {
    let mut shape = RawShape::new();
    assert!(shape.admits(selections, schema, "User"));
    let keys = scalar_keys(selections, schema, "User");
    let mut told = RawRows::with_scalar_keys(&keys);
    for text in texts {
        told.push(text.as_bytes());
    }
    let answer = told.has_wrong_kind_under(&keys);
    let untold = rows(texts);
    assert_eq!(
        answer,
        untold.has_wrong_kind_under(&keys),
        "{texts:?}: copied knowing the keys, and not"
    );
    let (read, told_keys) = if shape.told() {
        (&told, &keys[..])
    } else {
        (&untold, &[][..])
    };
    let asked = shape.may_hold_wrong_kind(read, told_keys, selections, schema, "User");
    assert_eq!(answer, asked, "{texts:?}: as the read asks");
    answer
}

/// A read tells the adapter its scalar keys unless it selects no object field and no scalar
/// but `String`s. Such a read is answered from the rows' tallies, and walks for its keys
/// only when a row holds a value no `String` holds.
#[test]
fn a_flat_read_selecting_only_strings_tells_no_keys() {
    let schema = schema();
    let told = |selections: &[FieldSelection]| {
        let mut shape = RawShape::new();
        assert!(shape.admits(selections, &schema, "User"));
        shape.told()
    };
    assert!(!told(&[leaf("name"), leaf("price")]), "a String and a Decimal");
    assert!(!told(&[leaf("price")]), "no scalar the rule adjudicates");
    assert!(told(&[leaf("name"), leaf("age")]), "an Int beside it");
    assert!(told(&[leaf("active")]), "a Boolean");
    assert!(
        told(&[leaf("name"), select("profile", vec![leaf("tier")])]),
        "an object field: a container below a row is then no sign of a wrong kind"
    );

    let name = [leaf("name"), leaf("price")];
    for (text, answer) in [
        (r#"{"name":"x","price":"1.50"}"#, false),
        (r#"{"name":"x","price":1.5}"#, false),
        (r#"{"name":7,"price":"1"}"#, true),
        (r#"{"name":true}"#, true),
        (r#"{"name":{"s":"x"}}"#, true),
        (r#"{"name":["x"]}"#, true),
        (r#"{"name":null}"#, false),
    ] {
        assert_eq!(wrong_kind(&schema, &name, &[text]), answer, "{text}");
    }
}
