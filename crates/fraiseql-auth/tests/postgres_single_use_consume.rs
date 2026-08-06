//! Live-PostgreSQL tests for the single-use consume of OTP codes and MFA
//! challenge tokens (#984).
//!
//! Both [`PgOtpStore::verify_otp`] and [`PgMfaStore::verify_challenge`] establish
//! their single-use guarantee with a `DELETE` on the success path. If that
//! `DELETE` fails and the function still returns `Ok`, the credential stays
//! replayable while the caller is told the guarantee held — the program's
//! fabricated-success shape.
//!
//! The issue notes that no test can inject a failure into *only* the `DELETE`.
//! It can, with no production seam: a `BEFORE DELETE` trigger that
//! `RAISE EXCEPTION`s leaves `SELECT`/`INSERT`/`UPDATE` on the same table
//! working, and — unlike `REVOKE` — still bites the rig's `rolsuper` role.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: postgres`
//! suite, which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** installs table-scoped triggers and truncates the shared
//! `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{MfaStore, OtpStore, PgMfaStore, PgOtpStore};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use totp_rs::{Algorithm, Secret, TOTP};

/// Name of the `BEFORE DELETE` trigger this suite installs. One name per table,
/// dropped on every setup so a panicking test cannot poison the next one.
const POISON_TRIGGER: &str = "trg_fraiseql_test_poison_delete";

/// Connect, apply both stores' DDL, drop any leaked poison triggers, and clear
/// the tables. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<(PgOtpStore, PgMfaStore, PgPool)> {
    let url = try_database_url()?;
    let db = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();

    let otp = PgOtpStore::new(db.clone());
    otp.init().await.unwrap();
    let mfa = PgMfaStore::new(db.clone());
    mfa.init().await.unwrap();

    for table in [
        "core.tb_otp_code",
        "core.tb_mfa_challenge",
        "core.tb_mfa_enrollment",
    ] {
        unpoison_deletes(&db, table).await;
    }
    sqlx::query(
        "TRUNCATE core.tb_otp_code, core.tb_otp_send_budget,
                  core.tb_mfa_challenge, core.tb_mfa_enrollment",
    )
    .execute(&db)
    .await
    .unwrap();

    Some((otp, mfa, db))
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(triple) => triple,
            None => {
                eprintln!("skipping #984 single-use consume test: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// Make every `DELETE` against `table` fail, and nothing else. This is the
/// transient database failure the issue describes (statement timeout, failover,
/// connection drop) narrowed to the one statement under test.
async fn poison_deletes(db: &PgPool, table: &str) {
    sqlx::query(
        "CREATE OR REPLACE FUNCTION core.fn_fraiseql_test_poison_delete() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'poisoned DELETE (test injection)'; END $$",
    )
    .execute(db)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER {POISON_TRIGGER} BEFORE DELETE ON {table}
         FOR EACH ROW EXECUTE FUNCTION core.fn_fraiseql_test_poison_delete()"
    ))
    .execute(db)
    .await
    .unwrap();
}

async fn unpoison_deletes(db: &PgPool, table: &str) {
    sqlx::query(&format!("DROP TRIGGER IF EXISTS {POISON_TRIGGER} ON {table}"))
        .execute(db)
        .await
        .unwrap();
}

async fn row_count(db: &PgPool, table: &str) -> i64 {
    sqlx::query(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(db)
        .await
        .unwrap()
        .get(0)
}

/// Current TOTP code for a base32 secret, matching `build_totp`'s parameters
/// (SHA-1, 6 digits, 30s step).
fn totp_now(secret_base32: &str) -> String {
    let bytes = Secret::Encoded(secret_base32.to_string()).to_bytes().unwrap();
    TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new())
        .unwrap()
        .generate_current()
        .unwrap()
}

// ── OTP — otp/postgres.rs verify_otp ──────────────────────────────────────────

#[tokio::test]
async fn otp_verify_fails_closed_when_the_single_use_delete_fails() {
    let (otp, _mfa, db) = skip_if_no_db!();
    let email = "consume-otp@example.com";
    let code = otp.create_otp(email).await.unwrap();

    poison_deletes(&db, "core.tb_otp_code").await;
    let result = otp.verify_otp(email, &code).await;
    unpoison_deletes(&db, "core.tb_otp_code").await;

    assert!(
        result.is_err(),
        "#984: the DELETE that makes the OTP single-use failed, so the code is still \
         replayable — verify_otp must fail closed, not report a successful verification"
    );
    assert_eq!(
        row_count(&db, "core.tb_otp_code").await,
        1,
        "precondition: the poisoned DELETE really did leave the row behind"
    );
}

#[tokio::test]
async fn otp_verify_consumes_the_code_when_the_delete_succeeds() {
    let (otp, _mfa, db) = skip_if_no_db!();
    let email = "happy-otp@example.com";
    let code = otp.create_otp(email).await.unwrap();

    otp.verify_otp(email, &code).await.unwrap();
    assert_eq!(row_count(&db, "core.tb_otp_code").await, 0, "the code is consumed");
    assert!(
        otp.verify_otp(email, &code).await.is_err(),
        "single-use: the same code must not verify twice"
    );
}

// ── MFA — totp_mfa/postgres.rs verify_challenge ───────────────────────────────

/// Enroll a user and confirm the enrollment through the real API.
async fn enrolled(mfa: &PgMfaStore, user_id: &str) -> String {
    let enrollment = mfa.begin_enrollment(user_id, "fraiseql", user_id).await.unwrap();
    let secret = enrollment.secret_base32;
    mfa.confirm_enrollment(user_id, &totp_now(&secret)).await.unwrap();
    secret
}

#[tokio::test]
async fn mfa_verify_fails_closed_when_the_challenge_delete_fails() {
    let (_otp, mfa, db) = skip_if_no_db!();
    let user_id = "user_consume_mfa";
    let secret = enrolled(&mfa, user_id).await;
    let challenge = mfa.create_challenge(user_id).await.unwrap();

    poison_deletes(&db, "core.tb_mfa_challenge").await;
    let result = mfa.verify_challenge(&challenge, &totp_now(&secret)).await;
    unpoison_deletes(&db, "core.tb_mfa_challenge").await;

    assert!(
        result.is_err(),
        "#984: the DELETE that consumes the challenge failed, so the challenge token stays \
         valid for the rest of its TTL — verify_challenge must fail closed"
    );
    assert_eq!(
        row_count(&db, "core.tb_mfa_challenge").await,
        1,
        "precondition: the poisoned DELETE really did leave the challenge behind"
    );
}

#[tokio::test]
async fn mfa_verify_consumes_the_challenge_when_the_delete_succeeds() {
    let (_otp, mfa, db) = skip_if_no_db!();
    let user_id = "user_happy_mfa";
    let secret = enrolled(&mfa, user_id).await;
    let challenge = mfa.create_challenge(user_id).await.unwrap();

    let verified = mfa.verify_challenge(&challenge, &totp_now(&secret)).await.unwrap();
    assert_eq!(verified, user_id);
    assert_eq!(row_count(&db, "core.tb_mfa_challenge").await, 0, "the challenge is consumed");
    assert!(
        mfa.verify_challenge(&challenge, &totp_now(&secret)).await.is_err(),
        "single-use: the same challenge token must not verify twice"
    );
}
