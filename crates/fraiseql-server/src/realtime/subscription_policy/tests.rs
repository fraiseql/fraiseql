//! Tests for the #596 subscription-policy owner-filter derivation — the
//! security-critical decision logic (bypass / fail-closed refuse / owner eq).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test module

use fraiseql_core::security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext};

use super::{OwnerFilterOutcome, SubscriptionPolicy};
use crate::realtime::subscriptions::FilterOperator;

fn policy() -> SubscriptionPolicy {
    SubscriptionPolicy {
        owner_path:     "$.owner_id".to_string(),
        identity_field: "user_id".to_string(),
        bypass_roles:   vec!["admin".to_string()],
    }
}

/// A base anonymous context; tests add roles / enriched fields as needed.
fn ctx() -> SecurityContext {
    let now = chrono::Utc::now();
    SecurityContext {
        user_id:          fraiseql_core::types::UserId("u-1".to_string()),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        authenticated_at: now,
        expires_at:       now + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn with_enriched(mut c: SecurityContext, field: &str, value: serde_json::Value) -> SecurityContext {
    c.attributes.insert(format!("{ENRICHED_NAMESPACE_PREFIX}{field}"), value);
    c
}

#[test]
fn resolved_identity_yields_an_owner_eq_filter() {
    let outcome = {
        let c = with_enriched(ctx(), "user_id", serde_json::json!("alice"));
        policy().derive(&c.attributes, &c.roles)
    };
    match outcome {
        OwnerFilterOutcome::Filter(filter) => {
            assert_eq!(filter.field, "owner_id");
            assert_eq!(filter.operator, FilterOperator::Eq);
            assert_eq!(filter.value, serde_json::json!("alice"));
        },
        other => panic!("expected an owner filter, got {other:?}"),
    }
}

#[test]
fn a_bypass_role_grants_full_visibility() {
    let mut c = with_enriched(ctx(), "user_id", serde_json::json!("alice"));
    c.roles = vec!["admin".to_string()];
    assert!(matches!(policy().derive(&c.attributes, &c.roles), OwnerFilterOutcome::Bypass));
}

#[test]
fn unresolvable_identity_refuses_fail_closed() {
    // No enrichment configured at all → refuse (not deliver-all).
    assert!(matches!(
        {
            let c = ctx();
            policy().derive(&c.attributes, &c.roles)
        },
        OwnerFilterOutcome::Refuse(_)
    ));

    // Enrichment present but the specific field is NULL → refuse.
    let null_field = with_enriched(ctx(), "user_id", serde_json::Value::Null);
    assert!(matches!(
        policy().derive(&null_field.attributes, &null_field.roles),
        OwnerFilterOutcome::Refuse(_)
    ));

    // A DIFFERENT enriched field present, but not the one the policy names → refuse.
    let wrong_field = with_enriched(ctx(), "tenant", serde_json::json!("acme"));
    assert!(matches!(
        policy().derive(&wrong_field.attributes, &wrong_field.roles),
        OwnerFilterOutcome::Refuse(_)
    ));
}

#[test]
fn inbound_forged_owner_filter_cannot_widen_visibility() {
    // The identity value is read ONLY from the server-resolved enriched namespace.
    // A client that stuffs a plain `user_id` attribute (not the enriched key) does
    // not influence the derivation — still fail-closed.
    let mut c = ctx();
    c.attributes.insert("user_id".to_string(), serde_json::json!("mallory"));
    assert!(
        matches!(policy().derive(&c.attributes, &c.roles), OwnerFilterOutcome::Refuse(_)),
        "a non-enriched (forgeable) attribute must not resolve the owner identity"
    );
}

#[test]
fn validate_rejects_nested_or_empty_paths() {
    let ok = policy();
    assert!(ok.validate().is_ok());

    let nested = SubscriptionPolicy {
        owner_path: "$.a.b".to_string(),
        ..policy()
    };
    assert!(nested.validate().is_err(), "nested paths unsupported");

    let empty_field = SubscriptionPolicy {
        identity_field: String::new(),
        ..policy()
    };
    assert!(empty_field.validate().is_err());

    let empty_path = SubscriptionPolicy {
        owner_path: "$.".to_string(),
        ..policy()
    };
    assert!(empty_path.validate().is_err());
}
