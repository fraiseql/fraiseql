//! Tests for request-level extractors.

use std::collections::HashMap;

use axum::extract::FromRequestParts;
use chrono::Utc;
use fraiseql_core::{security::AuthenticatedUser, types::UserId};
use serde_json::json;

use super::OptionalSecurityContext;
use crate::middleware::AuthUser;

/// Build an empty request and run the extractor against the given authenticated
/// user, returning the resulting `SecurityContext` (the user is always present).
async fn context_for(user: AuthenticatedUser) -> fraiseql_core::security::SecurityContext {
    let (mut parts, _body) = axum::http::Request::builder()
        .body(axum::body::Body::empty())
        .expect("empty request body builds")
        .into_parts();
    parts.extensions.insert(AuthUser(user));

    let OptionalSecurityContext(ctx) = OptionalSecurityContext::from_request_parts(&mut parts, &())
        .await
        .expect("OptionalSecurityContext extraction is infallible here");
    ctx.expect("an AuthUser in extensions yields a SecurityContext")
}

fn user_with_claims(extra_claims: HashMap<String, serde_json::Value>) -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: UserId::new("user-1"),
        scopes: vec![],
        expires_at: Utc::now() + chrono::Duration::hours(1),
        email: None,
        display_name: None,
        extra_claims,
    }
}

/// The HTTP extractor surfaces JWT `roles` into `SecurityContext.roles`, so a
/// `requires_role`-gated operation becomes reachable over HTTP with a correctly
/// scoped bearer token (#503).
#[tokio::test]
async fn extractor_populates_roles_from_jwt_roles_claim() {
    let mut extra = HashMap::new();
    extra.insert("roles".to_string(), json!(["report_reader"]));

    let ctx = context_for(user_with_claims(extra)).await;

    assert!(
        ctx.has_role("report_reader"),
        "roles must be reachable for the requires_role gate"
    );
}

/// A scalar `role` claim is honoured the same way.
#[tokio::test]
async fn extractor_populates_roles_from_scalar_role_claim() {
    let mut extra = HashMap::new();
    extra.insert("role".to_string(), json!("admin"));

    let ctx = context_for(user_with_claims(extra)).await;

    assert_eq!(ctx.roles, vec!["admin".to_string()]);
}

/// The role claim is still forwarded into `attributes` (for RLS / session vars),
/// in addition to populating `roles` — the two surfaces are independent.
#[tokio::test]
async fn extractor_keeps_role_claim_in_attributes_too() {
    let mut extra = HashMap::new();
    extra.insert("roles".to_string(), json!(["report_reader"]));

    let ctx = context_for(user_with_claims(extra)).await;

    assert_eq!(ctx.attributes.get("roles"), Some(&json!(["report_reader"])));
}

/// Without any role claim, `roles` stays empty — gated operations remain denied.
#[tokio::test]
async fn extractor_leaves_roles_empty_without_claim() {
    let ctx = context_for(user_with_claims(HashMap::new())).await;
    assert!(ctx.roles.is_empty());
}
