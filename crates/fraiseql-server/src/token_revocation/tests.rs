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

// ── The `cfg`-off arm must refuse, not downgrade ─────────────────────────────
//
// A revocation store that is per-process instead of shared is not a degraded
// service — it is a silently absent one: a token revoked on replica A stays
// valid on replicas B and C for its full lifetime. The two ways to reach an
// in-memory store while `backend = "redis"` was configured (the feature not
// compiled in, and the Redis connection failing) both used to `warn!` and carry
// on. In production both now refuse to boot, mirroring `observer_transport_check`.

#[test]
fn redis_backend_refuses_in_production_when_it_cannot_be_provided() {
    assert!(
        super::redis_revocation_unavailable_check(
            "the `redis-rate-limiting` feature is not compiled in",
            true
        )
        .is_err(),
        "an unusable Redis revocation store must refuse to boot in production rather than \
         silently downgrading to per-process state"
    );
}

#[test]
fn redis_backend_downgrades_with_a_warning_in_development() {
    assert!(
        super::redis_revocation_unavailable_check(
            "the `redis-rate-limiting` feature is not compiled in",
            false
        )
        .is_ok(),
        "development must still boot on the in-memory fallback"
    );
}

#[test]
fn the_refusal_names_the_cause_and_the_way_out() {
    let err = super::redis_revocation_unavailable_check("connection refused", true)
        .expect_err("production refuses");
    let msg = err.to_string();
    assert!(msg.contains("connection refused"), "the refusal must name the cause: {msg}");
    assert!(
        msg.contains("token_revocation"),
        "the refusal must name the config section: {msg}"
    );
}
