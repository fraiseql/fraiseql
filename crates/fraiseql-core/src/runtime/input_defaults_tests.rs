#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! What an input object's omitted, sent and null fields become.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{apply_input_defaults, apply_input_defaults_to_object};
use crate::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, InputFieldDefinition, InputObjectDefinition,
};

/// `OrderInput { buyer: String!, currency: String = "EUR", rate: Float! = 0.2,
/// lines: [LineInput!] }` and `LineInput { sku: String!, qty: Int! = 1 }`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut line = InputObjectDefinition::new("LineInput");
    line.fields = vec![
        InputFieldDefinition::new("sku", "String!").with_nullable(false),
        InputFieldDefinition::new("qty", "Int!")
            .with_nullable(false)
            .with_default_value("1"),
    ];
    let mut order = InputObjectDefinition::new("OrderInput");
    order.fields = vec![
        InputFieldDefinition::new("buyer", "String!").with_nullable(false),
        InputFieldDefinition::new("currency", "String").with_default_value("\"EUR\""),
        InputFieldDefinition::new("rate", "Float!")
            .with_nullable(false)
            .with_default_value("0.2"),
        InputFieldDefinition::new("lines", "[LineInput!]"),
    ];
    schema.input_types = vec![line, order];
    schema.build_indexes();
    schema
}

fn declared() -> Vec<ArgumentDefinition> {
    vec![ArgumentDefinition::new(
        "input",
        FieldType::Object("OrderInput".to_string()),
    )]
}

fn applied(input: Value) -> Result<Value, String> {
    apply_input_defaults_to_object(
        &schema(),
        "Mutation.createOrder",
        &declared(),
        Some(&Value::Object(std::iter::once(("input".to_string(), input)).collect())),
    )
    .map(|out| out.expect("the schema declares defaults")["input"].clone())
    .map_err(|e| e.to_string())
}

#[test]
fn an_omitted_field_takes_its_default() {
    assert_eq!(
        applied(json!({ "buyer": "c" })),
        Ok(json!({ "buyer": "c", "currency": "EUR", "rate": 0.2 }))
    );
}

#[test]
fn a_sent_value_and_a_sent_null_on_a_nullable_field_are_kept() {
    assert_eq!(
        applied(json!({ "buyer": "c", "currency": null, "rate": 0.1 })),
        Ok(json!({ "buyer": "c", "currency": null, "rate": 0.1 }))
    );
}

#[test]
fn a_null_sent_for_a_non_null_field_with_a_default_is_refused() {
    let refused = applied(json!({ "buyer": "c", "rate": null })).expect_err("non-null");
    for part in ["`rate`", "`OrderInput`", "non-null", "omitted"] {
        assert!(refused.contains(part), "names {part}: {refused}");
    }
}

#[test]
fn each_item_of_a_list_of_input_objects_takes_its_defaults() {
    assert_eq!(
        applied(json!({ "buyer": "c", "lines": [{ "sku": "a" }, { "sku": "b", "qty": 3 }] }))
            .map(|input| input["lines"].clone()),
        Ok(json!([{ "sku": "a", "qty": 1 }, { "sku": "b", "qty": 3 }]))
    );
}

#[test]
fn a_field_sent_under_its_stored_key_is_not_defaulted_again() {
    let mut schema = schema();
    schema.input_types[1].fields[1].name = "currency_code".to_string();
    schema.build_indexes();
    let out = apply_input_defaults_to_object(
        &schema,
        "Mutation.createOrder",
        &declared(),
        Some(&json!({ "input": { "buyer": "c", "currency_code": "USD" } })),
    )
    .unwrap()
    .unwrap();

    assert_eq!(out["input"]["currency_code"], "USD");
    assert_eq!(out["input"].as_object().unwrap().len(), 3, "no second key: {out}");
}

#[test]
fn an_object_with_a_key_its_type_does_not_declare_is_left_as_sent() {
    assert_eq!(
        applied(json!({ "buyer": "c", "lines": { "name": "desc" } }))
            .map(|input| input["lines"].clone()),
        Ok(json!({ "name": "desc" }))
    );
}

#[test]
fn an_absent_argument_is_left_to_the_arguments_own_default() {
    let out = apply_input_defaults_to_object(
        &schema(),
        "Mutation.createOrder",
        &declared(),
        Some(&json!({})),
    )
    .unwrap()
    .unwrap();
    assert_eq!(out, json!({}));
}

#[test]
fn a_schema_without_field_defaults_is_left_alone() {
    let mut schema = schema();
    for input in &mut schema.input_types {
        for field in &mut input.fields {
            field.default_value = None;
        }
    }
    let arguments = json!({ "input": { "buyer": "c" } });
    assert_eq!(
        apply_input_defaults_to_object(
            &schema,
            "Mutation.createOrder",
            &declared(),
            Some(&arguments)
        )
        .unwrap(),
        None
    );
}

#[test]
fn a_reads_arguments_take_the_defaults_in_place() {
    let mut arguments = HashMap::from([("input".to_string(), json!({ "buyer": "c" }))]);
    apply_input_defaults(&schema(), "Query.orders", &declared(), &mut arguments).unwrap();
    assert_eq!(arguments["input"], json!({ "buyer": "c", "currency": "EUR", "rate": 0.2 }));
}

#[test]
fn a_transport_read_takes_the_defaults_with_its_client_arguments() {
    let mut schema = schema();
    let mut query = crate::schema::QueryDefinition::new("quote", "Quote")
        .returning_list()
        .with_sql_source("fn_quote");
    query.arguments = declared();
    schema.queries.push(query.clone());
    schema.build_indexes();
    let arguments = HashMap::from([("input".to_string(), json!({ "buyer": "c" }))]);

    let read = crate::runtime::QueryMatch::from_operation(query, vec![], arguments, None)
        .unwrap()
        .with_client_arguments(&schema)
        .unwrap();

    assert_eq!(read.arguments["input"], json!({ "buyer": "c", "currency": "EUR", "rate": 0.2 }));
}
