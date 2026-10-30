//! A session for a user enrolled in `MFA` is minted only through the second factor.
//!
//! Every sign-in path — password, `OTP`, social, `SAML`, the upgrade of an anonymous
//! session — ends in [`PostgresSessionStore::create_session`], so that is where the second
//! factor is required: a path that skipped it would mint a full session from the first
//! factor alone.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns its users' rows by id prefix → run `--test-threads=1`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{
    AuthError, MfaStore as _, PgMfaStore, PostgresSessionStore, SessionStore as _, TokenPair,
    upgrade_anonymous_session,
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

/// Enroll `user_id` and confirm the enrollment; returns the secret.
async fn enroll(pool: &PgPool, user_id: &str) -> String {
    let mfa = PgMfaStore::new(pool.clone());
    let enrolled = mfa.begin_enrollment(user_id, "FraiseQL", "u@example.com", None).await.unwrap();
    mfa.confirm_enrollment(user_id, &totp_code(&enrolled.secret_base32))
        .await
        .unwrap();
    enrolled.secret_base32
}

/// The challenge a refused session owes, which must redeem into `user_id` with `secret`.
async fn redeem(pool: &PgPool, refused: Result<TokenPair, AuthError>, user_id: &str, secret: &str) {
    let challenge = match refused {
        Err(AuthError::SecondFactorRequired { challenge }) => challenge,
        other => panic!("a session for {user_id} must owe the second factor, got {other:?}"),
    };
    let redeemed = PgMfaStore::new(pool.clone())
        .verify_challenge(challenge.expose(), &totp_code(secret))
        .await
        .unwrap();
    assert_eq!(redeemed, user_id, "the challenge is bound to the user who signed in");
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
    let secret = enroll(&pool, user).await;
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());

    let minted = store.create_session(user, in_an_hour()).await;
    assert_eq!(sessions_of(&pool, user).await, 0, "no session row was written");
    redeem(&pool, minted, user, &secret).await;
}

#[tokio::test]
async fn an_anonymous_session_is_not_upgraded_into_an_enrolled_account() {
    let pool = skip_if_no_db!();
    let user = "mfa-gate-upgraded";
    let secret = enroll(&pool, user).await;
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());

    let upgraded = upgrade_anonymous_session(&store, "anon_mfa-gate", user, in_an_hour()).await;
    assert_eq!(sessions_of(&pool, user).await, 0);
    redeem(&pool, upgraded, user, &secret).await;
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

/// A store that cannot say whether the user is enrolled refuses the sign-in: answering
/// "not enrolled" would skip the second factor. Here the session store's role may read
/// accounts but not enrollments.
#[tokio::test]
async fn a_store_that_cannot_read_enrollments_refuses_the_session() {
    let pool = skip_if_no_db!();
    let user = "mfa-gate-unreadable";
    enroll(&pool, user).await;
    sqlx::raw_sql(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'mfa_gate_noread') THEN
                 CREATE ROLE mfa_gate_noread NOLOGIN;
             END IF;
         END $$;
         GRANT USAGE ON SCHEMA core, _system TO mfa_gate_noread;
         GRANT SELECT, INSERT ON _system.sessions TO mfa_gate_noread;
         REVOKE ALL ON core.tb_mfa_enrollment FROM mfa_gate_noread;
         DO $$ BEGIN
             IF to_regclass('core.tb_user') IS NOT NULL THEN
                 GRANT SELECT ON core.tb_user TO mfa_gate_noread;
             END IF;
         END $$;",
    )
    .execute(&pool)
    .await
    .unwrap();
    let restricted = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE mfa_gate_noread").execute(conn).await.map(|_| ())
            })
        })
        .connect(&try_database_url().unwrap())
        .await
        .unwrap();
    let store = PostgresSessionStore::with_hs256_secret(restricted, SECRET.to_vec());

    match store.create_session(user, in_an_hour()).await {
        Err(AuthError::DatabaseError { message }) => {
            assert!(
                message.contains("tb_mfa_enrollment"),
                "refused on the enrollment read: {message}"
            );
        },
        other => panic!("an unreadable enrollment must refuse the session, got {other:?}"),
    }
    assert_eq!(sessions_of(&pool, user).await, 0);
}

/// The one way past the gate: a redeemed challenge.
#[tokio::test]
async fn a_redeemed_challenge_mints_the_session() {
    let pool = skip_if_no_db!();
    let user = "mfa-gate-passed";
    let secret = enroll(&pool, user).await;
    let sessions =
        std::sync::Arc::new(PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec()));
    let challenge = match sessions.create_session(user, in_an_hour()).await {
        Err(AuthError::SecondFactorRequired { challenge }) => challenge,
        other => panic!("{other:?}"),
    };
    let state = std::sync::Arc::new(fraiseql_auth::MfaRouteState {
        mfa_store:      std::sync::Arc::new(PgMfaStore::new(pool.clone())),
        session_store:  sessions,
        issuer:         "FraiseQL".to_string(),
        verify_limiter: std::sync::Arc::new(fraiseql_auth::KeyedRateLimiter::new(
            fraiseql_auth::AuthRateLimitConfig::per_ip_strict(),
        )),
    });
    let response = fraiseql_auth::mfa_verify(
        axum::extract::State(state),
        axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 1))),
        axum::Json(
            serde_json::from_value(serde_json::json!({
                "challenge_token": challenge.expose(),
                "code": totp_code(&secret),
            }))
            .unwrap(),
        ),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(sessions_of(&pool, user).await, 1, "the redeemed challenge minted one session");
}
