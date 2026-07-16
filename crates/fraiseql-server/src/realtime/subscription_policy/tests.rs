//! Tests for the realtime-seam adapter (#596). The security-critical derivation
//! (`bypass` / owner-eq / fail-closed refuse, forge-proof enriched read) is tested in
//! `fraiseql-core` (`schema::subscription_policy`); here we only assert the
//! `OwnerCondition` → `OwnerEnforcement` mapping this seam adds.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test module

use fraiseql_core::schema::OwnerCondition;

use super::{OwnerEnforcement, owner_enforcement};
use crate::realtime::subscriptions::FilterOperator;

#[test]
fn bypass_condition_maps_to_bypass_enforcement() {
    assert_eq!(owner_enforcement(OwnerCondition::Bypass).unwrap(), OwnerEnforcement::Bypass);
}

#[test]
fn eq_condition_maps_to_a_scoped_owner_filter() {
    let enforcement = owner_enforcement(OwnerCondition::Eq {
        field: "owner_id".to_string(),
        value: serde_json::json!("alice"),
    })
    .unwrap();
    match enforcement {
        OwnerEnforcement::Scoped(filter) => {
            assert_eq!(filter.field, "owner_id");
            assert_eq!(filter.operator, FilterOperator::Eq);
            assert_eq!(filter.value, serde_json::json!("alice"));
        },
        other => panic!("expected a scoped owner filter, got {other:?}"),
    }
}

#[test]
fn refuse_condition_maps_to_an_error() {
    let err = owner_enforcement(OwnerCondition::Refuse("unresolvable".to_string()))
        .expect_err("a refusal must not yield an enforcement");
    assert!(err.contains("unresolvable"));
}

#[test]
fn default_enforcement_is_none() {
    assert_eq!(OwnerEnforcement::default(), OwnerEnforcement::None);
}
