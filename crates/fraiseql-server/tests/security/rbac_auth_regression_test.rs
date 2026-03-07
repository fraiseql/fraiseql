//! Regression tests for Campaign 1 RBAC auth bypass bug.
//!
//! SR-8: E2a — RBAC management router was merged without authentication middleware.
//!       Any client could read or modify role assignments without a token.
//!       Fix: the router is now wrapped in `bearer_auth_middleware`, which requires
//!       a valid admin bearer token and returns 401 (missing) or 403 (wrong token).
//!
//! Because the RBAC handlers require a live PostgreSQL connection, these tests
//! exercise the authentication layer in isolation: a protected router built with
//! the same `bearer_auth_middleware` pattern that now guards the RBAC endpoints.
//! The auth middleware runs before any handler logic, so the test correctly
//! verifies the security invariant even without a real database.
//!
//! **Execution engine:** none
//! **Infrastructure:** none

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! **Parallelism:** safe

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use fraiseql_server::middleware::{BearerAuthState, bearer_auth_middleware};
use tower::ServiceExt;

/// Admin token used throughout these tests. Must be ≥ 32 characters.
const ADMIN_TOKEN: &str = "test-admin-token-that-is-32-chars-long-for-security";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a router that mimics the RBAC management router with bearer auth applied.
///
/// Uses the same `route_layer` + `bearer_auth_middleware` pattern as the
/// production routing code to verify the auth enforcement pattern.
fn rbac_router_with_auth() -> Router {
    let auth_state = BearerAuthState::new(ADMIN_TOKEN.to_string());

    // A lightweight stand-in for the RBAC handlers — the auth middleware
    // runs before the handler, so the handler type does not matter for
    // testing the 401 / 403 behavior.
    async fn roles_handler() -> StatusCode { StatusCode::OK }
    async fn permissions_handler() -> StatusCode { StatusCode::OK }
    async fn user_roles_handler() -> StatusCode { StatusCode::OK }

    Router::new()
        .route("/api/roles", get(roles_handler))
        .route("/api/permissions", get(permissions_handler))
        .route("/api/user-roles", get(user_roles_handler))
        .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
}

// ---------------------------------------------------------------------------
// SR-8 regression tests
// ---------------------------------------------------------------------------

/// RBAC endpoints must return 401 when no Authorization header is provided.
///
/// Before E2a fix, these endpoints were mounted without any auth middleware
/// and returned 200 to any unauthenticated client.
#[tokio::test]
async fn rbac_endpoints_return_401_without_auth_header() {
    let router = rbac_router_with_auth();

    for path in &["/api/roles", "/api/permissions", "/api/user-roles"] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(*path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "E2a regression: RBAC endpoint {path} returned {} without auth (expected 401)",
            response.status()
        );

        assert!(
            response.headers().contains_key("www-authenticate"),
            "E2a regression: 401 response for {path} must include WWW-Authenticate header"
        );
    }
}

/// RBAC endpoints must return 403 when a Bearer token is present but incorrect.
///
/// The constant-time comparison prevents timing attacks on the admin token.
#[tokio::test]
async fn rbac_endpoints_return_403_with_wrong_token() {
    let router = rbac_router_with_auth();

    for path in &["/api/roles", "/api/permissions", "/api/user-roles"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(*path)
                    .header("Authorization", "Bearer wrong-token-that-does-not-match")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "E2a regression: RBAC endpoint {path} accepted wrong token (expected 403)"
        );
    }
}

/// Non-Bearer authorization schemes (e.g., Basic) must return 401.
///
/// The middleware must require exactly the `Bearer` scheme.
#[tokio::test]
async fn rbac_endpoints_return_401_for_basic_auth() {
    let router = rbac_router_with_auth();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/roles")
                .header("Authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "E2a regression: RBAC endpoint accepted Basic auth instead of requiring Bearer"
    );
}

/// A valid admin token must be allowed through to the handler.
///
/// This confirms the auth fix does not block legitimate requests.
#[tokio::test]
async fn rbac_endpoints_return_200_with_valid_admin_token() {
    let router = rbac_router_with_auth();

    for path in &["/api/roles", "/api/permissions", "/api/user-roles"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(*path)
                    .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Valid admin token must allow access to {path}; got {}",
            response.status()
        );
    }
}
