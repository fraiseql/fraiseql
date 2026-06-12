//! Phase 03 C3 regression tests for the admin-plane auth middlewares.
//!
//! `admin_auth_middleware` and `required_auth_middleware` are **always** mandatory:
//! they must reject anonymous/malformed requests with 401 even when the OIDC
//! validator's global `required` flag is `false` (the data plane is optional). This is
//! the H5 fix — `oidc_auth_middleware` defers to that global flag and so silently
//! un-auths admin routers whenever a deployment allows anonymous data queries.
//!
//! The valid-token paths (admin scope → 200, non-admin scope → 403) require a signed
//! token validated against an injected JWKS, whose cache is private to `fraiseql-core`;
//! that scope decision is covered by the `admin_scope` unit tests (`has_admin_scope` /
//! `require_admin_scope`). Here we lock down the always-mandatory behavior that no other
//! test exercises.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_server::middleware::{
    OidcAuthState, admin_auth_middleware, oidc_auth_middleware, required_auth_middleware,
};
use tower::ServiceExt;

/// Build an `OidcAuthState` with `required = false` (the optional / anonymous data
/// plane) — the configuration under which H5 manifested.
fn optional_oidc_state() -> OidcAuthState {
    let config = OidcConfig {
        issuer:               "https://test.fraiseql.dev".to_string(),
        audience:             Some("https://api.test.fraiseql.dev".to_string()),
        required:             false,
        additional_audiences: vec![],
        jwks_cache_ttl_secs:  3600,
        allowed_algorithms:   vec!["RS256".to_string()],
        clock_skew_secs:      60,
        jwks_uri:             None,
        scope_claim:          "scope".to_string(),
        require_jti:          false,
        me:                   None,
    };
    // with_jwks_uri bypasses async OIDC discovery; the 401 is returned before any
    // real JWKS request is made.
    let validator = OidcValidator::with_jwks_uri(config, "https://192.0.2.1/jwks".to_string());
    OidcAuthState::new(Arc::new(validator))
}

async fn ok_handler() -> StatusCode {
    StatusCode::OK
}

/// Router protected by `admin_auth_middleware` (mandatory token + `fraiseql:admin`).
fn admin_router() -> Router {
    let state = optional_oidc_state();
    Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(middleware::from_fn_with_state(state, admin_auth_middleware))
}

/// Router protected by `required_auth_middleware` (mandatory token, any scope).
fn required_router() -> Router {
    let state = optional_oidc_state();
    Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(middleware::from_fn_with_state(state, required_auth_middleware))
}

// ── H5: admin/required middlewares are mandatory even when data auth is optional ──

#[tokio::test]
async fn admin_route_without_token_is_401_even_when_data_auth_optional() {
    let response = admin_router()
        .oneshot(Request::builder().uri("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "H5: admin route must reject anonymous callers even when required=false"
    );
    assert!(
        response.headers().contains_key("www-authenticate"),
        "401 must include a WWW-Authenticate challenge"
    );
}

#[tokio::test]
async fn required_auth_route_without_token_is_401_even_when_data_auth_optional() {
    let response = required_router()
        .oneshot(Request::builder().uri("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "H5: required-auth route must reject anonymous callers even when required=false"
    );
}

#[tokio::test]
async fn admin_route_with_malformed_header_is_401() {
    let response = admin_router()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header("Authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "non-Bearer header must be 401");
}

#[tokio::test]
async fn required_auth_route_with_malformed_header_is_401() {
    let response = required_router()
        .oneshot(
            Request::builder()
                .uri("/admin")
                .header("Authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "non-Bearer header must be 401");
}

/// Contrast: the data-plane `oidc_auth_middleware` lets anonymous callers through when
/// `required=false`. This is the exact behavior the admin/required middlewares must NOT
/// inherit — it documents why H5 needed net-new middlewares rather than reuse.
#[tokio::test]
async fn oidc_data_plane_passes_anonymous_when_optional() {
    let state = optional_oidc_state();
    let router = Router::new()
        .route("/data", get(ok_handler))
        .route_layer(middleware::from_fn_with_state(state, oidc_auth_middleware));

    let response = router
        .oneshot(Request::builder().uri("/data").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "data plane stays anonymous-friendly when required=false (the H5 contrast)"
    );
}
