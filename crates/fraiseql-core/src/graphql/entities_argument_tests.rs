//! The list is read from the argument as a transport hands it over: the request's own
//! variables, defaults not yet applied.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::entities_representations;

const BY_R: &str = r#"query($r: [_Any!]! = [{ __typename: "User", id: 1 }]) { _entities(representations: $r) { __typename } }"#;

#[test]
fn an_omitted_variable_takes_its_declared_default() {
    let list = entities_representations(BY_R, None, None).unwrap();
    assert_eq!(list, json!([{"__typename": "User", "id": 1}]));
}

#[test]
fn a_supplied_variable_overrides_its_default() {
    let variables = json!({"r": [{"__typename": "Order", "id": 2}]});
    let list = entities_representations(BY_R, None, Some(&variables)).unwrap();
    assert_eq!(list, json!([{"__typename": "Order", "id": 2}]));
}

#[test]
fn a_variable_named_representations_is_not_the_argument() {
    let variables = json!({"representations": [{"__typename": "Order", "id": 2}]});
    let list = entities_representations(BY_R, None, Some(&variables)).unwrap();
    assert_eq!(list, json!([{"__typename": "User", "id": 1}]));
}

#[test]
fn the_named_operation_is_the_one_read() {
    let query = "query A { _entities(representations: [{ __typename: \"Order\", id: 2 }]) { __typename } } \
                 query B { _entities(representations: [{ __typename: \"User\", id: 1 }]) { __typename } }";
    let list = entities_representations(query, Some("B"), None).unwrap();
    assert_eq!(list, json!([{"__typename": "User", "id": 1}]));
}

#[test]
fn a_null_argument_is_refused() {
    let query = "query($r: [_Any!]) { _entities(representations: $r) { __typename } }";
    let err = entities_representations(query, None, Some(&json!({"r": null}))).unwrap_err();
    assert!(err.to_string().contains("is null"), "{err}");
}
