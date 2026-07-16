//! Tests for the #596 subscription-policy owner-condition derivation — the
//! security-critical decision logic (bypass / fail-closed refuse / owner eq).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test module

use std::collections::HashMap;

use serde_json::Value;

use super::{OwnerCondition, SubscriptionPolicy};
use crate::security::ENRICHED_NAMESPACE_PREFIX;

fn policy() -> SubscriptionPolicy {
    SubscriptionPolicy {
        owner_path:     "$.owner_id".to_string(),
        identity_field: "user_id".to_string(),
        bypass_roles:   vec!["admin".to_string()],
    }
}

/// Attributes carrying one server-resolved enriched field.
fn enriched(field: &str, value: Value) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert(format!("{ENRICHED_NAMESPACE_PREFIX}{field}"), value);
    m
}

#[test]
fn resolved_identity_yields_an_owner_eq_condition() {
    let attrs = enriched("user_id", serde_json::json!("alice"));
    match policy().derive(&attrs, &[]) {
        OwnerCondition::Eq { field, value } => {
            assert_eq!(field, "owner_id");
            assert_eq!(value, serde_json::json!("alice"));
        },
        other => panic!("expected an owner eq condition, got {other:?}"),
    }
}

#[test]
fn a_bypass_role_grants_full_visibility() {
    let attrs = enriched("user_id", serde_json::json!("alice"));
    let roles = vec!["admin".to_string()];
    assert!(matches!(policy().derive(&attrs, &roles), OwnerCondition::Bypass));
}

#[test]
fn unresolvable_identity_refuses_fail_closed() {
    // No enrichment configured at all → refuse (not deliver-all).
    assert!(matches!(policy().derive(&HashMap::new(), &[]), OwnerCondition::Refuse(_)));

    // Enrichment present but the specific field is NULL → refuse.
    let null_field = enriched("user_id", Value::Null);
    assert!(matches!(policy().derive(&null_field, &[]), OwnerCondition::Refuse(_)));

    // A DIFFERENT enriched field present, but not the one the policy names → refuse.
    let wrong_field = enriched("tenant", serde_json::json!("acme"));
    assert!(matches!(policy().derive(&wrong_field, &[]), OwnerCondition::Refuse(_)));
}

#[test]
fn inbound_forged_attribute_cannot_widen_visibility() {
    // The identity value is read ONLY from the server-resolved enriched namespace.
    // A client that stuffs a plain `user_id` attribute (not the enriched key) does not
    // influence the derivation — still fail-closed.
    let mut attrs = HashMap::new();
    attrs.insert("user_id".to_string(), serde_json::json!("mallory"));
    assert!(
        matches!(policy().derive(&attrs, &[]), OwnerCondition::Refuse(_)),
        "a non-enriched (forgeable) attribute must not resolve the owner identity"
    );
}

#[test]
fn validate_rejects_nested_or_empty_paths() {
    assert!(policy().validate().is_ok());

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
