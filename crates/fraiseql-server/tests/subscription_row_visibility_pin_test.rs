//! Row-level visibility regression for the realtime `/realtime/v1` entity-change stream
//! (#596, phase 06a — the FLIP of the phase-00 characterization).
//!
//! NOTE: this is the *dormant* push path — `RealtimeServer`/`RealtimeState` are never
//! assembled by any production binary (#605). The *live* graphql `/ws` path is covered by
//! `graphql_ws_row_visibility_pin_test.rs`. 06a's job on this seam is that it can never
//! come up deliver-all by accident: for a policy-declaring entity the delivery decision
//! (`owner_enforcement_admits`) is **fail-closed** — a subscription that did not resolve
//! an explicit `Bypass`/`Scoped` owner enforcement at subscribe time is denied.
//!
//! Phase 00 pinned the gap: with no server-side policy, principal A was delivered
//! principal B's row (`evaluate_field_filters` let it pass). That assertion is now flipped
//! — the delivery decision denies B's row to a scoped A and denies an unenforced policy
//! subscription outright. Subscribe-time refusal (the other half — an unresolvable
//! identity is refused) is covered end-to-end in `realtime::tests`.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_server::realtime::{
    delivery::owner_enforcement_admits,
    subscription_policy::OwnerEnforcement,
    subscriptions::{FieldFilter, FilterOperator},
};

/// The server-owned owner filter A resolves for a `subscription_policy` on `owner_id`.
fn a_owner_filter() -> FieldFilter {
    FieldFilter {
        field:    "owner_id".to_string(),
        operator: FilterOperator::Eq,
        value:    serde_json::json!("A"),
    }
}

#[test]
fn m596_unenforced_policy_subscription_is_denied_fail_closed() {
    // The flip: a subscription to a policy-declaring entity that reached delivery WITHOUT
    // a resolved owner enforcement (`None`) is denied — never deliver-all. This is what
    // keeps a future assembler of this dormant seam from coming up permissive.
    let b_row = serde_json::json!({ "id": 42, "owner_id": "B", "status": "approved" });
    assert!(
        !owner_enforcement_admits(&OwnerEnforcement::None, Some(&b_row)),
        "M-596: an unenforced subscription to a policy entity must be denied (was deliver-all)"
    );
    // Even with no row image at all, `None` denies.
    assert!(!owner_enforcement_admits(&OwnerEnforcement::None, None));
}

#[test]
fn m596_scoped_subscriber_does_not_receive_another_owners_row() {
    // The flip of the phase-00 assertion: principal A, scoped to `owner_id = A`, is NOT
    // delivered principal B's row.
    let b_row = serde_json::json!({ "id": 42, "owner_id": "B", "status": "approved" });
    assert!(
        !owner_enforcement_admits(&OwnerEnforcement::Scoped(a_owner_filter()), Some(&b_row)),
        "M-596: a scoped subscriber must NOT receive another owner's row"
    );

    // A's own row IS delivered.
    let a_row = serde_json::json!({ "id": 7, "owner_id": "A" });
    assert!(owner_enforcement_admits(
        &OwnerEnforcement::Scoped(a_owner_filter()),
        Some(&a_row)
    ));

    // A DELETE with no owning image cannot prove ownership → denied (scoped clients learn
    // of a delete only when a pre-image is present).
    assert!(!owner_enforcement_admits(&OwnerEnforcement::Scoped(a_owner_filter()), None));
}

#[test]
fn m596_bypass_role_retains_full_visibility() {
    let b_row = serde_json::json!({ "id": 42, "owner_id": "B" });
    assert!(
        owner_enforcement_admits(&OwnerEnforcement::Bypass, Some(&b_row)),
        "a bypass role keeps full visibility over a policy entity"
    );
}
