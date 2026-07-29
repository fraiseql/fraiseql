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

/// `build_security_context` is the one function that turns a validated token into
/// a `SecurityContext`, so every transport gets `org_id` → `tenant_id` and the
/// forwarded claims.
///
/// The MCP transport called `SecurityContext::from_user` directly, which leaves
/// `tenant_id` unset and `attributes` empty — so an MCP caller's `org_id` never
/// became a tenant key and every `SessionVariableSource::Jwt` mapping resolved to
/// nothing (#858). These assertions are made against the shared builder rather
/// than the HTTP extractor precisely because the shared builder is what both
/// transports now call.
mod shared_security_context {
    use super::{HashMap, json, user_with_claims};
    use crate::extractors::build_security_context;

    #[test]
    fn org_id_claim_becomes_the_tenant_id() {
        let mut extra = HashMap::new();
        extra.insert("org_id".to_string(), json!("acme"));

        let ctx = build_security_context(&user_with_claims(extra), "req-1".to_string());

        assert_eq!(
            ctx.tenant_id.as_ref().map(|t| t.0.as_str()),
            Some("acme"),
            "org_id must resolve to a tenant key for per-tenant dispatch",
        );
    }

    #[test]
    fn extra_claims_are_forwarded_to_attributes() {
        let mut extra = HashMap::new();
        extra.insert("department".to_string(), json!("finance"));

        let ctx = build_security_context(&user_with_claims(extra), "req-1".to_string());

        assert_eq!(ctx.attributes.get("department"), Some(&json!("finance")));
    }

    /// A token cannot forge a framework-reserved attribute by naming a claim after
    /// one (#390) — on any transport.
    #[test]
    fn framework_namespaced_claims_are_not_forwarded() {
        let mut extra = HashMap::new();
        extra.insert("fraiseql.actor_type".to_string(), json!("system"));

        let ctx = build_security_context(&user_with_claims(extra), "req-1".to_string());

        assert_ne!(ctx.attributes.get("fraiseql.actor_type"), Some(&json!("system")));
    }

    #[test]
    fn no_org_id_claim_leaves_the_tenant_unset() {
        let ctx = build_security_context(&user_with_claims(HashMap::new()), "req-1".to_string());
        assert!(ctx.tenant_id.is_none());
    }
}
