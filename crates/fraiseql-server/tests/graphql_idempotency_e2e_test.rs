//! End-to-end tests for `Idempotency-Key` deduplication on the GraphQL
//! mutation path (#747).
//!
//! The saga coordinator dispatches step mutations at-least-once: an ambiguous
//! failure (timeout, connection reset after send) or a crash-recovery replay
//! re-sends the same mutation under the same `Idempotency-Key`. These tests
//! prove the receiving side honours that contract:
//!
//! 1. a repeated mutation under one key executes **once** — the second request replays the stored
//!    response without touching the database;
//! 2. reusing a key with a different body is a 409 conflict, never a silent replay of the wrong
//!    response;
//! 3. mutations without a key keep at-will semantics (each request executes);
//! 4. queries ignore the header entirely (only mutations are deduplicated).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body, routing::post};
use fraiseql_core::{
    runtime::Executor,
    schema::{ArgumentDefinition, FieldType, MutationDefinition},
};
use fraiseql_server::routes::graphql::{AppState, graphql_handler};
use fraiseql_test_utils::{failing_adapter::FailingAdapter, schema_builder::TestSchemaBuilder};
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Build a successful `mutation_response` row as returned by `FailingAdapter`.
fn mutation_success_row(entity: Value) -> Vec<HashMap<String, Value>> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!(""));
    row.insert("entity".to_string(), entity);
    row.insert("entity_type".to_string(), json!("User"));
    row.insert("entity_id".to_string(), json!("11111111-1111-1111-1111-111111111111"));
    vec![row]
}

fn required_arg(name: &str, ty: FieldType) -> ArgumentDefinition {
    ArgumentDefinition {
        name:          name.to_string(),
        arg_type:      ty,
        nullable:      false,
        default_value: None,
        description:   None,
        deprecation:   None,
    }
}

/// Build the router plus a handle on the adapter so tests can count executions.
fn make_router() -> (Router, Arc<FailingAdapter>) {
    let mut mutation = MutationDefinition::new("updateUser", "User");
    mutation.sql_source = Some("fn_updateUser".to_string());
    mutation.arguments = vec![required_arg("id", FieldType::Id)];

    let schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_mutation(mutation)
        .build();

    let entity = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "bio": "idempotent bio"
    });
    let adapter = Arc::new(
        FailingAdapter::new().with_function_response("fn_updateUser", mutation_success_row(entity)),
    );

    let executor = Arc::new(Executor::new(schema, Arc::clone(&adapter)));
    let state = AppState::new(executor);

    let router = Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state);
    (router, adapter)
}

const MUTATION: &str = "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id bio } }";

/// POST a GraphQL body, optionally with an `Idempotency-Key` header.
async fn post_graphql(router: Router, body: Value, key: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    let response = router
        .oneshot(builder.body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

fn mutation_body(id: &str) -> Value {
    json!({ "query": MUTATION, "variables": { "id": id } })
}

/// The core #747 receiver guarantee: a mutation re-sent under the same
/// `Idempotency-Key` — as the saga coordinator does after a timeout or a
/// crash-recovery replay — executes exactly once. The second response is the
/// stored replay, byte-identical, with no second database dispatch.
#[tokio::test]
async fn repeated_mutation_with_same_key_executes_once() {
    let (router, adapter) = make_router();

    let (status1, body1) =
        post_graphql(router.clone(), mutation_body("u-1"), Some("step-key-1")).await;
    assert_eq!(status1, StatusCode::OK, "first attempt executes: {body1}");
    assert!(body1.get("data").is_some(), "first attempt returns data: {body1}");
    let executions_after_first = adapter.query_count();

    let (status2, body2) = post_graphql(router, mutation_body("u-1"), Some("step-key-1")).await;
    assert_eq!(status2, StatusCode::OK, "the retry succeeds: {body2}");
    assert_eq!(body2, body1, "the replayed response is the stored one");
    assert_eq!(
        adapter.query_count(),
        executions_after_first,
        "the repeated mutation must NOT reach the database — one logical effect"
    );
}

/// Reusing a key with a different body is a client error (409), never a replay
/// of the other request's response and never a silent second execution.
#[tokio::test]
async fn same_key_with_different_body_is_a_conflict() {
    let (router, adapter) = make_router();

    let (status1, _) = post_graphql(router.clone(), mutation_body("u-1"), Some("key-x")).await;
    assert_eq!(status1, StatusCode::OK);
    let executions_after_first = adapter.query_count();

    let (status2, body2) = post_graphql(router, mutation_body("u-2"), Some("key-x")).await;
    assert_eq!(
        status2,
        StatusCode::CONFLICT,
        "a reused key with a different body must be rejected: {body2}"
    );
    assert_eq!(
        adapter.query_count(),
        executions_after_first,
        "the conflicting mutation must not execute"
    );
}

/// Without a key, mutations keep their at-will semantics: every request
/// executes. Deduplication is strictly opt-in via the header.
#[tokio::test]
async fn mutation_without_key_executes_every_time() {
    let (router, adapter) = make_router();

    let (s1, _) = post_graphql(router.clone(), mutation_body("u-1"), None).await;
    let count_after_first = adapter.query_count();
    let (s2, _) = post_graphql(router, mutation_body("u-1"), None).await;

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert!(
        adapter.query_count() > count_after_first,
        "each keyless mutation must execute independently"
    );
}

/// Queries are reads — the header must be ignored for them: both requests
/// execute against the database, neither is replayed from the store.
#[tokio::test]
async fn queries_ignore_the_idempotency_key_header() {
    let (router, adapter) = make_router();
    let query = json!({ "query": "{ users { id } }" });

    let (s1, _) = post_graphql(router.clone(), query.clone(), Some("key-q")).await;
    let count_after_first = adapter.query_count();
    assert!(count_after_first > 0, "the query reaches the adapter");
    let (s2, _) = post_graphql(router, query, Some("key-q")).await;

    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert!(
        adapter.query_count() > count_after_first,
        "a repeated QUERY under an Idempotency-Key must re-execute, never replay"
    );
}
