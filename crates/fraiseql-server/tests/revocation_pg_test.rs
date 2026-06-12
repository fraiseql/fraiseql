//! Integration test for the PostgreSQL token-revocation store (#357).
//!
//! Proves the Postgres backend actually persists and checks revoked `jti`s (the
//! server previously silently downgraded `backend = "postgres"` to in-memory).
//! Requires PostgreSQL (`DATABASE_URL`); skips gracefully when unset.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stderr)] // Reason: test code.

use fraiseql_server::token_revocation::{PostgresRevocationStore, RevocationStore};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn postgres_revocation_store_persists_and_checks_jtis() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!(
            "skipping postgres_revocation_store_persists_and_checks_jtis: DATABASE_URL unset"
        );
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to the warm test PostgreSQL");

    // `new` ensures the backing table exists (idempotent DDL).
    let store = PostgresRevocationStore::new(pool)
        .await
        .expect("PostgresRevocationStore schema creation should succeed");

    // Unique jti so repeated runs against the shared test DB don't collide.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let jti = format!("test-357-{nanos}");

    // A never-seen jti is not revoked.
    assert!(!store.is_revoked(&jti).await.unwrap(), "fresh jti must not be revoked");

    // After revoke with a long TTL it is revoked …
    store.revoke(&jti, 3600).await.unwrap();
    assert!(store.is_revoked(&jti).await.unwrap(), "revoked jti must report revoked");

    // … and revoking again is idempotent (ON CONFLICT), still revoked.
    store.revoke(&jti, 3600).await.unwrap();
    assert!(store.is_revoked(&jti).await.unwrap(), "re-revoke is idempotent");

    // revoke_all_for_user records a per-user epoch (M-revoke-all). A user with no
    // revoke-all has no epoch …
    let sub = format!("user-357-{nanos}");
    assert!(
        store.user_revoked_after(&sub).await.unwrap().is_none(),
        "a user with no revoke-all must have no epoch"
    );

    // … after revoke-all with a long TTL the epoch is present (a positive unix second) …
    store.revoke_all_for_user(&sub, 3600).await.unwrap();
    let epoch = store
        .user_revoked_after(&sub)
        .await
        .unwrap()
        .expect("revoke-all must persist an epoch");
    assert!(epoch > 1_577_836_800, "epoch must be a real unix timestamp (after 2020)");

    // … and a zero-TTL epoch is already expired, so it is not returned.
    let expired_sub = format!("expired-357-{nanos}");
    store.revoke_all_for_user(&expired_sub, 0).await.unwrap();
    assert!(
        store.user_revoked_after(&expired_sub).await.unwrap().is_none(),
        "a zero-ttl epoch must be treated as expired"
    );

    // Housekeeping call must succeed (the long-TTL rows are not yet expired).
    store.cleanup_expired().await.unwrap();
}
