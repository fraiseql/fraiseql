//! Pure unit tests for the `@requires` pre-fetch helpers.
//!
//! These cover the I/O-free logic — building the representation, extracting the
//! field, merging the value, and shaping the pre-dispatch failure — so the
//! "never dispatch with missing inputs" contract is exercised on every push,
//! independent of a live database, a mock subgraph, or the `saga` feature.
#![allow(clippy::unwrap_used, clippy::float_cmp)] // Reason: test code

use serde_json::json;

use super::{build_representation, extract_field, merge_variable, prefetch_failure};
use crate::saga_store::StepState;

#[test]
fn extract_field_returns_top_level_scalar() {
    let entity = json!({"price": 9.99, "name": "Widget"});
    assert_eq!(extract_field(&entity, "price"), Some(json!(9.99)));
}

#[test]
fn extract_field_traverses_dotted_path() {
    let entity = json!({"dimensions": {"weight": 5}});
    assert_eq!(extract_field(&entity, "dimensions.weight"), Some(json!(5)));
}

#[test]
fn extract_field_missing_is_none() {
    let entity = json!({"name": "Widget"});
    assert_eq!(extract_field(&entity, "price"), None, "an absent field is unresolved");
}

#[test]
fn extract_field_null_is_none() {
    // A resolved-but-null field counts as unresolved: the step must fail before
    // dispatch rather than run with a null input.
    let entity = json!({"price": null});
    assert_eq!(extract_field(&entity, "price"), None);
}

#[test]
fn merge_variable_inserts_into_object() {
    let mut variables = json!({"orderId": "o1"});
    merge_variable(&mut variables, "price", json!(9.99)).unwrap();
    assert_eq!(variables, json!({"orderId": "o1", "price": 9.99}));
}

#[test]
fn merge_variable_overwrites_existing_key() {
    let mut variables = json!({"price": "stale"});
    merge_variable(&mut variables, "price", json!(9.99)).unwrap();
    assert_eq!(variables["price"], json!(9.99), "the fetched value wins");
}

#[test]
fn merge_variable_non_object_errors() {
    let mut variables = json!([1, 2, 3]);
    let err = merge_variable(&mut variables, "price", json!(1)).unwrap_err();
    assert!(err.contains("non-object"), "error explains the non-object variables: {err}");
}

#[test]
fn build_representation_from_object_key() {
    let rep = build_representation("Product", &json!({"id": "p1"})).unwrap();
    assert_eq!(rep.typename, "Product");
    assert_eq!(rep.all_fields.get("id"), Some(&json!("p1")));
    assert_eq!(rep.key_fields.get("id"), Some(&json!("p1")));
}

#[test]
fn build_representation_non_object_key_errors() {
    let err = build_representation("Product", &json!("p1")).unwrap_err();
    assert!(err.contains("must be a JSON object"), "error explains the bad key: {err}");
}

#[test]
fn prefetch_failure_is_failed_step_with_message() {
    let (result, state) = prefetch_failure(2, "@requires field 'price' missing");
    assert_eq!(state, StepState::Failed);
    assert!(!result.success, "a pre-fetch failure is a real Failed step");
    assert_eq!(result.step_number, 2);
    assert_eq!(result.error.as_deref(), Some("@requires field 'price' missing"));
    assert!(result.data.is_none(), "a failed step fabricates no result data");
    assert_eq!(result.duration_ms, 0, "the mutation never ran");
}

/// #765 — a dotted `@requires` path must produce a **valid** `_entities`
/// selection: `dimensions.weight` selects the object field with a subselection
/// (`dimensions { weight }`), never the bare composite field (which every
/// spec-compliant GraphQL server rejects with "must have a selection of
/// subfields" — so documented dotted-path support could never work against an
/// Apollo-class subgraph).
#[tokio::test]
async fn dotted_requires_path_requests_nested_subselection() {
    use std::collections::HashMap;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::{
        http_resolver::{HttpClientConfig, HttpEntityResolver},
        saga_store::RequiredField,
    };

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"_entities": [
                {"__typename": "Product", "dimensions": {"weight": 2.5}}
            ]}
        })))
        .mount(&server)
        .await;

    let resolver = HttpEntityResolver::new_for_test(HttpClientConfig::default()).unwrap();
    let mut urls = HashMap::new();
    urls.insert(
        "catalog".to_string(),
        reqwest::Url::parse(&format!("{}/graphql", server.uri())).unwrap(),
    );
    let required = vec![RequiredField {
        subgraph:   "catalog".to_string(),
        typename:   "Product".to_string(),
        key:        json!({"id": "p1"}),
        field_path: "dimensions.weight".to_string(),
        target_var: "weight".to_string(),
    }];

    let merged = super::resolve_required_fields(&required, &json!({}), Some(&resolver), &urls)
        .await
        .unwrap();
    assert_eq!(merged["weight"], json!(2.5), "the nested value is extracted and merged");

    // The wire-level selection must be spec-valid GraphQL.
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let query = body["query"].as_str().unwrap();
    assert!(
        query.contains("dimensions { weight }"),
        "an object field needs a subselection — a bare 'dimensions' leaf is invalid \
         GraphQL against any spec-compliant subgraph; got: {query}"
    );
}
