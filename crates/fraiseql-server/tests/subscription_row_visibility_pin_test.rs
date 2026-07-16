//! Baseline pin for #596: `/ws` fan-out ignores row-level visibility (phase 00).
//!
//! Two principals subscribe to the same entity. A row owned by principal B is
//! delivered to principal A because the push path enforces no row boundary:
//! subscriptions carry client-supplied field filters (cooperative, not a
//! security boundary) and there is no server-derived owner policy. The
//! production `RlsEvaluator` has no non-test implementor, so the delivery
//! pipeline's RLS hook is effectively permissive by default.
//!
//! `// M-596` marks the assertion phase 06 flips: once an entity declares a
//! `subscription_policy`, principal A must NOT receive principal B's row.
//!
//! This is the focused, infra-free characterization (subscription manager +
//! field-filter delivery). The full two-principal WS+DB conformance test lands
//! in phase 06 next to the cascade two-tenant test.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_server::realtime::{
    delivery::evaluate_field_filters,
    subscriptions::{SubscriptionDetails, SubscriptionManager},
};

/// A synthetic "connection subscribes without any field filter" — the common
/// case: the client just asks for an entity's change stream.
const fn unfiltered_details(context_hash: u64) -> SubscriptionDetails {
    SubscriptionDetails {
        event_filter:          None,
        field_filters:         Vec::new(),
        security_context_hash: context_hash,
    }
}

#[test]
fn pin_596_principal_a_receives_principal_b_row() {
    let manager = SubscriptionManager::new(1024);

    // Principal A subscribes to `Order`. A's security context hashes to some value;
    // the manager stores it but derives NO owner condition from it.
    let principal_a_hash = 0xA;
    assert!(
        manager
            .subscribe("conn-A", "Order", unfiltered_details(principal_a_hash))
            .unwrap()
    );

    // A is the sole subscriber to `Order`.
    let subscribers = manager.get_subscribers("Order").expect("A is subscribed");
    assert_eq!(subscribers.len(), 1);
    let (conn_id, details) = &subscribers[0];
    assert_eq!(conn_id, "conn-A");

    // An `Order` row owned by principal B is inserted. The after-image carries the
    // owner column, but the delivery path has no policy binding it to A's identity.
    let b_row = serde_json::json!({ "id": 42, "owner_id": "B", "status": "approved" });

    // The only gate before delivery is `evaluate_field_filters` — and A supplied
    // none, so B's row passes. THIS is the #596 gap: A receives B's after-image.
    assert!(
        evaluate_field_filters(&details.field_filters, Some(&b_row)),
        "M-596: with no server-side row policy, principal A is delivered principal B's row. \
         Phase 06 adds a `subscription_policy` that derives an owner condition from A's \
         enriched identity at subscribe time; when it lands, A must be filtered out here."
    );
}

#[test]
fn pin_596_client_field_filters_are_cooperative_not_a_boundary() {
    // A client CAN scope itself with a field filter, but nothing forces it to —
    // and a malicious client simply omits the filter (previous test) or supplies
    // one that widens visibility. Demonstrate that the filter is entirely
    // client-chosen: a filter naming the wrong owner still "passes" its own rows.
    let details = SubscriptionDetails {
        event_filter:          None,
        field_filters:         fraiseql_server::realtime::subscriptions::parse_filter(
            "owner_id=eq.B",
        )
        .unwrap(),
        security_context_hash: 0xA,
    };
    // Principal A (hash 0xA) asks — cooperatively — to see owner B's rows. The
    // server honors it because field filters are not an authorization boundary.
    let b_row = serde_json::json!({ "id": 7, "owner_id": "B" });
    assert!(
        evaluate_field_filters(&details.field_filters, Some(&b_row)),
        "M-596: field filters are client-supplied and cooperative — a client can request \
         another principal's rows and the push path will deliver them. Only a server-derived \
         policy (phase 06) closes this."
    );
}
