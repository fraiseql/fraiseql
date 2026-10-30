//! A session for a user enrolled in `MFA` is minted only through the second factor.
//!
//! Every sign-in path — password, `OTP`, social, `SAML`, the upgrade of an anonymous
//! session — ends in [`PostgresSessionStore::create_session`], so that is where the second
//! factor is required: a path that skipped it would mint a full session from the first
//! factor alone.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns its users' rows by id prefix → run `--test-threads=1`.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{
    MfaStore as _, PgMfaStore, PostgresSessionStore, SessionStore as _, upgrade_anonymous_session,
};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, postgres::PgPoolOptions};

const SECRET: &[u8] = b"session-mfa-test-secret-32-bytes!";

async fn fresh() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    PostgresSessionStore::new(pool.clone()).init().await.unwrap();
    PgMfaStore::new(pool.clone()).init().await.unwrap();
    for table in [
        "core.tb_mfa_challenge",
        "core.tb_mfa_enrollment",
        "_system.sessions",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE user_id LIKE 'mfa-gate-%'"))
            .execute(&pool)
            .await
            .unwrap();
    }
    Some(pool)
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping the MFA session gate test: DATABASE_URL not set");
                return;
            },
        }
    };
}

fn in_an_hour() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

fn totp_code(secret: &str) -> String {
    totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(secret.to_string()).to_bytes().unwrap(),
        None,
        String::new(),
    )
    .unwrap()
    .generate_current()
    .unwrap()
}

/// Enroll `user_id` and confirm the enrollment.
async fn enroll(pool: &PgPool, user_id: &str) {
    let mfa = PgMfaStore::new(pool.clone());
    let enrolled = mfa.begin_enrollment(user_id, "FraiseQL", "u@example.com", None).await.unwrap();
    mfa.confirm_enrollment(user_id, &totp_code(&enrolled.secret_base32))
        .await
        .unwrap();
}

async fn sessions_of(pool: &PgPool, user_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM _system.sessions WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn no_session_is_minted_for_an_enrolled_user_from_the_first_factor() {
    let pool = skip_if_no_db!();
    let user = "mfa-gate-enrolled";
    enroll(&pool, user).await;
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());

    let minted = store.create_session(user, in_an_hour()).await;
    assert!(minted.is_err(), "a session was minted without the second factor");
    assert_eq!(sessions_of(&pool, user).await, 0, "and no session row was written");
}

#[tokio::test]
async fn an_anonymous_session_is_not_upgraded_into_an_enrolled_account() {
    let pool = skip_if_no_db!();
    let user = "mfa-gate-upgraded";
    enroll(&pool, user).await;
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());

    let upgraded = upgrade_anonymous_session(&store, "anon_mfa-gate", user, in_an_hour()).await;
    assert!(upgraded.is_err(), "the upgrade minted a session without the second factor");
    assert_eq!(sessions_of(&pool, user).await, 0);
}

/// The control: a user with no enrollment, or one that was never confirmed, signs in with
/// the first factor alone, as before.
#[tokio::test]
async fn a_user_without_a_confirmed_enrollment_gets_a_session() {
    let pool = skip_if_no_db!();
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());
    store.create_session("mfa-gate-none", in_an_hour()).await.unwrap();

    let pending = "mfa-gate-pending";
    PgMfaStore::new(pool.clone())
        .begin_enrollment(pending, "FraiseQL", "p@example.com", None)
        .await
        .unwrap();
    store.create_session(pending, in_an_hour()).await.unwrap();
}
