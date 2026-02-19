//! Shared test helpers for behavioral integration tests.
//!
//! Provides router builders and HTTP request helpers that exercise real
//! production handlers through axum's `tower::ServiceExt::oneshot`.

use std::sync::Arc;

use axum::{
    Router, body::Body,
    routing::{get, post},
};
use fraiseql_core::{runtime::Executor, schema::CompiledSchema};
use fraiseql_server::routes::{
    api::query::{explain_handler, stats_handler, validate_handler},
    api::schema::{export_json_handler, export_sdl_handler},
    graphql::AppState,
    health::health_handler,
    introspection::introspection_handler,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

/// Create a default `AppState` with a healthy `FailingAdapter` and empty schema.
pub fn make_test_state() -> AppState<FailingAdapter> {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    AppState::new(Arc::new(Executor::new(schema, adapter)))
}

/// Create an `AppState` with a custom adapter and schema.
pub fn make_test_state_with(
    adapter: FailingAdapter,
    schema: CompiledSchema,
) -> AppState<FailingAdapter> {
    AppState::new(Arc::new(Executor::new(schema, Arc::new(adapter))))
}

/// Build a router with health and introspection endpoints.
pub fn health_router(state: AppState<FailingAdapter>) -> Router {
    Router::new()
        .route("/health", get(health_handler::<FailingAdapter>))
        .route(
            "/introspection",
            get(introspection_handler::<FailingAdapter>),
        )
        .with_state(state)
}

/// Build a router with API query and schema endpoints.
pub fn api_router(state: AppState<FailingAdapter>) -> Router {
    Router::new()
        .route(
            "/api/v1/query/explain",
            post(explain_handler::<FailingAdapter>),
        )
        .route(
            "/api/v1/query/validate",
            post(validate_handler::<FailingAdapter>),
        )
        .route(
            "/api/v1/query/stats",
            get(stats_handler::<FailingAdapter>),
        )
        .route(
            "/api/v1/schema.graphql",
            get(export_sdl_handler::<FailingAdapter>),
        )
        .route(
            "/api/v1/schema.json",
            get(export_json_handler::<FailingAdapter>),
        )
        .with_state(state)
}

/// Send a GET request and parse the JSON response.
pub async fn get_json(router: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

/// Send a GET request and return the raw body as a string.
pub async fn get_text(router: &Router, uri: &str) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

/// Send a POST request with JSON body and parse the JSON response.
pub async fn post_json(
    router: &Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}
