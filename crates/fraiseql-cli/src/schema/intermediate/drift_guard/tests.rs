//! Unit tests for the drifted-security-key guard.
//!
//! The end-to-end seam behaviour — SDK-shaped JSON in, compiled schema or loud failure
//! out — lives in `tests/sdk_security_seam_test.rs`. These cover the guard's own edges:
//! that it does not fire on correct schemas, and that its message is actionable.

use serde_json::json;

use super::reject_drifted_security_keys;

#[test]
fn a_correct_schema_passes() {
    let raw = json!({
        "types": [{
            "name": "Order",
            "fields": [{"name": "ssn", "type": "String", "requires_scope": "read:Order.ssn"}]
        }],
        "queries": [{"name": "orders", "inject_params": {"tenant_id": "jwt:org_id"}}],
        "mutations": [{"name": "createOrder", "inject_params": {
            "tenant_id": {"source": "jwt", "claim": "org_id"}
        }}]
    });

    assert!(reject_drifted_security_keys(&raw).is_ok());
}

#[test]
fn an_empty_schema_passes() {
    assert!(reject_drifted_security_keys(&json!({})).is_ok());
}

#[test]
fn the_legacy_inject_key_is_named_with_its_replacement() {
    let raw = json!({"queries": [{"name": "orders", "inject": {"tenant_id": "jwt:org_id"}}]});

    let err = reject_drifted_security_keys(&raw).expect_err("`inject` must be refused");

    assert!(err.contains("orders"), "must name the operation: {err}");
    assert!(err.contains("`inject`"), "must name the key found: {err}");
    assert!(err.contains("inject_params"), "must name the replacement: {err}");
}

#[test]
fn a_drifted_inject_key_on_a_mutation_is_refused() {
    let raw = json!({"mutations": [{"name": "createOrder", "inject": {"t": "jwt:org_id"}}]});

    let err = reject_drifted_security_keys(&raw).expect_err("mutations must be checked too");
    assert!(err.contains("createOrder"), "{err}");
}

#[test]
fn every_drifted_scope_spelling_is_refused_and_named() {
    for key in [
        "scope",
        "scopes",
        "requiresScope",
        "requiresScopes",
        "requires_scopes",
    ] {
        let raw = json!({
            "types": [{"name": "Order", "fields": [{"name": "ssn", key: "hr:view_pii"}]}]
        });

        let err =
            reject_drifted_security_keys(&raw).expect_err(&format!("`{key}` must be refused"));

        assert!(err.contains("Order.ssn"), "`{key}`: must name the field: {err}");
        assert!(err.contains(key), "`{key}`: must name the key found: {err}");
        assert!(err.contains("requires_scope"), "`{key}`: must name the replacement: {err}");
    }
}

/// An unrelated unknown key must **not** trip the guard.
///
/// This guard is deliberately a named allow-list of known-drifted security keys, not a
/// general unknown-field rejector. Widening it here would turn every additive schema
/// change into a compile error and is a separate, larger decision.
#[test]
fn an_unrelated_unknown_key_is_ignored() {
    let raw = json!({
        "types": [{"name": "Order", "fields": [{"name": "ssn", "some_future_hint": true}]}],
        "queries": [{"name": "orders", "some_future_option": 3}]
    });

    assert!(reject_drifted_security_keys(&raw).is_ok());
}
