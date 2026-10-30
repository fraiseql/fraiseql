//! Live-PostgreSQL tests for the per-address `OTP` guess budget (#1857).
//!
//! The verify cap used to live on the code row, and every send reset it: three
//! sends a window bought nine guesses at a six-digit code, while the module doc
//! promised three. The budget now lives on the address's send-budget row, so a
//! resend issues a new code without issuing new guesses.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: postgres`
//! suite, which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` `OTP` tables on setup → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{AuthError, OtpStore, PgOtpStore};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// Connect, apply the store's DDL and clear its tables. `None` (skip) when unconfigured.
async fn fresh() -> Option<(PgOtpStore, PgPool)> {
    let url = try_database_url()?;
    let db = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let otp = PgOtpStore::new(db.clone());
    otp.init().await.unwrap();
    sqlx::query("TRUNCATE core.tb_otp_code, core.tb_otp_send_budget")
        .execute(&db)
        .await
        .unwrap();
    Some((otp, db))
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(pair) => pair,
            None => {
                eprintln!("skipping #1857 OTP guess-budget test: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// Spend the address's whole guess budget on wrong codes.
async fn spend_every_guess(otp: &PgOtpStore, email: &str) {
    for _ in 0..3 {
        let wrong = otp.verify_otp(email, "not-a-code").await.unwrap_err();
        assert!(matches!(wrong, AuthError::InvalidToken { .. }), "got {wrong:?}");
    }
}

#[tokio::test]
async fn a_resend_does_not_buy_more_guesses_inside_the_window() {
    let (otp, _db) = skip_if_no_db!();
    let email = "victim@example.com";

    otp.create_otp(email).await.unwrap();
    spend_every_guess(&otp, email).await;

    // The right code, freshly sent: refused, because the guesses are the address's.
    let code = otp.create_otp(email).await.unwrap();
    let fourth = otp.verify_otp(email, &code).await.unwrap_err();
    assert!(matches!(fourth, AuthError::RateLimited { .. }), "got {fourth:?}");
}

#[tokio::test]
async fn the_guess_budget_returns_with_the_next_window() {
    let (otp, db) = skip_if_no_db!();
    let email = "returning@example.com";

    otp.create_otp(email).await.unwrap();
    spend_every_guess(&otp, email).await;

    // Close the window as time would: move its start back past the 15-minute width.
    sqlx::query(
        "UPDATE core.tb_otp_send_budget SET window_start = window_start - 900 WHERE email = $1",
    )
    .bind(email)
    .execute(&db)
    .await
    .unwrap();

    let code = otp.create_otp(email).await.unwrap();
    otp.verify_otp(email, &code).await.unwrap();
}

#[tokio::test]
async fn one_address_spending_its_guesses_leaves_another_untouched() {
    let (otp, _db) = skip_if_no_db!();

    otp.create_otp("spent@example.com").await.unwrap();
    spend_every_guess(&otp, "spent@example.com").await;

    let code = otp.create_otp("other@example.com").await.unwrap();
    otp.verify_otp("other@example.com", &code).await.unwrap();
}

/// A database provisioned before #1857 has the budget table without the guess
/// counter; `init` must bring it forward rather than leave verify failing on it.
#[tokio::test]
async fn init_adds_the_guess_counter_to_an_existing_budget_table() {
    let (otp, db) = skip_if_no_db!();
    sqlx::query("ALTER TABLE core.tb_otp_send_budget DROP COLUMN IF EXISTS guesses")
        .execute(&db)
        .await
        .unwrap();

    otp.init().await.unwrap();

    let code = otp.create_otp("upgraded@example.com").await.unwrap();
    otp.verify_otp("upgraded@example.com", &code).await.unwrap();
}

/// A code sent late in a window outlives it (the code lives 10 minutes, the window
/// 15). Verifying it after the window closes opens the next window, so the guesses
/// spent in the old one no longer count against it.
#[tokio::test]
async fn a_code_verified_after_its_window_closed_starts_the_next_one() {
    let (otp, db) = skip_if_no_db!();
    let email = "late@example.com";

    otp.create_otp(email).await.unwrap();
    spend_every_guess(&otp, email).await;
    let code = otp.create_otp(email).await.unwrap();

    sqlx::query(
        "UPDATE core.tb_otp_send_budget SET window_start = window_start - 900 WHERE email = $1",
    )
    .bind(email)
    .execute(&db)
    .await
    .unwrap();

    otp.verify_otp(email, &code).await.unwrap();
}
