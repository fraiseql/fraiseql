//! Postgres-backed [`OtpStore`] (#367).
//!
//! `InMemoryOtpStore` keeps codes and the per-email send budget in process
//! memory. Behind more than one replica that is a **silent security downgrade**,
//! not just an inconvenience: the send rate limit and the 3-attempt verify cap
//! are both per-process, so N replicas multiply both budgets by N and a
//! six-digit code becomes brute-forceable. This store makes the budgets shared,
//! which is what lets `[auth.local] otp = true` be served by a real deployment.
//!
//! At rest the code is stored as its SHA-256 hash — a database read cannot
//! replay a live magic link. The comparison stays constant-time on the hash.

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use subtle::ConstantTimeEq as _;

use super::{MAX_VERIFY_ATTEMPTS, OTP_RATE_MAX, OTP_RATE_WINDOW_SECS, OTP_TTL_SECS, OtpStore};
use crate::{
    error::{AuthError, Result},
    session::unix_now,
};

/// DDL for the `OTP` tables. Executed at boot and by the live-PG suite.
pub const PG_OTP_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;
CREATE TABLE IF NOT EXISTS core.tb_otp_code (
    email      TEXT PRIMARY KEY,
    code_hash  BYTEA NOT NULL,
    expires_at BIGINT NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS core.tb_otp_send_budget (
    email        TEXT PRIMARY KEY,
    count        INTEGER NOT NULL DEFAULT 0,
    window_start BIGINT NOT NULL
);
REVOKE ALL ON core.tb_otp_code FROM PUBLIC;
REVOKE ALL ON core.tb_otp_send_budget FROM PUBLIC;
";

fn code_hash(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

fn db_err(e: sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: e.to_string(),
    }
}

/// Postgres-backed `OTP` store.
#[derive(Debug, Clone)]
pub struct PgOtpStore {
    db: PgPool,
}

impl PgOtpStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Execute the table DDL. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if a statement fails.
    pub async fn init(&self) -> Result<()> {
        for stmt in PG_OTP_SCHEMA_SQL.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&self.db).await.map_err(db_err)?;
        }
        Ok(())
    }
}

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl OtpStore for PgOtpStore {
    #[allow(clippy::cast_possible_wrap)] // Reason: expiry is only ever written from unix_now()
    async fn sweep_expired(&self) -> Result<u64> {
        let now = unix_now()? as i64;
        let codes = sqlx::query("DELETE FROM core.tb_otp_code WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.db)
            .await
            .map_err(db_err)?
            .rows_affected();

        // See the in-memory twin: a closed window is dead weight, and this table is the
        // one that otherwise keeps a row for every address that ever asked for a code.
        let window = i64::try_from(crate::otp::OTP_RATE_WINDOW_SECS).unwrap_or(i64::MAX);
        let budgets =
            sqlx::query("DELETE FROM core.tb_otp_send_budget WHERE window_start + $1 <= $2")
                .bind(window)
                .bind(now)
                .execute(&self.db)
                .await
                .map_err(db_err)?
                .rows_affected();

        Ok(codes + budgets)
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // Reason: unix seconds and small counters fit i64/i32
    async fn create_otp(&self, email: &str) -> Result<String> {
        let now = unix_now()?;

        // Reserve one send atomically: the window reset and the increment happen
        // in a single UPSERT, so two concurrent requests cannot both read a
        // stale count and each grant themselves a send.
        let reserved = sqlx::query(
            "INSERT INTO core.tb_otp_send_budget (email, count, window_start)
             VALUES ($1, 1, $2)
             ON CONFLICT (email) DO UPDATE SET
                 count = CASE
                     WHEN $2 - core.tb_otp_send_budget.window_start >= $3 THEN 1
                     ELSE core.tb_otp_send_budget.count + 1 END,
                 window_start = CASE
                     WHEN $2 - core.tb_otp_send_budget.window_start >= $3 THEN $2
                     ELSE core.tb_otp_send_budget.window_start END
             RETURNING count, window_start",
        )
        .bind(email)
        .bind(now as i64)
        .bind(OTP_RATE_WINDOW_SECS as i64)
        .fetch_one(&self.db)
        .await
        .map_err(db_err)?;

        let count: i32 = reserved.get("count");
        let window_start: i64 = reserved.get("window_start");
        if count as u32 > OTP_RATE_MAX {
            return Err(AuthError::RateLimited {
                retry_after_secs: (window_start as u64 + OTP_RATE_WINDOW_SECS).saturating_sub(now),
            });
        }

        // SECURITY: rand::rng() uses OS-level entropy; random_range is unbiased.
        let code = format!("{:06}", rand::Rng::random_range(&mut rand::rng(), 0u32..1_000_000));

        sqlx::query(
            "INSERT INTO core.tb_otp_code (email, code_hash, expires_at, attempts)
             VALUES ($1, $2, $3, 0)
             ON CONFLICT (email) DO UPDATE SET
                 code_hash = EXCLUDED.code_hash,
                 expires_at = EXCLUDED.expires_at,
                 attempts = 0",
        )
        .bind(email)
        .bind(code_hash(&code))
        .bind((now + OTP_TTL_SECS) as i64)
        .execute(&self.db)
        .await
        .map_err(db_err)?;

        Ok(code)
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)] // Reason: expiry/counters are written from unix_now() and small constants
    async fn verify_otp(&self, email: &str, code: &str) -> Result<()> {
        let now = unix_now()?;

        // Charge the attempt first and read the post-increment count, so a
        // concurrent flood cannot each read `attempts = 0` and get a free guess.
        let row = sqlx::query(
            "UPDATE core.tb_otp_code SET attempts = attempts + 1 WHERE email = $1
             RETURNING code_hash, expires_at, attempts",
        )
        .bind(email)
        .fetch_optional(&self.db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AuthError::InvalidToken {
            reason: "no pending OTP for email".into(),
        })?;

        let stored_hash: Vec<u8> = row.get("code_hash");
        let expires_at: i64 = row.get("expires_at");
        let attempts: i32 = row.get("attempts");

        // The DELETE is what makes the code single-use, so its failure must fail
        // the verification (#984): returning Ok after a failed consume reports a
        // guarantee that does not hold and leaves the code replayable until it
        // expires. Propagated on every path — a consume that silently does
        // nothing is the same defect wherever it sits.
        let consume = || async {
            sqlx::query("DELETE FROM core.tb_otp_code WHERE email = $1")
                .bind(email)
                .execute(&self.db)
                .await
                .map_err(db_err)
                .map(|_| ())
        };

        if now >= expires_at as u64 {
            consume().await?;
            return Err(AuthError::InvalidToken {
                reason: "OTP has expired".into(),
            });
        }
        if attempts > MAX_VERIFY_ATTEMPTS as i32 {
            consume().await?;
            return Err(AuthError::RateLimited {
                retry_after_secs: OTP_RATE_WINDOW_SECS,
            });
        }

        // Constant-time comparison on the hashes (#788): a data-dependent
        // comparison leaks how many leading bytes matched via its return time.
        let presented = code_hash(code);
        if !bool::from(presented.ct_eq(&stored_hash)) {
            return Err(AuthError::InvalidToken {
                reason: "invalid OTP code".into(),
            });
        }

        // Correct — consume it (single-use).
        consume().await?;
        Ok(())
    }
}
