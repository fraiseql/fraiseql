//! End-to-end proof for #450: a root `{ __typename }` query resolves to the
//! operation's root type name through the full HTTP → parse → classify →
//! execute → serialize pipeline.
//!
//! The router is built over an EMPTY schema (no queries) backed by a
//! `FailingAdapter` that errors on any database call. If `{ __typename }` still
//! resolves to `"Query"`, it proves the meta-field is handled without a schema
//! lookup or a DB round-trip — before the fix it was rejected with
//! "Query '__typename' not found in schema". No auth layer is mounted, so this
//! also confirms the probe works unauthenticated (it is not gated behind
//! introspection-auth).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use axum::{Router, body::Body, routing::post};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::routes::graphql::{AppState, graphql_handler};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

/// `/graphql` over an empty schema + a `FailingAdapter` (errors on any DB call).
fn typename_router() -> Router {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    let state = AppState::new(Arc::new(Executor::new(schema, adapter)));
    Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state)
}

async fn post_graphql(router: Router, body: Value) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn root_typename_probe_resolves_to_query() {
    let (status, body) =
        post_graphql(typename_router(), json!({ "query": "{ __typename }" })).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["__typename"], "Query", "body: {body}");
    // A validation error would have produced `data: null` + an errors array.
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
}

#[tokio::test]
async fn root_typename_probe_supports_alias() {
    let (status, body) =
        post_graphql(typename_router(), json!({ "query": "{ ping: __typename }" })).await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["ping"], "Query", "body: {body}");
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
}
