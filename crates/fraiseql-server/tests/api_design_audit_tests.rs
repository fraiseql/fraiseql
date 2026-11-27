//! Design audit endpoints, driven through their handlers.
//!
//! Each test posts a schema to the real `routes::api::design` handlers, mounted on a
//! router over an empty compiled schema, and asserts the verdict they return: the issues
//! each category reports, the severity and entity each issue names, each category's
//! score, and the overall score and severity counts.
//!
//! **Execution engine:** none (the handlers read only the posted schema)
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use axum::{Router, body::Body, routing::post};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::routes::{
    api::design::{
        auth_audit_handler, cache_audit_handler, compilation_audit_handler, cost_audit_handler,
        federation_audit_handler, overall_design_audit_handler,
    },
    graphql::AppState,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The six design audit routes, as the server mounts them, over an empty schema.
fn router() -> Router {
    let adapter = Arc::new(FailingAdapter::new());
    let state = AppState::new(Arc::new(Executor::new(CompiledSchema::new(), adapter)));
    Router::new()
        .route("/federation", post(federation_audit_handler))
        .route("/cost", post(cost_audit_handler))
        .route("/cache", post(cache_audit_handler))
        .route("/auth", post(auth_audit_handler))
        .route("/compilation", post(compilation_audit_handler))
        .route("/audit", post(overall_design_audit_handler))
        .with_state(state)
}

/// Post `schema` to `path` and return the `data` of a successful answer.
async fn audit(path: &str, schema: Value) -> Value {
    let request = Request::post(path)
        .header("content-type", "application/json")
        .body(Body::from(json!({ "schema": schema }).to_string()))
        .unwrap();
    let response = router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{path} answered {}", response.status());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "success", "{body}");
    body["data"].clone()
}

/// The issues of a category answer.
fn issues(category: &Value) -> &Vec<Value> {
    category["issues"].as_array().unwrap()
}

/// A schema every analyzer passes: a keyed type and an authenticated mutation.
fn clean_schema() -> Value {
    json!({
        "types": [
            {"name": "Query", "fields": [{"name": "order", "type": "Order"}]},
            {"name": "Order", "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]},
            {"name": "Mutation", "fields": [
                {"name": "createOrder", "type": "Order", "requires_auth": true}
            ]}
        ]
    })
}

/// `entity` owned by each of `count` subgraphs.
fn entity_in_subgraphs(entity: &str, count: usize) -> Value {
    let subgraphs: Vec<Value> = (0..count)
        .map(|i| json!({"name": format!("sg{i}"), "entities": [entity]}))
        .collect();
    json!({ "subgraphs": subgraphs })
}

/// `count` types, none of which marks a primary key.
fn unkeyed_types(count: usize) -> Value {
    let types: Vec<Value> = (0..count)
        .map(|i| json!({"name": format!("T{i}"), "fields": [{"name": "id", "type": "ID"}]}))
        .collect();
    json!({ "types": types })
}

/// A `Mutation` type with `count` fields that require no authentication.
fn unauthenticated_mutations(count: usize) -> Value {
    let fields: Vec<Value> = (0..count)
        .map(|i| json!({"name": format!("m{i}"), "type": "Boolean"}))
        .collect();
    json!({ "types": [{"name": "Mutation", "fields": fields}] })
}

/// A `Query` type with `count` list fields that set no default limit.
fn unbounded_lists(count: usize) -> Value {
    let fields: Vec<Value> =
        (0..count).map(|i| json!({"name": format!("l{i}"), "type": "[ID]"})).collect();
    json!({ "types": [{"name": "Query", "fields": fields}] })
}

/// A `Query` type with `count` expensive fields that carry no cache directive.
fn uncached_expensive_fields(count: usize) -> Value {
    let fields: Vec<Value> = (0..count)
        .map(|i| json!({"name": format!("e{i}"), "type": "ID", "is_expensive": true}))
        .collect();
    json!({ "types": [{"name": "Query", "fields": fields}] })
}

#[tokio::test]
async fn a_schema_every_analyzer_passes_scores_100_everywhere() {
    let data = audit("/audit", clean_schema()).await;
    assert_eq!(data["overall_score"], 100, "{data}");
    assert_eq!(data["severity_counts"], json!({"critical": 0, "warning": 0, "info": 0}));
    for category in [
        "federation",
        "cost",
        "cache",
        "authorization",
        "compilation",
    ] {
        assert_eq!(data[category]["score"], 100, "{category}: {data}");
        assert!(issues(&data[category]).is_empty(), "{category}: {data}");
    }
}

#[tokio::test]
async fn an_entity_owned_by_three_subgraphs_is_reported_once_per_owner() {
    let data = audit("/federation", entity_in_subgraphs("User", 3)).await;
    let found = issues(&data);
    assert_eq!(found.len(), 3, "{data}");
    for issue in found {
        assert_eq!(issue["severity"], "warning", "{issue}");
        assert_eq!(issue["affected"], "User", "{issue}");
        assert!(!issue["suggestion"].as_str().unwrap().is_empty(), "{issue}");
    }
    assert!(found[0]["message"].as_str().unwrap().contains("3 subgraphs"), "{data}");
    assert_eq!(data["score"], 70, "three issues at 10 points each: {data}");
}

#[tokio::test]
async fn an_entity_owned_by_two_subgraphs_is_not_reported() {
    let data = audit("/federation", entity_in_subgraphs("User", 2)).await;
    assert!(issues(&data).is_empty(), "{data}");
    assert_eq!(data["score"], 100, "{data}");
}

#[tokio::test]
async fn a_type_without_a_primary_key_is_reported_by_name() {
    let data = audit("/compilation", unkeyed_types(1)).await;
    let found = issues(&data);
    assert_eq!(found.len(), 1, "{data}");
    assert_eq!(found[0]["severity"], "warning", "{data}");
    assert_eq!(found[0]["affected"], "T0", "{data}");
    assert_eq!(data["score"], 90, "{data}");
}

#[tokio::test]
async fn a_type_marking_its_primary_key_is_not_reported() {
    let schema = json!({"types": [
        {"name": "T0", "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]}
    ]});
    let data = audit("/compilation", schema).await;
    assert!(issues(&data).is_empty(), "{data}");
}

#[tokio::test]
async fn a_mutation_requiring_no_authentication_is_reported_by_field() {
    let data = audit("/auth", unauthenticated_mutations(1)).await;
    let found = issues(&data);
    assert_eq!(found.len(), 1, "{data}");
    assert_eq!(found[0]["severity"], "warning", "{data}");
    assert_eq!(found[0]["affected"], "m0", "{data}");
    assert_eq!(data["score"], 88, "{data}");
}

#[tokio::test]
async fn a_mutation_requiring_authentication_is_not_reported() {
    let schema = json!({"types": [{"name": "Mutation", "fields": [
        {"name": "m0", "type": "Boolean", "requires_auth": true}
    ]}]});
    let data = audit("/auth", schema).await;
    assert!(issues(&data).is_empty(), "{data}");
}

#[tokio::test]
async fn a_list_without_a_default_limit_is_a_cost_warning() {
    let data = audit("/cost", unbounded_lists(1)).await;
    let found = issues(&data);
    assert_eq!(found.len(), 1, "{data}");
    assert_eq!(found[0]["severity"], "warning", "{data}");
    assert!(found[0]["message"].as_str().unwrap().contains("l0"), "{data}");
    assert_eq!(data["score"], 92, "{data}");
}

#[tokio::test]
async fn a_list_with_a_default_limit_is_not_a_cost_warning() {
    let schema = json!({"types": [{"name": "Query", "fields": [
        {"name": "l0", "type": "[ID]", "default_limit": 10}
    ]}]});
    let data = audit("/cost", schema).await;
    assert!(issues(&data).is_empty(), "{data}");
}

#[tokio::test]
async fn an_expensive_field_without_a_cache_directive_is_reported() {
    let data = audit("/cache", uncached_expensive_fields(1)).await;
    let found = issues(&data);
    assert_eq!(found.len(), 1, "{data}");
    assert_eq!(found[0]["severity"], "info", "{data}");
    assert!(found[0]["message"].as_str().unwrap().contains("Query.e0"), "{data}");
    assert_eq!(data["score"], 94, "{data}");
}

#[tokio::test]
async fn an_expensive_field_with_a_cache_directive_is_not_reported() {
    let schema = json!({"types": [{"name": "Query", "fields": [
        {"name": "e0", "type": "ID", "is_expensive": true, "has_cache_directive": true}
    ]}]});
    let data = audit("/cache", schema).await;
    assert!(issues(&data).is_empty(), "{data}");
}

#[tokio::test]
async fn the_overall_audit_agrees_with_each_category_endpoint() {
    let schema = json!({
        "subgraphs": entity_in_subgraphs("User", 3)["subgraphs"],
        "types": [
            {"name": "Order", "fields": [
                {"name": "id", "type": "ID"},
                {"name": "lines", "type": "[ID]"},
                {"name": "total", "type": "ID", "is_expensive": true}
            ]},
            {"name": "Mutation", "fields": [{"name": "cancel", "type": "Boolean"}]}
        ]
    });
    let overall = audit("/audit", schema.clone()).await;
    for (category, path) in [
        ("federation", "/federation"),
        ("cost", "/cost"),
        ("cache", "/cache"),
        ("authorization", "/auth"),
        ("compilation", "/compilation"),
    ] {
        let alone = audit(path, schema.clone()).await;
        assert_eq!(overall[category], alone, "{category}");
        assert!(!issues(&alone).is_empty(), "{category} reports nothing: {alone}");
    }
}

#[tokio::test]
async fn the_overall_score_and_counts_weigh_each_issue_by_severity() {
    // Three federation warnings (15 each), one cost warning (8), one cache info (1),
    // one authorization warning (12) and one compilation warning (5).
    let schema = json!({
        "subgraphs": entity_in_subgraphs("User", 3)["subgraphs"],
        "types": [
            {"name": "Order", "fields": [
                {"name": "id", "type": "ID"},
                {"name": "lines", "type": "[ID]"},
                {"name": "total", "type": "ID", "is_expensive": true}
            ]},
            {"name": "Mutation", "fields": [{"name": "cancel", "type": "Boolean"}]}
        ]
    });
    let data = audit("/audit", schema).await;
    assert_eq!(
        data["severity_counts"],
        json!({"critical": 0, "warning": 6, "info": 1}),
        "{data}"
    );
    assert_eq!(data["overall_score"], 100 - 45 - 8 - 1 - 12 - 5, "{data}");
}

#[tokio::test]
async fn a_category_with_more_issues_than_its_score_holds_scores_zero() {
    // 12 issues at 10 points, 13 at 8, 17 at 6, 9 at 12: each deducts more than 100.
    let cases = [
        ("/federation", "federation", entity_in_subgraphs("User", 12)),
        ("/cost", "cost", unbounded_lists(13)),
        ("/cache", "cache", uncached_expensive_fields(17)),
        ("/auth", "authorization", unauthenticated_mutations(9)),
        ("/compilation", "compilation", unkeyed_types(11)),
    ];
    for (path, category, schema) in cases {
        let alone = audit(path, schema.clone()).await;
        assert_eq!(alone["score"], 0, "{path}: {} issues", issues(&alone).len());
        let overall = audit("/audit", schema).await;
        assert_eq!(overall[category]["score"], 0, "/audit {category}");
    }
}
