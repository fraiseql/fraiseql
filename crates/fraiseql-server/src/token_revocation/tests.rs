//! Unit tests for token-revocation backend selection (#357).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use fraiseql_core::schema::{CompiledSchema, SecurityConfig};
use serde_json::json;

use super::revocation_manager_from_schema_in;

/// Deployment modes, named so call sites read as the scenario they test.
const PRODUCTION: bool = true;
#[cfg(feature = "redis-rate-limiting")]
const DEVELOPMENT: bool = false;

fn schema_with_revocation(value: serde_json::Value) -> CompiledSchema {
    // JSON in, typed field out: a null section means "absent" (the compiler's
    // spelling of an unset `[security.token_revocation]`), anything else must
    // parse as the typed config — the same rule the schema seam enforces (#977).
    let security = SecurityConfig {
        token_revocation: if value.is_null() {
            None
        } else {
            Some(serde_json::from_value(value).expect("valid token_revocation JSON"))
        },
        ..SecurityConfig::default()
    };
    CompiledSchema {
        security: Some(security),
        ..CompiledSchema::default()
    }
}

#[tokio::test]
async fn unknown_backend_is_rejected_loudly() {
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "frobnicate" }));
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION).await.is_err(),
        "an unrecognised revocation backend must be a startup error, not a silent in-memory fallback"
    );
}

#[tokio::test]
async fn postgres_backend_is_deferred_on_the_generic_path() {
    // postgres needs a database connection; the generic construction path defers it
    // (the PostgreSQL runtime builds it via build_postgres_revocation_manager).
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "postgres" }));
    let mgr = revocation_manager_from_schema_in(&schema, PRODUCTION)
        .await
        .expect("postgres defers (Ok(None)) on the generic path, it does not error");
    assert!(mgr.is_none(), "postgres backend is deferred to the PostgreSQL runtime path");
}

#[tokio::test]
async fn memory_backend_builds_a_manager() {
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "memory" }));
    let mgr = revocation_manager_from_schema_in(&schema, PRODUCTION)
        .await
        .expect("memory backend builds");
    assert!(mgr.is_some());
}

#[tokio::test]
async fn disabled_revocation_builds_nothing() {
    let schema = schema_with_revocation(json!({ "enabled": false, "backend": "postgres" }));
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION)
            .await
            .expect("disabled revocation is not an error")
            .is_none()
    );
}

#[tokio::test]
async fn null_token_revocation_is_treated_as_absent() {
    // The CLI compiler emits `token_revocation: null` when the section is absent — the
    // common case. This must be Ok(None), not a hard ConfigError (regression: a `?` on
    // the parse turned null into a boot failure, caught by pipeline_e2e).
    let schema = schema_with_revocation(serde_json::Value::Null);
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION)
            .await
            .expect("null token_revocation means 'not configured', not an error")
            .is_none()
    );
}

#[tokio::test]
async fn absent_token_revocation_key_builds_nothing() {
    let schema = CompiledSchema::default();
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION)
            .await
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

// ── #770: the redis arm must fail loud, not downgrade ────────────────────────
//
// `backend = "redis"` is a request for revocation state shared across replicas.
// Before #770 two configuration mistakes booted anyway on a silent in-memory
// store: a malformed URL (`Client::open` rejected it, the arm warned and fell
// back) and a well-formed URL pointing at nothing (`Client::open` does not
// connect, so the store "built" and every runtime call failed instead).

#[tokio::test]
async fn the_undocumented_env_backend_alias_is_rejected() {
    // "env" was accepted as an undocumented alias for "memory" — an operator typo
    // away from silently running per-process revocation while believing an
    // environment-driven backend was in effect.
    let schema = schema_with_revocation(json!({ "enabled": true, "backend": "env" }));
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION).await.is_err(),
        "backend = \"env\" is not a documented backend and must be rejected like any \
         other unknown value"
    );
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
async fn a_malformed_redis_url_refuses_to_boot_in_production() {
    // `rediss//cache:6379` (missing colon) is the config-typo path from #770.
    let schema = schema_with_revocation(json!({
        "enabled": true,
        "backend": "redis",
        "redis_url": "rediss//cache:6379",
        "fail_open": false,
    }));
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION).await.is_err(),
        "a malformed redis_url must refuse to boot, not warn and downgrade to \
         per-process revocation"
    );
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
async fn an_unreachable_redis_url_refuses_to_boot_in_production() {
    // Well-formed URL, nothing listening: the case `Client::open` cannot see.
    let schema = schema_with_revocation(json!({
        "enabled": true,
        "backend": "redis",
        "redis_url": "redis://127.0.0.1:6390",
        "fail_open": false,
    }));
    assert!(
        revocation_manager_from_schema_in(&schema, PRODUCTION).await.is_err(),
        "a well-formed redis_url pointing at nothing must refuse to boot: the store \
         used to build without connecting, and revocations then lived only in this \
         process's memory"
    );
}

#[cfg(feature = "redis-rate-limiting")]
#[tokio::test]
async fn an_unreachable_redis_url_downgrades_with_a_warning_in_development() {
    let schema = schema_with_revocation(json!({
        "enabled": true,
        "backend": "redis",
        "redis_url": "redis://127.0.0.1:6390",
    }));
    let mgr = revocation_manager_from_schema_in(&schema, DEVELOPMENT)
        .await
        .expect("development boots on the in-memory fallback with a warning");
    assert!(mgr.is_some(), "the development fallback still builds a manager");
}
