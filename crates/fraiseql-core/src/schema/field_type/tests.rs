//! Tests for the `FieldDefinition.authorize` policy-gated marker (#423).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::{FieldDefinition, FieldType};

#[test]
fn authorize_deserializes_true() {
    let json = r#"{ "name": "email", "field_type": "String", "authorize": true }"#;
    let field: FieldDefinition = serde_json::from_str(json).unwrap();
    assert!(field.authorize, "explicit authorize:true must deserialize to true");
}

#[test]
fn authorize_defaults_false_when_absent() {
    // A compiled schema that predates this field has no `authorize` key.
    let json = r#"{ "name": "id", "field_type": "Int" }"#;
    let field: FieldDefinition = serde_json::from_str(json).unwrap();
    assert!(!field.authorize, "absent authorize must default to false (back-compat)");
}

#[test]
fn authorize_false_is_not_serialized() {
    // skip_serializing_if keeps existing fixtures byte-stable: false never emits.
    let field = FieldDefinition::new("id", FieldType::Int);
    let json = serde_json::to_string(&field).unwrap();
    assert!(
        !json.contains("authorize"),
        "authorize:false must not be serialized, got: {json}"
    );
}

#[test]
fn authorize_true_round_trips() {
    let field = FieldDefinition::new("email", FieldType::String).with_authorize(true);
    let json = serde_json::to_string(&field).unwrap();
    assert!(
        json.contains("\"authorize\":true"),
        "authorize:true must serialize, got: {json}"
    );
    let back: FieldDefinition = serde_json::from_str(&json).unwrap();
    assert!(back.authorize);
}

#[test]
fn with_authorize_builder_sets_flag() {
    assert!(FieldDefinition::new("x", FieldType::Int).with_authorize(true).authorize);
    assert!(!FieldDefinition::new("x", FieldType::Int).authorize);
}
