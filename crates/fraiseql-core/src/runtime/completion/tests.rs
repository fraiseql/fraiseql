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
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["data"], Value::Null, "`[Product!]!` under the root");
    assert_eq!(response["errors"][0]["path"], json!(["products", 1, "name"]));
}

#[test]
fn every_violation_is_reported_before_the_list_is_nulled() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1"}, {"id": "2"}]}});
    let selections = [sel("products", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, None);
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
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response, json!({"data": {"products": [{"id": "1", "name": "a"}]}}));
}

#[test]
fn a_type_introspection_does_not_publish_is_passed_through() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"unknown": [{"name": null}]}});
    let selections = [sel("unknown", vec![sel("name", vec![])])];
    types.complete(&mut response, "Query", &selections, None);
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
    types.complete(&mut response, "Query", &selections, None);
    let keys: Vec<&String> = response["data"]["products"][0].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["name", "id", "note"], "{response}");
}

#[test]
fn a_violation_inside_a_matching_inline_fragment_is_reported() {
    let types = OutputTypes::from_schema(&schema());
    let mut response = json!({"data": {"products": [{"id": "1"}]}});
    let selections = [sel(
        "products",
        vec![
            sel("id", vec![]),
            sel("...on Product", vec![sel("name", vec![])]),
        ],
    )];
    types.complete(&mut response, "Query", &selections, None);
    assert_eq!(response["errors"][0]["path"], json!(["products", 0, "name"]), "{response}");
}

/// § 3.5.1 at its bounds: an integer in the signed 32-bit range is served, one past it is
/// refused; a whole number written with a fraction or an exponent is that integer; a
/// fractional one is refused; a value that is not a number is not this rule's to judge.
#[test]
fn an_int_position_serves_a_32_bit_integer_coerces_a_whole_decimal_and_refuses_the_rest() {
    let cases = [
        (json!(0), IntCoercion::Served),
        (json!(2_147_483_647), IntCoercion::Served),
        (json!(-2_147_483_648), IntCoercion::Served),
        (json!(2_147_483_648_i64), IntCoercion::Refused),
        (json!(-2_147_483_649_i64), IntCoercion::Refused),
        (json!(u64::MAX), IntCoercion::Refused),
        (json!(3.0), IntCoercion::Whole(3)),
        (json!(-0.0), IntCoercion::Whole(0)),
        (serde_json::from_str::<Value>("1e3").unwrap(), IntCoercion::Whole(1000)),
        (json!(2_147_483_647.0), IntCoercion::Whole(2_147_483_647)),
        (json!(-2_147_483_648.0), IntCoercion::Whole(-2_147_483_648)),
        (json!(2_147_483_648.0), IntCoercion::Refused),
        (json!(-2_147_483_649.0), IntCoercion::Refused),
        (json!(2.5), IntCoercion::Refused),
        (json!(1.5e300), IntCoercion::Refused),
        (json!("3000000000"), IntCoercion::Served),
        (json!(true), IntCoercion::Served),
    ];
    for (value, expected) in cases {
        assert_eq!(coerce_int(&value), expected, "{value}");
    }
}

/// Aggregate rows are projected by the same `Int` rule (§ 3.5.1) before the response is
/// completed: a whole decimal is its integer, a fraction or a value past the range a field
/// error at its row's path.
#[test]
fn an_aggregate_row_projects_an_int_by_the_same_rule() {
    let mut schema = CompiledSchema::new();
    let mut row = TypeDefinition::new("CounterRow", "v_counter");
    row.fields = vec![FieldDefinition::nullable("n", FieldType::Int)];
    schema.types.push(row);
    schema.build_indexes();
    let types = OutputTypes::from_schema(&schema);
    let n = FieldSelection {
        name:          "n".to_string(),
        alias:         None,
        arguments:     Vec::new(),
        nested_fields: Vec::new(),
        directives:    Vec::new(),
    };

    let (rows, errors) = types.project_rows(
        vec![
            json!({"n": 3.0}),
            json!({"n": 2.5}),
            json!({"n": 3_000_000_000_i64}),
            json!({"n": 7}),
        ],
        "CounterRow",
        &[n],
        None,
        "c",
    );

    assert_eq!(rows, json!([{"n": 3}, {"n": null}, {"n": null}, {"n": 7}]));
    assert!(rows[0]["n"].is_i64(), "{rows}");
    let paths: Vec<&Value> = errors.iter().map(|e| &e["path"]).collect();
    assert_eq!(paths, vec![&json!(["c", 1, "n"]), &json!(["c", 2, "n"])], "{errors:?}");
}
