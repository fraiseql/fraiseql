#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;
use crate::schema::{FieldDefinition, FieldType, QueryDefinition, TypeDefinition};

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut product = TypeDefinition::new("Product", "v_product");
    product.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::nullable("note", FieldType::String),
    ];
    schema.types.push(product);
    schema
        .queries
        .push(QueryDefinition::new("products", "Product").returning_list());
    schema
}

fn sel(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: nested,
        directives:    vec![],
    }
}

#[test]
fn a_missing_non_null_key_in_a_non_null_list_nulls_data() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1", "name": "a"}, {"id": "2"}]}});
    let selections = [sel(
        "products",
        vec![sel("id", vec![]), sel("name", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, &HashMap::new());
    assert_eq!(response["data"], Value::Null, "`[Product!]!` under the root");
    assert_eq!(response["errors"][0]["path"], json!(["products", 1, "name"]));
}

#[test]
fn every_violation_is_reported_before_the_list_is_nulled() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1"}, {"id": "2"}]}});
    let selections = [sel("products", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, &HashMap::new());
    assert_eq!(response["errors"].as_array().map(Vec::len), Some(2), "{response}");
}

#[test]
fn an_absent_nullable_field_is_left_absent() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1", "name": "a"}]}});
    let selections = [sel(
        "products",
        vec![sel("name", vec![]), sel("note", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, &HashMap::new());
    assert_eq!(response, json!({"data": {"products": [{"id": "1", "name": "a"}]}}));
}

#[test]
fn a_type_introspection_does_not_publish_is_passed_through() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"unknown": [{"name": null}]}});
    let selections = [sel("unknown", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, &HashMap::new());
    assert_eq!(response, json!({"data": {"unknown": [{"name": null}]}}));
}

#[test]
fn completion_keeps_the_documents_field_order() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"name": "a", "id": "1", "note": "n"}]}});
    let selections = [sel(
        "products",
        vec![sel("name", vec![]), sel("id", vec![]), sel("note", vec![])],
    )];
    types.complete(&mut response, "Query", &selections, &HashMap::new());
    let keys: Vec<&String> = response["data"]["products"][0].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["name", "id", "note"], "{response}");
}
