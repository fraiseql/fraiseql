//! End-to-end proof for #453: the GraphQL request path enforces the configured
//! [`IntrospectionPolicy`]. `{ __schema }` / `{ __type }` — single-root, aliased,
//! or multi-root — are rejected when the policy forbids; `__typename` and normal
//! queries always pass. A blocked introspection query is a GraphQL error in the
//! `errors[]` array with **HTTP 200** (GraphQL-over-HTTP convention), never a
//! 5xx, and must never leak schema data.
//!
//! The router is built over an empty schema + a `FailingAdapter` (errors on any
//! DB call). Introspection is answered from the compiled schema with no DB
//! round-trip, so a *permitted* introspection query returns data; a normal query
//! is resolved against the (empty) schema and errors — which must NOT be an
//! introspection rejection. An authenticated request is simulated by injecting an
//! `AuthUser` into the request extensions, exactly as the OIDC middleware would.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body, routing::post};
use chrono::{Duration, Utc};
use fraiseql_core::{
    runtime::Executor,
    schema::CompiledSchema,
    security::{AuthenticatedUser, IntrospectionPolicy},
    types::UserId,
};
use fraiseql_server::{
    middleware::AuthUser,
    routes::graphql::{AppState, graphql_handler},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The `extensions.code` (serialized top-level as `code`) of a policy rejection.
const INTROSPECTION_DISABLED: &str = "INTROSPECTION_DISABLED";

/// `/graphql` over an empty schema + a `FailingAdapter`, with the given policy.
fn router_with_policy(policy: IntrospectionPolicy) -> Router {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    let state =
        AppState::new(Arc::new(Executor::new(schema, adapter))).with_introspection_policy(policy);
    Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state)
}

/// `/graphql` over an empty schema with the *default* `AppState` — no explicit
/// `with_introspection_policy`. Used to prove the default is fail-closed.
fn default_router() -> Router {
    let schema = CompiledSchema::new();
    let adapter = Arc::new(FailingAdapter::new());
    let state = AppState::new(Arc::new(Executor::new(schema, adapter)));
    Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state)
}

/// An authenticated principal, as the OIDC middleware would deposit in the
/// request extensions for `OptionalSecurityContext` to read.
fn authenticated_user() -> AuthUser {
    AuthUser(AuthenticatedUser {
        user_id:      UserId::new("test-user"),
        scopes:       Vec::new(),
        expires_at:   Utc::now() + Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::new(),
    })
}

async fn post_graphql(router: Router, body: &Value, auth: Option<AuthUser>) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap();
    if let Some(auth) = auth {
        req.extensions_mut().insert(auth);
    }
    let response = router.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

/// True iff the response carries an introspection-policy rejection error.
fn is_introspection_rejection(body: &Value) -> bool {
    body.get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errs| errs.iter().any(|e| e["code"] == INTROSPECTION_DISABLED))
}

// ============================================================================
// Disabled — the fail-closed default. Introspection is blocked, `__typename`
// and normal queries pass.
// ============================================================================

#[tokio::test]
async fn default_policy_blocks_introspection() {
    // No `with_introspection_policy` call — the AppState default must fail closed.
    let (status, body) = post_graphql(
        default_router(),
        &json!({ "query": "{ __schema { queryType { name } } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        is_introspection_rejection(&body),
        "default policy must block introspection: {body}"
    );
}

#[tokio::test]
async fn disabled_blocks_schema_introspection() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ __schema { queryType { name } } }" }),
        None,
    )
    .await;

    // GraphQL-over-HTTP: a well-formed but disallowed query is 200 + errors[].
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(is_introspection_rejection(&body), "expected introspection rejection: {body}");
    assert!(body["data"]["__schema"].is_null(), "must not leak schema: {body}");
}

#[tokio::test]
async fn disabled_blocks_type_introspection() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ __type(name: \"Query\") { name } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(is_introspection_rejection(&body), "body: {body}");
}

#[tokio::test]
async fn disabled_blocks_aliased_schema_introspection() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ foo: __schema { queryType { name } } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        is_introspection_rejection(&body),
        "aliased introspection must be blocked: {body}"
    );
    assert!(body["data"]["foo"].is_null(), "must not leak schema: {body}");
}

#[tokio::test]
async fn disabled_blocks_multi_root_introspection() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ users { id } __schema { types { name } } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        is_introspection_rejection(&body),
        "multi-root introspection must be blocked: {body}"
    );
    assert!(body["data"]["__schema"].is_null(), "must not leak schema: {body}");
}

#[tokio::test]
async fn disabled_allows_root_typename() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ __typename }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(!is_introspection_rejection(&body), "__typename must never be blocked: {body}");
    assert_eq!(body["data"]["__typename"], "Query", "body: {body}");
}

#[tokio::test]
async fn disabled_does_not_reject_normal_query_as_introspection() {
    // A normal query is resolved against the (empty) schema and errors there;
    // the point is only that it is NOT rejected as introspection.
    let (_status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Disabled),
        &json!({ "query": "{ users { id } }" }),
        None,
    )
    .await;

    assert!(
        !is_introspection_rejection(&body),
        "normal query must not be introspection-rejected: {body}"
    );
}

// ============================================================================
// Allowed — introspection permitted for everyone.
// ============================================================================

#[tokio::test]
async fn allowed_permits_schema_introspection() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::Allowed),
        &json!({ "query": "{ __schema { queryType { name } } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(!is_introspection_rejection(&body), "Allowed must permit introspection: {body}");
    // Introspection is answered from the schema (no DB call), so data is present.
    assert!(body["data"]["__schema"].is_object(), "expected schema data: {body}");
}

// ============================================================================
// InternalOnly — introspection permitted only for authenticated requests.
// ============================================================================

#[tokio::test]
async fn internal_only_blocks_anonymous() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::InternalOnly),
        &json!({ "query": "{ __schema { queryType { name } } }" }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        is_introspection_rejection(&body),
        "InternalOnly must block anonymous introspection: {body}"
    );
}

#[tokio::test]
async fn internal_only_allows_authenticated() {
    let (status, body) = post_graphql(
        router_with_policy(IntrospectionPolicy::InternalOnly),
        &json!({ "query": "{ __schema { queryType { name } } }" }),
        Some(authenticated_user()),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        !is_introspection_rejection(&body),
        "InternalOnly must allow authenticated introspection: {body}"
    );
    assert!(
        body["data"]["__schema"].is_object(),
        "expected schema data for authed user: {body}"
    );
}
