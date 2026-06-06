//! Unit tests for token-revocation backend selection (#357).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use fraiseql_core::schema::{CompiledSchema, SecurityConfig};
use serde_json::json;

use super::revocation_manager_from_schema;

fn schema_with_revocation(value: serde_json::Value) -> CompiledSchema {
    let mut security = SecurityConfig::default();
    security.additional.insert("token_revocation".to_string(), value);
    CompiledSchema {
        security: Some(security),
        ..CompiledSchema::default()
    }
}

#[test]
fn unknown_backend_is_rejected_loudly() {
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "frobnicate" }));
    assert!(
        revocation_manager_from_schema(&schema).is_err(),
        "an unrecognised revocation backend must be a startup error, not a silent in-memory fallback"
    );
}

#[test]
fn postgres_backend_is_deferred_on_the_generic_path() {
    // postgres needs a database connection; the generic construction path defers it
    // (the PostgreSQL runtime builds it via build_postgres_revocation_manager).
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "postgres" }));
    let mgr = revocation_manager_from_schema(&schema)
        .expect("postgres defers (Ok(None)) on the generic path, it does not error");
    assert!(mgr.is_none(), "postgres backend is deferred to the PostgreSQL runtime path");
}

#[test]
fn memory_backend_builds_a_manager() {
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "memory" }));
    let mgr = revocation_manager_from_schema(&schema).expect("memory backend builds");
    assert!(mgr.is_some());
}

#[test]
fn disabled_revocation_builds_nothing() {
    let schema = schema_with_revocation(json!({ "enabled": false, "backend": "postgres" }));
    assert!(
        revocation_manager_from_schema(&schema)
            .expect("disabled revocation is not an error")
            .is_none()
    );
}

#[test]
fn null_token_revocation_is_treated_as_absent() {
    // The CLI compiler emits `token_revocation: null` when the section is absent — the
    // common case. This must be Ok(None), not a hard ConfigError (regression: a `?` on
    // the parse turned null into a boot failure, caught by pipeline_e2e).
    let schema = schema_with_revocation(serde_json::Value::Null);
    assert!(
        revocation_manager_from_schema(&schema)
            .expect("null token_revocation means 'not configured', not an error")
            .is_none()
    );
}

#[test]
fn absent_token_revocation_key_builds_nothing() {
    let schema = CompiledSchema::default();
    assert!(
        revocation_manager_from_schema(&schema)
            .expect("no security section is ok")
            .is_none()
    );
}
