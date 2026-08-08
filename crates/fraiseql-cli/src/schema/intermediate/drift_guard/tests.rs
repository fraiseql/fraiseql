//! Unit tests for the drifted-key guard.
//!
//! The end-to-end seam behaviour — SDK-shaped JSON in, compiled schema or loud failure
//! out — lives in `tests/sdk_security_seam_test.rs` for the security keys and
//! `tests/seam_query_shape_test.rs` for `#890`'s `return_array`. These cover the guard's
//! own edges: that it does not fire on correct schemas, and that its message is actionable.

use serde_json::json;

use super::reject_drifted_keys;

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

    assert!(reject_drifted_keys(&raw).is_ok());
}

#[test]
fn an_empty_schema_passes() {
    assert!(reject_drifted_keys(&json!({})).is_ok());
}

#[test]
fn the_legacy_inject_key_is_named_with_its_replacement() {
    let raw = json!({"queries": [{"name": "orders", "inject": {"tenant_id": "jwt:org_id"}}]});

    let err = reject_drifted_keys(&raw).expect_err("`inject` must be refused");

    assert!(err.contains("orders"), "must name the operation: {err}");
    assert!(err.contains("`inject`"), "must name the key found: {err}");
    assert!(err.contains("inject_params"), "must name the replacement: {err}");
}

#[test]
fn a_drifted_inject_key_on_a_mutation_is_refused() {
    let raw = json!({"mutations": [{"name": "createOrder", "inject": {"t": "jwt:org_id"}}]});

    let err = reject_drifted_keys(&raw).expect_err("mutations must be checked too");
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

        let err = reject_drifted_keys(&raw).expect_err(&format!("`{key}` must be refused"));

        assert!(err.contains("Order.ssn"), "`{key}`: must name the field: {err}");
        assert!(err.contains(key), "`{key}`: must name the key found: {err}");
        assert!(err.contains("requires_scope"), "`{key}`: must name the replacement: {err}");
    }
}

/// `#890`: `return_array` is the `[queries.*]` TOML spelling of `returns_list`.
///
/// The assertion is on the *distinctive* half of the message, not on the word
/// `returns_list`. Serde's `deny_unknown_fields` error already lists every valid field
/// name — including `returns_list` — so a test asserting only that would pass with this
/// guard entry deleted, which is `#909`'s lesson.
#[test]
fn the_toml_return_array_spelling_is_named_with_its_replacement() {
    for collection in ["queries", "mutations"] {
        let raw = json!({collection: [{"name": "listTenants", "return_array": true}]});

        let err = reject_drifted_keys(&raw)
            .expect_err(&format!("`return_array` must be refused in {collection}"));

        assert!(err.contains("listTenants"), "must name the operation: {err}");
        assert!(err.contains("`return_array`"), "must name the key found: {err}");
        assert!(
            err.contains("TOML `[queries.*]` spelling"),
            "must explain that this is the other surface's spelling — the half serde's own \
             message cannot supply: {err}"
        );
        assert!(err.contains("`returns_list`"), "must name the replacement: {err}");
    }
}

/// The correct spelling must not trip the guard — otherwise every list query breaks.
#[test]
fn the_canonical_returns_list_spelling_passes() {
    let raw = json!({"queries": [{"name": "listTenants", "returns_list": true}]});
    assert!(reject_drifted_keys(&raw).is_ok());
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

    assert!(reject_drifted_keys(&raw).is_ok());
}
