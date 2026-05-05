//! Tests for the `bulk` module.

#![allow(clippy::unwrap_used)]

use axum::http::HeaderMap;

use super::helpers::{extract_entity_from_result, has_filter_params, set_rows_affected};

// -----------------------------------------------------------------------
// has_filter_params tests
// -----------------------------------------------------------------------

#[test]
fn no_filter_params_empty() {
    assert!(!has_filter_params(&[]));
}

#[test]
fn no_filter_only_reserved() {
    let params = vec![
        ("select", "id,name"),
        ("sort", "-name"),
        ("limit", "10"),
        ("offset", "0"),
    ];
    assert!(!has_filter_params(&params));
}

#[test]
fn filter_bracket_operator() {
    let params = vec![("status[eq]", "inactive")];
    assert!(has_filter_params(&params));
}

#[test]
fn filter_json_dsl() {
    let params = vec![("filter", r#"{"status":{"eq":"inactive"}}"#)];
    assert!(has_filter_params(&params));
}

#[test]
fn filter_simple_value() {
    // Simple value param that isn't reserved → implicit eq
    let params = vec![("status", "inactive")];
    assert!(has_filter_params(&params));
}

#[test]
fn filter_mixed_with_reserved() {
    let params = vec![("limit", "10"), ("status[eq]", "inactive")];
    assert!(has_filter_params(&params));
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
