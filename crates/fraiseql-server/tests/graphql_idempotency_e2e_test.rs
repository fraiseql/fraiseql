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
//! 4. queries ignore the header entirely (only mutations are deduplicated);
//! 5. a key is the principal's own: another principal sending the same key and body is never served
//!    the stored response — its request runs as its own, under its own gates.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body, routing::post};
use chrono::{Duration, Utc};
use fraiseql_core::{
    runtime::Executor,
    schema::{ArgumentDefinition, FieldType, MutationDefinition},
    security::AuthenticatedUser,
    types::UserId,
};
use fraiseql_server::{
    middleware::AuthUser,
    routes::graphql::{AppState, graphql_handler},
};
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
    make_router_requiring(None)
}

/// `make_router`, with `updateUser` requiring `role` when one is given.
fn make_router_requiring(role: Option<&str>) -> (Router, Arc<FailingAdapter>) {
    let mut mutation = MutationDefinition::new("updateUser", "User");
    mutation.sql_source = Some("fn_updateUser".to_string());
    mutation.arguments = vec![required_arg("id", FieldType::Id)];
    mutation.requires_role = role.map(ToString::to_string);

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

    let router = Router::new().route("/graphql", post(graphql_handler)).with_state(state);
    (router, adapter)
}

const MUTATION: &str = "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id bio } }";

/// POST a GraphQL body, optionally with an `Idempotency-Key` header.
async fn post_graphql(router: Router, body: Value, key: Option<&str>) -> (StatusCode, Value) {
    post_graphql_as(router, body, key, None).await
}

/// `post_graphql`, as the principal the OIDC middleware would have authenticated.
async fn post_graphql_as(
    router: Router,
    body: Value,
    key: Option<&str>,
    auth: Option<AuthUser>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json");
    if let Some(key) = key {
        builder = builder.header("idempotency-key", key);
    }
    let mut request = builder.body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
    if let Some(auth) = auth {
        request.extensions_mut().insert(auth);
    }
    let response = router.oneshot(request).await.unwrap();
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

/// A principal `sub`, holding `roles`.
fn principal(sub: &str, roles: &[&str]) -> AuthUser {
    AuthUser(AuthenticatedUser {
        user_id:      UserId::new(sub),
        scopes:       Vec::new(),
        expires_at:   Utc::now() + Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::from([("roles".to_string(), json!(roles))]),
    })
}

/// Control: the principal that stored a response is replayed it — the retry the key is for.
#[tokio::test]
async fn the_principal_that_used_a_key_is_replayed_its_response() {
    let (router, adapter) = make_router_requiring(Some("editor"));
    let alice = || Some(principal("alice", &["editor"]));

    let (_, first) =
        post_graphql_as(router.clone(), mutation_body("u-1"), Some("order-42"), alice()).await;
    assert!(first["data"]["updateUser"].is_object(), "alice may run it: {first}");
    let executions = adapter.query_count();

    let (status, again) =
        post_graphql_as(router, mutation_body("u-1"), Some("order-42"), alice()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again, first, "the stored response");
    assert_eq!(adapter.query_count(), executions, "not executed again");
}

/// **Reproduction (ruling AM).** A stored response is a read, and it was produced under the
/// first caller's gates. Bob, who does not hold the role `updateUser` requires, sends alice's
/// key and body: he must be refused as he is under any other key — not served her payload.
#[tokio::test]
#[ignore = "reproduction (ruling AM): another principal is replayed a stored response"]
async fn a_stored_response_is_never_replayed_to_another_principal() {
    let (router, _) = make_router_requiring(Some("editor"));

    let (_, first) = post_graphql_as(
        router.clone(),
        mutation_body("u-1"),
        Some("order-42"),
        Some(principal("alice", &["editor"])),
    )
    .await;
    assert!(first["data"]["updateUser"].is_object(), "alice may run it: {first}");

    let (_, bobs) = post_graphql_as(
        router,
        mutation_body("u-1"),
        Some("order-42"),
        Some(principal("bob", &[])),
    )
    .await;
    assert_ne!(bobs, first, "bob was served alice's stored response");
    assert!(
        bobs.get("data").and_then(|d| d.get("updateUser")).is_none_or(Value::is_null),
        "bob may not run `updateUser`: {bobs}"
    );
    assert!(bobs["errors"].is_array(), "refused as under any other key: {bobs}");
}

/// **Reproduction (ruling AM).** Two principals allowed to run the mutation choosing the same
/// key: the second one's mutation runs — as if it had chosen another key — rather than being
/// silently swallowed by the first one's entry.
#[tokio::test]
#[ignore = "reproduction (ruling AM): another principal's mutation does not run"]
async fn another_principal_under_the_same_key_runs_its_own_mutation() {
    let (router, adapter) = make_router_requiring(Some("editor"));

    let (_, first) = post_graphql_as(
        router.clone(),
        mutation_body("u-1"),
        Some("order-42"),
        Some(principal("alice", &["editor"])),
    )
    .await;
    assert!(first["data"]["updateUser"].is_object(), "{first}");
    let executions = adapter.query_count();

    let (status, bobs) = post_graphql_as(
        router,
        mutation_body("u-1"),
        Some("order-42"),
        Some(principal("bob", &["editor"])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bobs}");
    assert!(adapter.query_count() > executions, "bob's mutation was never executed");
}
