//! The admin plane's scope rule, pinned on [`admin_auth_middleware`] itself.
//!
//! Each case mints a real token with the in-process issuer and sends it through the
//! middleware, so the rule is checked where it runs: the `scope` claim is split by the
//! token validator and matched whole against [`ADMIN_SCOPE`].
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body, middleware::from_fn_with_state, routing::get};
use http::{Request, StatusCode};
use tower::ServiceExt;

use super::{
    admin_scope::ADMIN_SCOPE,
    oidc_auth::{OidcAuthState, TenantClaim, admin_auth_middleware},
};
use crate::testing::idp::TestIdp;

/// A route behind the admin plane, answering 200 to whoever the middleware lets through.
fn admin_plane(idp: &TestIdp) -> Router {
    let state = OidcAuthState::new(idp.validator(true), TenantClaim(Arc::from("tenant_id")));
    Router::new()
        .route("/admin", get(|| async { "admitted" }))
        .layer(from_fn_with_state(state, admin_auth_middleware))
}

/// The status and body the admin plane answers a token carrying `scope`, or no `scope`
/// claim at all when `None`.
async fn answer(scope: Option<&str>) -> (StatusCode, String) {
    let idp = TestIdp::start().await;
    let mut claims = idp.claims("alice");
    if let Some(scope) = scope {
        claims = claims.set("scope", scope);
    }
    let request = Request::builder()
        .uri("/admin")
        .header("authorization", format!("Bearer {}", idp.mint(&claims)))
        .body(Body::empty())
        .unwrap();
    let response = admin_plane(&idp).oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn the_admin_scope_alone_is_admitted() {
    assert_eq!(answer(Some(ADMIN_SCOPE)).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_admin_scope_among_others_is_admitted() {
    assert_eq!(answer(Some("read write fraiseql:admin user:list")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_admin_scope_first_in_a_list_is_admitted() {
    assert_eq!(answer(Some("fraiseql:admin read write")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_token_without_the_admin_scope_is_forbidden_and_told_which_scope() {
    let (status, body) = answer(Some("read write user:list")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains(ADMIN_SCOPE), "the refusal names the scope it wants: {body}");
}

#[tokio::test]
async fn an_empty_scope_is_forbidden() {
    assert_eq!(answer(Some("")).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_token_with_no_scope_claim_is_forbidden() {
    assert_eq!(answer(None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_scope_that_only_starts_with_the_admin_scope_is_forbidden() {
    assert_eq!(answer(Some("fraiseql:admin_readonly")).await.0, StatusCode::FORBIDDEN);
}
