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
/// a `SecurityContext`, so every transport derives the tenant and forwards the claims
/// the same way.
///
/// The MCP transport called `SecurityContext::from_user` directly, which leaves
/// `tenant_id` unset and `attributes` empty, so an MCP caller never had a tenant and
/// every `SessionVariableSource::Jwt` mapping resolved to nothing (#858). These
/// assertions are made against the shared builder rather than the HTTP extractor
/// precisely because the shared builder is what every transport calls.
mod shared_security_context {
    use super::{HashMap, json, user_with_claims};
    use crate::extractors::build_security_context;

    fn tenant_of(
        claims: &[(&str, serde_json::Value)],
        tenant_claim: Option<&str>,
    ) -> Option<String> {
        let extra: HashMap<_, _> =
            claims.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        build_security_context(&user_with_claims(extra), "req-1".to_string(), tenant_claim)
            .tenant_id
            .map(|t| t.0)
    }

    /// The tenant is the configured claim (#1388). It used to be `org_id` whatever the
    /// schema said, so a token carrying both scoped requests by the wrong one.
    #[test]
    fn the_configured_tenant_claim_becomes_the_tenant_id() {
        let claims = [("tenant_id", json!("a")), ("org_id", json!("b"))];
        assert_eq!(tenant_of(&claims, Some("tenant_id")).as_deref(), Some("a"));
        assert_eq!(tenant_of(&claims, Some("org_id")).as_deref(), Some("b"));
    }

    #[test]
    fn org_id_is_not_the_tenant_unless_it_is_the_configured_claim() {
        assert_eq!(tenant_of(&[("org_id", json!("b"))], Some("tenant_id")), None);
    }

    /// No `TenantClaim` (a mount that never configured one) means no tenant, never a
    /// guess: everything that scopes by tenant then fails closed.
    #[test]
    fn without_a_configured_claim_there_is_no_tenant() {
        assert_eq!(tenant_of(&[("tenant_id", json!("a")), ("org_id", json!("b"))], None), None);
    }

    /// A numeric tenant identifier is the tenant spelled in decimal; a value that is not
    /// a scalar identifier is no tenant.
    #[test]
    fn a_numeric_tenant_claim_is_a_tenant_and_a_structured_one_is_not() {
        assert_eq!(
            tenant_of(&[("tenant_id", json!(42))], Some("tenant_id")).as_deref(),
            Some("42")
        );
        for value in [
            json!({"id": "a"}),
            json!(["a"]),
            json!(true),
            json!(null),
            json!(""),
        ] {
            assert_eq!(
                tenant_of(&[("tenant_id", value.clone())], Some("tenant_id")),
                None,
                "{value}"
            );
        }
    }

    #[test]
    fn extra_claims_are_forwarded_to_attributes() {
        let mut extra = HashMap::new();
        extra.insert("department".to_string(), json!("finance"));

        let ctx = build_security_context(&user_with_claims(extra), "req-1".to_string(), None);

        assert_eq!(ctx.attributes.get("department"), Some(&json!("finance")));
    }

    /// A token cannot forge a framework-reserved attribute by naming a claim after
    /// one (#390) — on any transport.
    #[test]
    fn framework_namespaced_claims_are_not_forwarded() {
        let mut extra = HashMap::new();
        extra.insert("fraiseql.actor_type".to_string(), json!("system"));

        let ctx = build_security_context(&user_with_claims(extra), "req-1".to_string(), None);

        assert_ne!(ctx.attributes.get("fraiseql.actor_type"), Some(&json!("system")));
    }
}
