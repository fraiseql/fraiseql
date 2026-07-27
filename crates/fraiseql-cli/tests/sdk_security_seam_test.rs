//! #806 / #807: the JSON → `IntermediateSchema` seam must not silently drop a
//! security declaration.
//!
//! Two authorization controls were lost between the SDKs and the compiler, both by plain
//! key drift, both with `✓ Schema compiled successfully` printed over the top:
//!
//! * **#806** — `TypeScript`, Go and Java emit server-side parameter injection under
//!   `inject_params` with a nested `{source, claim}` value. The compiler read `inject` with a flat
//!   `"jwt:<claim>"` string. `#[serde(default)]` and no `deny_unknown_fields` turned the whole map
//!   into an empty default, so the JWT tenant filter vanished from every query and mutation those
//!   SDKs produced.
//! * **#807** — Go, C# and F# emit field scopes as `scope`/`scopes`; the Rust authoring SDK emits
//!   `requiresScope`. The compiler reads `requires_scope`. The compiled field ends up with
//!   `requires_scope: None`, which the runtime field filter treats as "public and always
//!   accessible" — so a field the author declared, and the SDK validated the grammar of, is served
//!   to callers with no scopes at all.
//!
//! **Why these tests and not the existing ones.** `converter_inject_params_test.rs` and
//! `converter_field_survival_test.rs` both start from *already-constructed Rust structs*
//! and assert they survive `SchemaConverter::convert()`. They do — the converter was
//! never the broken half. Neither test ever deserializes JSON, which is the only place
//! the drift exists. Testing the half that works is why two authorization controls could
//! be dropped for several releases under a green suite.
//!
//! So every case here starts from **bytes in the shape a real SDK emits**, and either
//! survives to the compiled schema or fails the compile loudly. There is no third
//! outcome.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{IntermediateSchema, reject_drifted_security_keys},
};
use fraiseql_core::schema::InjectedParamSource;
use serde_json::json;

/// Compile a raw intermediate-schema JSON value exactly as the CLI's JSON workflow does:
/// drift guard on the raw value, then deserialize, then convert.
///
/// The ordering is the point. The guard has to see the raw JSON, because after
/// deserialization a drifted key is indistinguishable from an absent one.
fn compile(raw: &serde_json::Value) -> Result<fraiseql_core::schema::CompiledSchema, String> {
    reject_drifted_security_keys(raw)?;
    let intermediate: IntermediateSchema =
        serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    SchemaConverter::convert(intermediate).map_err(|e| e.to_string())
}

/// One `Order` type with an `ssn` field, plus whatever extra field JSON is supplied.
fn schema_with(
    field_extra: &serde_json::Value,
    query_extra: &serde_json::Value,
) -> serde_json::Value {
    let mut ssn = json!({"name": "ssn", "type": "String", "nullable": false});
    if let Some(extra) = field_extra.as_object() {
        for (k, v) in extra {
            ssn[k] = v.clone();
        }
    }

    let mut query = json!({
        "name": "orders",
        "return_type": "Order",
        "returns_list": true,
        "sql_source": "v_order"
    });
    if let Some(extra) = query_extra.as_object() {
        for (k, v) in extra {
            query[k] = v.clone();
        }
    }

    json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [{"name": "id", "type": "ID", "nullable": false}, ssn]
        }],
        "queries": [query],
        "mutations": []
    })
}

/// The compiled `orders` query's injection map.
fn injected(schema: &fraiseql_core::schema::CompiledSchema) -> Vec<(String, InjectedParamSource)> {
    schema
        .queries
        .iter()
        .find(|q| q.name == "orders")
        .expect("orders query")
        .inject_params
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The compiled `Order.ssn` field's required scope.
fn ssn_scope(schema: &fraiseql_core::schema::CompiledSchema) -> Option<String> {
    schema
        .types
        .iter()
        .find(|t| t.name == "Order")
        .expect("Order type")
        .fields
        .iter()
        .find(|f| f.name == "ssn")
        .expect("ssn field")
        .requires_scope
        .clone()
}

// ---------------------------------------------------------------------------
// #806 — injection must survive
// ---------------------------------------------------------------------------

/// The shape `TypeScript`, Go and Java all emit. This is the one that was dropped.
#[test]
fn nested_inject_params_from_the_sdks_reaches_the_compiled_schema() {
    let raw = schema_with(
        &json!({}),
        &json!({"inject_params": {"tenant_id": {"source": "jwt", "claim": "org_id"}}}),
    );

    let schema = compile(&raw).expect("SDK-shaped schema must compile");

    assert_eq!(
        injected(&schema),
        vec![("tenant_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()))],
        "the nested inject_params every non-Python SDK emits must reach the compiled query",
    );
}

/// The flat form is accepted under the same key, for hand-authored schemas.
#[test]
fn flat_inject_params_value_is_accepted_under_the_canonical_key() {
    let raw = schema_with(&json!({}), &json!({"inject_params": {"tenant_id": "jwt:org_id"}}));

    let schema = compile(&raw).expect("flat inject_params value must compile");

    assert_eq!(
        injected(&schema),
        vec![("tenant_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()))],
    );
}

/// The pre-#806 key must fail loudly rather than deserialize to an empty map.
///
/// Silence is the whole defect: an unrecognised injection key that binds to
/// `Default::default()` compiles a query with no tenant predicate and reports success.
#[test]
fn the_legacy_inject_key_is_refused_not_ignored() {
    let raw = schema_with(&json!({}), &json!({"inject": {"tenant_id": "jwt:org_id"}}));

    let err = compile(&raw).expect_err("the legacy `inject` key must not compile silently");

    assert!(err.contains("inject_params"), "the error must name the key to use, got: {err}",);
}

/// A malformed source string must be rejected, not silently carried.
#[test]
fn an_injection_source_without_a_scheme_is_refused() {
    let raw = schema_with(&json!({}), &json!({"inject_params": {"tenant_id": "org_id"}}));

    let err = compile(&raw).expect_err("a source with no `<scheme>:` prefix must not compile");
    assert!(err.contains("tenant_id"), "the error must name the offending param, got: {err}");
}

// ---------------------------------------------------------------------------
// #807 — field scopes must survive
// ---------------------------------------------------------------------------

/// The documented key, which Java and Elixir already emit, keeps working.
#[test]
fn requires_scope_reaches_the_compiled_field() {
    let raw = schema_with(&json!({"requires_scope": "read:Order.ssn"}), &json!({}));

    let schema = compile(&raw).expect("requires_scope must compile");

    assert_eq!(ssn_scope(&schema), Some("read:Order.ssn".to_string()));
}

/// Every drifted spelling must fail the compile loudly.
///
/// These are not hypothetical: each is emitted by a shipped SDK today, and each produced
/// a compiled field with `requires_scope: None` — a PII column the author gated, the SDK
/// validated, and the compiler quietly ungated.
#[test]
fn every_drifted_scope_key_is_refused_not_ignored() {
    for (key, value) in [
        ("scope", json!("hr:view_pii")),             // Go, C#, F#
        ("scopes", json!(["hr:view_pii"])),          // Go, C# (plural)
        ("requiresScope", json!("hr:view_pii")),     // Rust authoring SDK
        ("requiresScopes", json!(["hr:view_pii"])),  // Rust authoring SDK (plural)
        ("requires_scopes", json!(["hr:view_pii"])), // Java, Elixir (plural)
    ] {
        let raw = schema_with(&json!({ key: value }), &json!({}));

        let result = compile(&raw);

        let err = result.err().unwrap_or_else(|| {
            panic!(
                "field key `{key}` compiled successfully — it must be refused. Compiling it \
                 silently is what left `requires_scope: None` on an author-declared PII field."
            )
        });
        assert!(
            err.contains("requires_scope"),
            "`{key}`: the error must name the canonical key, got: {err}",
        );
    }
}
