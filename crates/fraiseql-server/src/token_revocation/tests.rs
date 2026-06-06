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
