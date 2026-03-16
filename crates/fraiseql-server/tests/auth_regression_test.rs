//! Regression tests for Campaign 1 auth bypass bugs.
//!
//! SR-1: E1 — GET /graphql passed `security_context`: None, bypassing RLS
//!       and field-level auth for all unauthenticated GET queries.
//!       Fix: OIDC middleware checks `required` flag and returns 401 when
//!       authentication is mandatory and no Authorization header is present.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables use _ prefix
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures
#![allow(clippy::match_same_arms)] // Reason: test data clarity
#![allow(clippy::branches_sharing_code)] // Reason: test assertion clarity
#![allow(clippy::undocumented_unsafe_blocks)] // Reason: test exercises unsafe paths

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_server::middleware::{OidcAuthState, oidc_auth_middleware};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an `OidcAuthState` where authentication is *required* (`required=true`).
///
/// Uses `OidcValidator::with_jwks_uri` to bypass the async OIDC discovery —
/// safe for tests because the 401 is returned before the JWKS endpoint is hit.
fn required_oidc_state() -> OidcAuthState {
    let config = OidcConfig {
        issuer:               "https://test.fraiseql.dev".to_string(),
        audience:             Some("https://api.test.fraiseql.dev".to_string()),
        required:             true, // THE CRITICAL FLAG — the E1 fix enforces this
        additional_audiences: vec![],
        jwks_cache_ttl_secs:  3600,
        allowed_algorithms:   vec!["RS256".to_string()],
        clock_skew_secs:      60,
        jwks_uri:             None,
        scope_claim:          "scope".to_string(),
        require_jti:          false,
    };
    // with_jwks_uri bypasses async OIDC discovery; 401 is returned before
    // any real JWKS request is made.
    let validator = OidcValidator::with_jwks_uri(config, "https://192.0.2.1/jwks".to_string());
    OidcAuthState::new(Arc::new(validator))
}

/// Build an `OidcAuthState` where authentication is *optional* (`required=false`).
fn optional_oidc_state() -> OidcAuthState {
    let config = OidcConfig {
        issuer:               "https://test.fraiseql.dev".to_string(),
        audience:             Some("https://api.test.fraiseql.dev".to_string()),
        required:             false, // optional auth
        additional_audiences: vec![],
        jwks_cache_ttl_secs:  3600,
        allowed_algorithms:   vec!["RS256".to_string()],
        clock_skew_secs:      60,
        jwks_uri:             None,
        scope_claim:          "scope".to_string(),
        require_jti:          false,
    };
    let validator = OidcValidator::with_jwks_uri(config, "https://192.0.2.1/jwks".to_string());
    OidcAuthState::new(Arc::new(validator))
}

/// Minimal handler representing the GET /graphql endpoint.
async fn dummy_graphql_handler() -> StatusCode {
    StatusCode::OK
}

/// Build a test router that wraps the dummy graphql handler with OIDC middleware.
fn graphql_router_with_required_auth() -> Router {
    let oidc_state = required_oidc_state();
    Router::new()
        .route("/graphql", get(dummy_graphql_handler))
        .route_layer(middleware::from_fn_with_state(oidc_state, oidc_auth_middleware))
}

fn graphql_router_with_optional_auth() -> Router {
    let oidc_state = optional_oidc_state();
    Router::new()
        .route("/graphql", get(dummy_graphql_handler))
        .route_layer(middleware::from_fn_with_state(oidc_state, oidc_auth_middleware))
}

// ---------------------------------------------------------------------------
// SR-1 regression tests
// ---------------------------------------------------------------------------

/// GET /graphql without Authorization header must return 401 when OIDC auth is
/// configured as required. Before the E1 fix, the handler passed
/// `security_context: None` and served data without enforcement.
#[tokio::test]
async fn get_graphql_without_auth_returns_401_when_auth_required() {
    let router = graphql_router_with_required_auth();

    let response = router
        .oneshot(Request::builder().uri("/graphql").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "E1 regression: GET /graphql must return 401 when auth is required and no token is provided"
    );

    // WWW-Authenticate header must be present (RFC 7235 §4.1)
    assert!(
        response.headers().contains_key("www-authenticate"),
        "E1 regression: 401 response must include WWW-Authenticate header"
    );
}

/// GET /graphql with a malformed Authorization header must return 401.
/// "Bearer" prefix is required; bare tokens or Basic auth must be rejected.
#[tokio::test]
async fn get_graphql_with_malformed_auth_header_returns_401() {
    let router = graphql_router_with_required_auth();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .header("Authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "E1 regression: non-Bearer Authorization header must return 401"
    );
}

/// When auth is configured as optional (development / anonymous-query mode),
/// GET /graphql without Authorization must be allowed through.
/// This verifies the optional path was not broken by the E1 fix.
#[tokio::test]
async fn get_graphql_without_auth_passes_when_auth_is_optional() {
    let router = graphql_router_with_optional_auth();

    let response = router
        .oneshot(Request::builder().uri("/graphql").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Without auth the dummy handler returns 200 — auth is truly optional.
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "When auth is optional, unauthenticated GET /graphql must be allowed"
    );
}
