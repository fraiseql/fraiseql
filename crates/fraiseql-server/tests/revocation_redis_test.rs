//! Cross-replica token revocation against a real Redis (#770).
//!
//! Two `TokenRevocationManager`s built through the shipped construction path
//! (`revocation_manager_from_schema_in`, production mode) and sharing one Redis
//! stand in for two server replicas. A token revoked on replica A must be
//! rejected on replica B — the property `backend = "redis"` promises and the
//! silent in-memory downgrade (#770) silently broke.
//!
//! Requires Redis (`REDIS_URL`); skips gracefully when unset. Wired into the
//! Dagger `integration: redis` leg, which binds a real Redis.
#![cfg(feature = "redis-rate-limiting")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stderr)] // Reason: test code.

use fraiseql_core::schema::{CompiledSchema, SecurityConfig};
use fraiseql_server::token_revocation::{TokenRejection, revocation_manager_from_schema_in};
use serde_json::json;

fn schema_with_redis_revocation(redis_url: &str) -> CompiledSchema {
    let mut security = SecurityConfig::default();
    security.additional.insert(
        "token_revocation".to_string(),
        json!({
            "enabled": true,
            "backend": "redis",
            "redis_url": redis_url,
            "require_jti": true,
            "fail_open": false,
        }),
    );
    CompiledSchema {
        security: Some(security),
        ..CompiledSchema::default()
    }
}

fn unique(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-770-{nanos}")
}

#[tokio::test]
async fn a_token_revoked_on_one_replica_is_rejected_on_every_replica() {
    let Ok(url) = std::env::var("REDIS_URL") else {
        eprintln!(
            "skipping a_token_revoked_on_one_replica_is_rejected_on_every_replica: REDIS_URL unset"
        );
        return;
    };

    let schema = schema_with_redis_revocation(&url);
    // Production mode: with Redis actually reachable this must boot, proving the
    // fail-loud guard refuses only what is genuinely unavailable.
    let replica_a = revocation_manager_from_schema_in(&schema, true)
        .await
        .expect("replica A boots against reachable Redis in production mode")
        .expect("token_revocation is enabled, so a manager is built");
    let replica_b = revocation_manager_from_schema_in(&schema, true)
        .await
        .expect("replica B boots against reachable Redis in production mode")
        .expect("token_revocation is enabled, so a manager is built");

    let jti = unique("jti");
    let sub = unique("sub");
    let now = chrono::Utc::now().timestamp();

    // A fresh token passes on both replicas.
    replica_a
        .check_token(Some(&jti), &sub, Some(now))
        .await
        .expect("fresh token passes on replica A");
    replica_b
        .check_token(Some(&jti), &sub, Some(now))
        .await
        .expect("fresh token passes on replica B");

    // Revoke on A → rejected on B. This is the assertion the in-memory downgrade
    // fails: logout on one replica must log the user out everywhere.
    replica_a.revoke(&jti, 3600).await.expect("revoke on replica A");
    let rejection = replica_b
        .check_token(Some(&jti), &sub, Some(now))
        .await
        .expect_err("a token revoked on replica A must be rejected on replica B");
    assert!(
        matches!(rejection, TokenRejection::Revoked),
        "rejection must be Revoked, got {rejection:?}"
    );
}

#[tokio::test]
async fn a_revoke_all_epoch_recorded_on_one_replica_holds_on_every_replica() {
    let Ok(url) = std::env::var("REDIS_URL") else {
        eprintln!(
            "skipping a_revoke_all_epoch_recorded_on_one_replica_holds_on_every_replica: REDIS_URL unset"
        );
        return;
    };

    let schema = schema_with_redis_revocation(&url);
    let replica_a = revocation_manager_from_schema_in(&schema, true)
        .await
        .expect("replica A boots")
        .expect("manager built");
    let replica_b = revocation_manager_from_schema_in(&schema, true)
        .await
        .expect("replica B boots")
        .expect("manager built");

    let sub = unique("sub-all");
    let issued_before_revocation = chrono::Utc::now().timestamp() - 10;

    replica_a.revoke_all_for_user(&sub).await.expect("revoke-all on replica A");

    // A token issued before the epoch is rejected on the other replica even though
    // its jti was never individually revoked.
    let fresh_jti = unique("jti-all");
    let rejection = replica_b
        .check_token(Some(&fresh_jti), &sub, Some(issued_before_revocation))
        .await
        .expect_err("a pre-epoch token must be rejected on replica B after revoke-all on A");
    assert!(
        matches!(rejection, TokenRejection::Revoked),
        "rejection must be Revoked, got {rejection:?}"
    );
}
