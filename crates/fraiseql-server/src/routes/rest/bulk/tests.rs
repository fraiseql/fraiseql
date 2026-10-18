//! Tests for the `bulk` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::HeaderMap;

use super::helpers::{extract_entity_from_result, extract_ids, set_rows_affected};

// -----------------------------------------------------------------------
// extract_ids tests
// -----------------------------------------------------------------------
//
// `has_filter_params` used to be tested here. It is gone: it answered a *syntactic*
// question about the query string ("does this look filtered?") that disagreed with what
// actually reached SQL, which is #862. The guard now lives in `build_filter_query_match`,
// where `params.where_clause` is knowable, and is covered end-to-end against a real
// database in `tests/rest_bulk_safety_e2e_pg.rs`.

#[test]
fn extract_ids_reads_the_first_data_key() {
    let result = serde_json::json!({"data": {"items": [{"id": 1}, {"id": 2}]}});
    assert_eq!(extract_ids(&result, "id"), vec![serde_json::json!(1), serde_json::json!(2)]);
}

#[test]
fn extract_ids_skips_rows_without_the_id_field() {
    // A row we cannot identify is a row we must not mutate — it is dropped, not
    // defaulted, so it can never be passed to a per-row mutation as a null id.
    let result = serde_json::json!({"data": {"items": [{"id": 1}, {"other": 2}]}});
    assert_eq!(extract_ids(&result, "id"), vec![serde_json::json!(1)]);
}

#[test]
fn extract_ids_is_empty_for_an_empty_or_malformed_envelope() {
    assert!(extract_ids(&serde_json::json!({"data": {"items": []}}), "id").is_empty());
    assert!(extract_ids(&serde_json::json!({}), "id").is_empty());
}

// -----------------------------------------------------------------------
// extract_entity_from_result tests
// -----------------------------------------------------------------------

#[test]
fn extract_entity_nested_format() {
    let result: serde_json::Value =
        serde_json::from_str(r#"{"data":{"createUser":{"entity":{"id":1,"name":"Alice"}}}}"#)
            .unwrap();
    let entity = extract_entity_from_result(&result).unwrap();
    assert_eq!(entity["id"], 1);
    assert_eq!(entity["name"], "Alice");
}

#[test]
fn extract_entity_executor_format() {
    let result: serde_json::Value = serde_json::from_str(
        r#"{"data":{"createUser":{"pk_user_id":1,"name":"Alice","__typename":"User"}}}"#,
    )
    .unwrap();
    let entity = extract_entity_from_result(&result).unwrap();
    assert_eq!(entity["pk_user_id"], 1);
    assert!(entity.get("__typename").is_none());
}

#[test]
fn extract_entity_null() {
    let result: serde_json::Value =
        serde_json::from_str(r#"{"data":{"createUser":{"entity":null}}}"#).unwrap();
    assert!(extract_entity_from_result(&result).is_none());
}

#[test]
fn extract_entity_null_value() {
    assert!(extract_entity_from_result(&serde_json::Value::Null).is_none());
}

// -----------------------------------------------------------------------
// X-Rows-Affected header tests
// -----------------------------------------------------------------------

#[test]
fn rows_affected_header() {
    let mut headers = HeaderMap::new();
    set_rows_affected(&mut headers, 42);
    assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "42");
}

#[test]
fn rows_affected_zero() {
    let mut headers = HeaderMap::new();
    set_rows_affected(&mut headers, 0);
    assert_eq!(headers.get("x-rows-affected").unwrap().to_str().unwrap(), "0");
}

// -----------------------------------------------------------------------
// #1580: the rows a bulk filter selects are compared by stored enum value
// -----------------------------------------------------------------------

#[test]
fn a_bulk_filter_on_a_valued_enum_reaches_sql_as_the_stored_value() {
    use std::sync::Arc;

    use fraiseql_core::{
        runtime::Executor,
        schema::{CompiledSchema, EnumDefinition, EnumValueDefinition, FieldType, RestConfig},
    };
    use fraiseql_test_utils::{
        failing_adapter::FailingAdapter,
        schema_builder::{TestFieldBuilder, TestQueryBuilder, TestTypeBuilder},
    };

    use crate::routes::rest::resource::RestRouteTable;

    let mut schema = CompiledSchema::new();
    schema.queries.push(
        TestQueryBuilder::new("orders", "Order")
            .returns_list(true)
            .with_sql_source("v_order")
            .build(),
    );
    schema.types.push(
        TestTypeBuilder::new("Order", "v_order")
            .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
            .with_field(
                TestFieldBuilder::new("status", FieldType::Enum("OrderStatus".into())).build(),
            )
            .build(),
    );
    schema.enums.push(
        EnumDefinition::new("OrderStatus")
            .with_values(vec![EnumValueDefinition::new("SHIPPED").stored_as("shipped")]),
    );
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });
    schema.build_indexes();
    let executor = Arc::new(Executor::new(schema.clone(), Arc::new(FailingAdapter::new())));
    let route_table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let config = schema.rest_config.clone().unwrap();
    let bulk = super::BulkHandler::new(&executor, &schema, &config, &route_table);

    let selection = bulk
        .build_filter_query_match("orders", &[("status[eq]", "SHIPPED")], "Order", "no filter")
        .unwrap();

    assert_eq!(selection.arguments["where"], serde_json::json!({"status": {"eq": "shipped"}}));
}
