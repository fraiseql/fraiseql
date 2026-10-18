#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1580: the enum positions of a request are rewritten to the values their members store.

use std::collections::HashMap;

use serde_json::json;

use super::{coerce_enum_argument_object, coerce_enum_arguments};
use crate::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldType,
    InputFieldDefinition, InputObjectDefinition,
};

/// `OrderStatus` (`PENDING = "pending"`, `SHIPPED = "shipped"`, `LOST`), and an input
/// `OrderInput { status: OrderStatus!, history: [OrderStatus!] }`.
fn schema(valued: bool) -> CompiledSchema {
    let member = |name: &str, value: &str| {
        let member = EnumValueDefinition::new(name);
        if valued {
            member.stored_as(value)
        } else {
            member
        }
    };
    let mut schema = CompiledSchema::new();
    schema.enums.push(EnumDefinition::new("OrderStatus").with_values(vec![
        member("PENDING", "pending"),
        member("SHIPPED", "shipped"),
        EnumValueDefinition::new("LOST"),
    ]));
    schema.input_types.push(
        InputObjectDefinition::new("OrderInput")
            .with_field(InputFieldDefinition::new("status", "OrderStatus!"))
            .with_field(InputFieldDefinition::new("history", "[OrderStatus!]")),
    );
    schema
}

fn declared() -> Vec<ArgumentDefinition> {
    vec![
        ArgumentDefinition::new("status", FieldType::Enum("OrderStatus".into())),
        ArgumentDefinition::new("statuses", FieldType::list(FieldType::Enum("OrderStatus".into()))),
        ArgumentDefinition::new("input", FieldType::Object("OrderInput".into())),
        ArgumentDefinition::new("note", FieldType::String),
    ]
}

#[test]
fn every_enum_position_becomes_its_stored_value() {
    let mut arguments: HashMap<String, serde_json::Value> = HashMap::from([
        ("status".to_string(), json!("SHIPPED")),
        ("statuses".to_string(), json!(["PENDING", "LOST"])),
        (
            "input".to_string(),
            json!({"status": "SHIPPED", "history": ["PENDING", "SHIPPED"]}),
        ),
        ("note".to_string(), json!("SHIPPED")),
    ]);

    coerce_enum_arguments(&schema(true), &declared(), &mut arguments);

    assert_eq!(arguments["status"], json!("shipped"));
    assert_eq!(arguments["statuses"], json!(["pending", "LOST"]), "LOST stores its name");
    assert_eq!(
        arguments["input"],
        json!({"status": "shipped", "history": ["pending", "shipped"]})
    );
    assert_eq!(arguments["note"], json!("SHIPPED"), "a String is not an enum position");
}

#[test]
fn a_bare_value_at_a_list_position_is_coerced_as_one_item() {
    let mut arguments = HashMap::from([("statuses".to_string(), json!("PENDING"))]);
    coerce_enum_arguments(&schema(true), &declared(), &mut arguments);
    assert_eq!(arguments["statuses"], json!("pending"));
}

#[test]
fn a_mutations_argument_object_is_coerced_in_a_copy() {
    let provided = json!({"status": "PENDING", "input": {"status": "SHIPPED"}});

    let coerced = coerce_enum_argument_object(&schema(true), &declared(), Some(&provided));

    assert_eq!(coerced, Some(json!({"status": "pending", "input": {"status": "shipped"}})));
    assert_eq!(provided["status"], json!("PENDING"), "the caller's map is not touched");
}

#[test]
fn a_schema_whose_enums_store_their_names_is_left_alone() {
    let mut arguments = HashMap::from([("status".to_string(), json!("SHIPPED"))]);
    coerce_enum_arguments(&schema(false), &declared(), &mut arguments);
    assert_eq!(arguments["status"], json!("SHIPPED"));
    assert_eq!(
        coerce_enum_argument_object(&schema(false), &declared(), Some(&json!({}))),
        None,
        "no copy is made"
    );
}
