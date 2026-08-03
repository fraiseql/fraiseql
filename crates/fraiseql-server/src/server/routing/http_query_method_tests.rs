//! #508 — HTTP `QUERY` (RFC 10008) acceptance on the GraphQL endpoint.
//!
//! These drive the **real** `build_graphql_router` through `tower::ServiceExt::oneshot`,
//! not a hand-built router: the whole change lives in the method dispatch, so a test
//! that mounts its own `Router::new().route(…)` would prove nothing about what the
//! server actually serves.
//!
//! What is pinned:
//! - `QUERY` + a query, flag **on** → reaches the executor (not a 405).
//! - `QUERY` + a mutation → **405**, because a safe, retryable method must never carry a
//!   state-changing operation (an intermediary is entitled to replay it).
//! - `QUERY` + a subscription → **405**, same reason.
//! - `QUERY`, flag **off** → 405; the method is opt-in.
//! - `POST` and `GET` behave identically whether the flag is on or off.
//! - An unrelated method (`DELETE`) still 405s with the flag on — the fallback that carries QUERY
//!   must not turn the endpoint into an accept-anything route.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::{cache::CachedDatabaseAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// Build a server whose GraphQL endpoint is mounted with `enable_http_query` as given.
///
/// Boxed at the delegation point: `Server::new`'s future trips `clippy::large_futures`
/// (pedantic, denied) at every call site otherwise.
async fn server_with_query_method(enabled: bool) -> Server<CachedDatabaseAdapter<FailingAdapter>> {
    let config = ServerConfig {
        // #874: `Server::new` runs `validate()`, and the default `cors_enabled = true`
        // with no origins is refused in production mode. CORS is not what this pins.
        cors_enabled: false,
        enable_http_query: enabled,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema")
}

/// Send one request at the real GraphQL router and return its status.
async fn status_for(enabled: bool, method: &str, body: &str) -> StatusCode {
    let server = server_with_query_method(enabled).await;
    let state = server.build_app_state();
    let app = server.build_graphql_router(&state);
    let request = Request::builder()
        .method(method)
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    app.oneshot(request).await.unwrap().status()
}

/// A GET at the real router, whose query string carries the document.
async fn get_status_for(enabled: bool, query: &str) -> StatusCode {
    let server = server_with_query_method(enabled).await;
    let state = server.build_app_state();
    let app = server.build_graphql_router(&state);
    let uri = format!("/graphql?query={}", urlencoding::encode(query));
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    app.oneshot(request).await.unwrap().status()
}

const QUERY_DOC: &str = r#"{"query":"query { users { id } }"}"#;
const MUTATION_DOC: &str = r#"{"query":"mutation { createUser(name: \"x\") { id } }"}"#;
const SUBSCRIPTION_DOC: &str = r#"{"query":"subscription { userAdded { id } }"}"#;

#[tokio::test]
async fn query_method_with_a_query_is_accepted_when_enabled() {
    let status = status_for(true, "QUERY", QUERY_DOC).await;
    // The empty fixture schema has no `users` field, so the executor answers with a
    // GraphQL error — the point is that the request REACHED it rather than being
    // refused by the method router. 405 would mean QUERY never dispatched.
    assert_ne!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "QUERY + query must dispatch into the POST execution path when enabled"
    );
}

#[tokio::test]
async fn query_method_refuses_a_mutation() {
    assert_eq!(
        status_for(true, "QUERY", MUTATION_DOC).await,
        StatusCode::METHOD_NOT_ALLOWED,
        "a mutation over the safe, retryable QUERY method must be refused"
    );
}

#[tokio::test]
async fn query_method_refuses_a_subscription() {
    assert_eq!(
        status_for(true, "QUERY", SUBSCRIPTION_DOC).await,
        StatusCode::METHOD_NOT_ALLOWED,
        "a subscription over QUERY must be refused"
    );
}

#[tokio::test]
async fn query_method_is_refused_when_disabled() {
    assert_eq!(
        status_for(false, "QUERY", QUERY_DOC).await,
        StatusCode::METHOD_NOT_ALLOWED,
        "QUERY is opt-in: with the flag off the endpoint must not accept it"
    );
}

#[tokio::test]
async fn an_unrelated_method_is_still_refused_when_query_is_enabled() {
    // The fallback that carries QUERY must not become an accept-anything arm.
    assert_eq!(
        status_for(true, "DELETE", QUERY_DOC).await,
        StatusCode::METHOD_NOT_ALLOWED,
        "enabling QUERY must not make the endpoint answer other methods"
    );
}

#[tokio::test]
async fn post_and_get_are_unchanged_by_the_flag() {
    for enabled in [false, true] {
        assert_ne!(
            status_for(enabled, "POST", QUERY_DOC).await,
            StatusCode::METHOD_NOT_ALLOWED,
            "POST must keep working with enable_http_query = {enabled}"
        );
        assert_ne!(
            get_status_for(enabled, "query { users { id } }").await,
            StatusCode::METHOD_NOT_ALLOWED,
            "GET must keep working with enable_http_query = {enabled}"
        );
        // GET's own queries-only gate is untouched either way.
        assert_eq!(
            get_status_for(enabled, "mutation { createUser(name: \"x\") { id } }").await,
            StatusCode::METHOD_NOT_ALLOWED,
            "GET must still refuse mutations with enable_http_query = {enabled}"
        );
    }
}
