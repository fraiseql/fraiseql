//! Tests for service-account authentication (ADR-0018): match/refuse, fail-closed
//! indistinguishability, ceiling, and the `static_enriched` injection.
#![allow(clippy::unwrap_used)] // Reason: test code

use std::collections::HashMap;

use axum::http::{HeaderMap, HeaderValue};
use fraiseql_core::security::{ActorType, ENRICHED_NAMESPACE_PREFIX};

use super::{SaAuth, ServiceAccountAuthenticator, ServiceAccountConfig};

fn config(secret_env: &str) -> HashMap<String, ServiceAccountConfig> {
    HashMap::from([(
        "reconciler".to_string(),
        ServiceAccountConfig {
            secret_env:      secret_env.to_string(),
            roles:           vec!["ledger:read".to_string()],
            scopes:          vec![],
            tenant:          Some("acme".to_string()),
            static_enriched: HashMap::from([(
                "user_id".to_string(),
                serde_json::json!("svc-reconciler"),
            )]),
        },
    )])
}

/// An authenticator whose one account's secret is the literal `s3cret`.
fn authenticator() -> std::sync::Arc<ServiceAccountAuthenticator> {
    ServiceAccountAuthenticator::from_config(&config("SA_SECRET"), |env| {
        (env == "SA_SECRET").then(|| "s3cret".to_string())
    })
    .expect("one account resolves")
}

fn headers_with(api_key: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-api-key", HeaderValue::from_str(api_key).unwrap());
    h
}

#[test]
fn matching_secret_yields_a_service_account_context_with_its_ceiling() {
    let ctx = authenticator().authenticate(&headers_with("s3cret")).expect("match");
    assert_eq!(ctx.user_id.as_str(), "service_account:reconciler");
    assert_eq!(ctx.actor_type(), ActorType::ServiceAccount);
    assert!(ctx.has_role("ledger:read"));
    assert_eq!(ctx.tenant_id.as_ref().map(|t| t.as_str()), Some("acme"));
}

#[test]
fn static_enriched_is_injected_under_the_forge_proof_namespace() {
    let ctx = authenticator().authenticate(&headers_with("s3cret")).unwrap();
    let key = format!("{ENRICHED_NAMESPACE_PREFIX}user_id");
    assert_eq!(ctx.attributes.get(&key), Some(&serde_json::json!("svc-reconciler")));
}

#[test]
fn an_apikey_scheme_prefix_on_the_value_is_accepted() {
    assert!(authenticator().authenticate(&headers_with("ApiKey s3cret")).is_some());
    assert!(authenticator().authenticate(&headers_with("Bearer s3cret")).is_some());
}

#[test]
fn wrong_secret_and_absent_header_are_indistinguishable_none() {
    // Fail-closed, no oracle: a bad secret and a missing header both yield None.
    assert!(authenticator().authenticate(&headers_with("wrong")).is_none());
    assert!(authenticator().authenticate(&HeaderMap::new()).is_none());
}

#[test]
fn header_present_detects_a_non_empty_secret_header() {
    assert!(authenticator().header_present(&headers_with("anything")));
    assert!(!authenticator().header_present(&HeaderMap::new()));
}

#[test]
fn an_account_whose_secret_env_is_unset_is_skipped() {
    // No account resolves → no authenticator (the account is unusable, not anonymous).
    let auth = ServiceAccountAuthenticator::from_config(&config("MISSING_ENV"), |_| None);
    assert!(auth.is_none());
}

#[test]
fn an_empty_secret_is_rejected_at_build_time() {
    let auth = ServiceAccountAuthenticator::from_config(&config("EMPTY"), |_| Some(String::new()));
    assert!(auth.is_none(), "an empty secret must not produce a usable account");
}

#[test]
fn resolve_rejects_a_jwt_plus_secret_as_ambiguous() {
    // Rider 2 / ADR-0018 amendment: two candidate principals on one request → reject.
    assert!(matches!(
        authenticator().resolve(&headers_with("s3cret"), true),
        SaAuth::Ambiguous
    ));
}

#[test]
fn resolve_authenticates_a_matching_secret_without_a_jwt() {
    assert!(matches!(
        authenticator().resolve(&headers_with("s3cret"), false),
        SaAuth::Authenticated(_)
    ));
}

#[test]
fn resolve_reports_unmatched_for_a_present_but_bad_secret() {
    assert!(matches!(
        authenticator().resolve(&headers_with("wrong"), false),
        SaAuth::Unmatched
    ));
}

#[test]
fn resolve_passes_through_when_no_secret_header_regardless_of_jwt() {
    assert!(matches!(authenticator().resolve(&HeaderMap::new(), false), SaAuth::NoSecret));
    assert!(matches!(authenticator().resolve(&HeaderMap::new(), true), SaAuth::NoSecret));
}

#[test]
fn a_ceilingless_account_authenticates_with_no_authority() {
    let cfg = HashMap::from([(
        "bare".to_string(),
        ServiceAccountConfig {
            secret_env:      "BARE_SECRET".to_string(),
            roles:           vec![],
            scopes:          vec![],
            tenant:          None,
            static_enriched: HashMap::new(),
        },
    )]);
    let auth =
        ServiceAccountAuthenticator::from_config(&cfg, |_| Some("open".to_string())).unwrap();
    let ctx = auth.authenticate(&headers_with("open")).unwrap();
    assert_eq!(ctx.user_id.as_str(), "service_account:bare");
    assert!(ctx.roles.is_empty(), "no roles → RLS/field-authz deny writes");
    assert!(ctx.scopes.is_empty());
    assert!(ctx.tenant_id.is_none());
}
