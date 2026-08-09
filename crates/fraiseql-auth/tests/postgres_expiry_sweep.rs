//! Live-PostgreSQL tests for the MFA/OTP expiry sweeps (#950).
//!
//! Every read of these tables checks expiry, so correctness never depended on a sweep —
//! which is exactly why none existed. What did depend on it is table size: a challenge
//! that is minted and simply abandoned is never consumed, never observed expired, and
//! never deleted, so `core.tb_mfa_challenge` and `core.tb_otp_code` grow for the life of
//! the deployment. `core.tb_otp_send_budget` is worse: one row per address that has ever
//! requested a code, removed by nothing at all.
//!
//! The assertions below are deliberately two-sided. A sweep that deletes everything would
//! satisfy "the expired row is gone" while destroying live credentials, so each test also
//! pins that an unexpired row survives.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{MfaStore, OtpStore, PgMfaStore, PgOtpStore};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

/// Connect, apply both stores' DDL, and clear the tables. `None` (skip) when unconfigured.
async fn fresh() -> Option<(PgOtpStore, PgMfaStore, PgPool)> {
    let url = try_database_url()?;
    let db = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();

    let otp = PgOtpStore::new(db.clone());
    otp.init().await.unwrap();
    let mfa = PgMfaStore::new(db.clone());
    mfa.init().await.unwrap();

    sqlx::query(
        "TRUNCATE core.tb_otp_code, core.tb_otp_send_budget,
                  core.tb_mfa_challenge, core.tb_mfa_enrollment",
    )
    .execute(&db)
    .await
    .unwrap();

    Some((otp, mfa, db))
}

fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

async fn count(db: &PgPool, table: &str) -> i64 {
    sqlx::query(&format!("SELECT count(*) AS n FROM {table}"))
        .fetch_one(db)
        .await
        .unwrap()
        .get::<i64, _>("n")
}

#[tokio::test]
async fn mfa_sweep_removes_abandoned_challenges_and_keeps_live_ones() {
    let Some((_, mfa, db)) = fresh().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    sqlx::query(
        "INSERT INTO core.tb_mfa_challenge (token_hash, user_id, expires_at)
         VALUES ($1, 'abandoned', $2), ($3, 'live', $4)",
    )
    .bind(vec![1_u8; 32])
    .bind(now() - 60)
    .bind(vec![2_u8; 32])
    .bind(now() + 3600)
    .execute(&db)
    .await
    .unwrap();

    let removed = mfa.sweep_expired().await.unwrap();

    assert_eq!(removed, 1, "exactly the expired challenge is swept");
    let survivors: Vec<String> =
        sqlx::query("SELECT user_id FROM core.tb_mfa_challenge ORDER BY user_id")
            .fetch_all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<String, _>("user_id"))
            .collect();
    assert_eq!(survivors, vec!["live".to_string()], "a live challenge must survive the sweep");
}

#[tokio::test]
async fn otp_sweep_removes_expired_codes_and_stale_budgets() {
    let Some((otp, _, db)) = fresh().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    sqlx::query(
        "INSERT INTO core.tb_otp_code (email, code_hash, expires_at)
         VALUES ('stale@example.com', $1, $2), ('live@example.com', $3, $4)",
    )
    .bind(vec![1_u8; 32])
    .bind(now() - 60)
    .bind(vec![2_u8; 32])
    .bind(now() + 3600)
    .execute(&db)
    .await
    .unwrap();

    // The budget window is 15 minutes (`OTP_RATE_WINDOW_SECS`); a row whose window closed
    // long ago is dead weight — the next request for that address opens a fresh window.
    sqlx::query(
        "INSERT INTO core.tb_otp_send_budget (email, count, window_start)
         VALUES ('ancient@example.com', 3, $1), ('current@example.com', 1, $2)",
    )
    .bind(now() - 86_400)
    .bind(now())
    .execute(&db)
    .await
    .unwrap();

    let removed = otp.sweep_expired().await.unwrap();

    assert_eq!(removed, 2, "one expired code and one ancient budget row");
    assert_eq!(count(&db, "core.tb_otp_code").await, 1, "the live code survives");
    assert_eq!(count(&db, "core.tb_otp_send_budget").await, 1, "the current budget survives");

    let live: String = sqlx::query("SELECT email FROM core.tb_otp_code")
        .fetch_one(&db)
        .await
        .unwrap()
        .get("email");
    assert_eq!(live, "live@example.com");
}

#[tokio::test]
async fn sweeping_an_empty_table_is_a_no_op() {
    let Some((otp, mfa, _)) = fresh().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    assert_eq!(mfa.sweep_expired().await.unwrap(), 0);
    assert_eq!(otp.sweep_expired().await.unwrap(), 0);
}
