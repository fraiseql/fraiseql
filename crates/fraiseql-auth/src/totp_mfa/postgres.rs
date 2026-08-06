//! Postgres-backed [`MfaStore`] (#367).
//!
//! `InMemoryMfaStore` keeps enrollments in process memory, so a restart
//! silently destroys every user's second factor and a second replica does not
//! see enrollments made against the first. Enrollments are **durable account
//! data**, not a short-lived code — mounting the MFA endpoints on a per-process
//! store would lock users out of their own accounts on every deploy. This store
//! is what makes `[auth.local] mfa = true` safe to serve.
//!
//! At rest: the `TOTP` secret is stored base32-encoded (it must be recoverable
//! to verify codes — it is a shared secret, not a password), recovery codes are
//! `bcrypt`-hashed and deleted as they are consumed, and challenge tokens are
//! stored as their SHA-256 hash so a database read cannot replay a live
//! challenge. The per-user failure budget lives in the same table as the
//! enrollment, so it survives restarts too — a brute-force budget that resets
//! on redeploy is not a budget.

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use super::{
    BCRYPT_COST, CHALLENGE_TTL_SECS, EnrollmentResponse, FAILURE_WINDOW_SECS,
    MAX_CHALLENGE_ATTEMPTS, MAX_USER_FAILURES, MfaStore, RECOVERY_CODE_COUNT, build_totp,
    check_recovery_code, generate_challenge_token, generate_recovery_code, verify_totp_code,
};
use crate::{
    error::{AuthError, Result},
    session::unix_now,
};

/// DDL for the `MFA` tables. Executed at boot and by the live-PG suite — which
/// is what keeps it valid PostgreSQL (the `#748` precedent).
pub const PG_MFA_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;
CREATE TABLE IF NOT EXISTS core.tb_mfa_enrollment (
    user_id              TEXT PRIMARY KEY,
    secret_base32        TEXT NOT NULL,
    recovery_code_hashes TEXT[] NOT NULL DEFAULT '{}',
    confirmed            BOOLEAN NOT NULL DEFAULT false,
    failure_count        INTEGER NOT NULL DEFAULT 0,
    failure_window_start BIGINT NOT NULL DEFAULT 0,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS core.tb_mfa_challenge (
    token_hash BYTEA PRIMARY KEY,
    user_id    TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_mfa_challenge_expires ON core.tb_mfa_challenge(expires_at);
REVOKE ALL ON core.tb_mfa_enrollment FROM PUBLIC;
REVOKE ALL ON core.tb_mfa_challenge FROM PUBLIC;
";

/// Hash a challenge token for storage/lookup. Storing the token verbatim would
/// let a database read replay a live challenge.
fn challenge_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn db_err(e: sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: e.to_string(),
    }
}

/// Postgres-backed `TOTP` `MFA` store.
#[derive(Debug, Clone)]
pub struct PgMfaStore {
    db: PgPool,
}

/// A loaded enrollment row.
struct EnrollmentRow {
    secret_base32:        String,
    recovery_code_hashes: Vec<String>,
    confirmed:            bool,
    failure_count:        i32,
    failure_window_start: i64,
}

impl PgMfaStore {
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
        for stmt in PG_MFA_SCHEMA_SQL.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&self.db).await.map_err(db_err)?;
        }
        Ok(())
    }

    async fn load(&self, user_id: &str) -> Result<Option<EnrollmentRow>> {
        let row = sqlx::query(
            "SELECT secret_base32, recovery_code_hashes, confirmed, failure_count, \
             failure_window_start FROM core.tb_mfa_enrollment WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(db_err)?;
        Ok(row.map(|r| EnrollmentRow {
            secret_base32:        r.get("secret_base32"),
            recovery_code_hashes: r.get("recovery_code_hashes"),
            confirmed:            r.get("confirmed"),
            failure_count:        r.get("failure_count"),
            failure_window_start: r.get("failure_window_start"),
        }))
    }

    /// Seconds left in the current lockout, or `None` if not locked out.
    #[allow(clippy::cast_sign_loss)] // Reason: counters are only ever written non-negative
    fn lockout_remaining(row: &EnrollmentRow, now: u64) -> Option<u64> {
        let elapsed = now.saturating_sub(row.failure_window_start as u64);
        (row.failure_count as u32 >= MAX_USER_FAILURES && elapsed < FAILURE_WINDOW_SECS)
            .then(|| FAILURE_WINDOW_SECS - elapsed)
    }

    /// Charge one failed verification, opening a fresh window if the previous
    /// one has elapsed. Done in SQL so concurrent verifies cannot lose a
    /// failure to a read-modify-write race — the whole point of the budget.
    #[allow(clippy::cast_possible_wrap)] // Reason: unix seconds and small counters fit i64/i32
    async fn record_failure(&self, user_id: &str, now: u64) -> Result<()> {
        sqlx::query(
            "UPDATE core.tb_mfa_enrollment
             SET failure_count = CASE
                     WHEN $2 - failure_window_start >= $3 THEN 1
                     ELSE failure_count + 1 END,
                 failure_window_start = CASE
                     WHEN $2 - failure_window_start >= $3 THEN $2
                     ELSE failure_window_start END
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(now as i64)
        .bind(FAILURE_WINDOW_SECS as i64)
        .execute(&self.db)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn clear_failures(&self, user_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE core.tb_mfa_enrollment SET failure_count = 0, failure_window_start = 0 \
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl MfaStore for PgMfaStore {
    async fn begin_enrollment(
        &self,
        user_id: &str,
        issuer: &str,
        account_name: &str,
    ) -> Result<EnrollmentResponse> {
        let secret = totp_rs::Secret::generate_secret();
        let secret_base32 = secret.to_encoded().to_string();
        let totp = build_totp(&secret_base32, Some(issuer), account_name)?;
        let otpauth_uri = totp.get_url();

        let mut recovery_codes_plain = Vec::with_capacity(RECOVERY_CODE_COUNT);
        let mut recovery_code_hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        for _ in 0..RECOVERY_CODE_COUNT {
            let code = generate_recovery_code();
            let hash = bcrypt::hash(&code, BCRYPT_COST).map_err(|e| AuthError::Internal {
                message: format!("bcrypt error: {e}"),
            })?;
            recovery_codes_plain.push(code);
            recovery_code_hashes.push(hash);
        }

        // Re-enrolling replaces the previous (possibly confirmed) enrollment and
        // resets the failure budget — the user proved control of the account to
        // reach this endpoint.
        sqlx::query(
            "INSERT INTO core.tb_mfa_enrollment
                 (user_id, secret_base32, recovery_code_hashes, confirmed,
                  failure_count, failure_window_start)
             VALUES ($1, $2, $3, false, 0, 0)
             ON CONFLICT (user_id) DO UPDATE SET
                 secret_base32 = EXCLUDED.secret_base32,
                 recovery_code_hashes = EXCLUDED.recovery_code_hashes,
                 confirmed = false,
                 failure_count = 0,
                 failure_window_start = 0",
        )
        .bind(user_id)
        .bind(&secret_base32)
        .bind(&recovery_code_hashes)
        .execute(&self.db)
        .await
        .map_err(db_err)?;

        Ok(EnrollmentResponse {
            secret_base32,
            otpauth_uri,
            recovery_codes: recovery_codes_plain,
        })
    }

    async fn confirm_enrollment(&self, user_id: &str, totp_code: &str) -> Result<()> {
        let row = self.load(user_id).await?.ok_or_else(|| AuthError::InvalidToken {
            reason: "no pending MFA enrollment for user".into(),
        })?;
        if !verify_totp_code(&row.secret_base32, totp_code)? {
            return Err(AuthError::InvalidToken {
                reason: "invalid TOTP code".into(),
            });
        }
        sqlx::query("UPDATE core.tb_mfa_enrollment SET confirmed = true WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    #[allow(clippy::cast_possible_wrap)] // Reason: unix seconds fit i64
    async fn create_challenge(&self, user_id: &str) -> Result<String> {
        let now = unix_now()?;
        let row = self.load(user_id).await?.ok_or_else(|| AuthError::InvalidToken {
            reason: "user has no MFA enrollment".into(),
        })?;
        if let Some(retry_after_secs) = Self::lockout_remaining(&row, now) {
            return Err(AuthError::RateLimited { retry_after_secs });
        }
        let token = generate_challenge_token();
        sqlx::query(
            "INSERT INTO core.tb_mfa_challenge (token_hash, user_id, expires_at, attempts)
             VALUES ($1, $2, $3, 0)",
        )
        .bind(challenge_hash(&token))
        .bind(user_id)
        .bind((now + CHALLENGE_TTL_SECS) as i64)
        .execute(&self.db)
        .await
        .map_err(db_err)?;
        Ok(token)
    }

    #[allow(clippy::cast_sign_loss)] // Reason: expiry is only ever written from unix_now()
    async fn verify_challenge(&self, challenge_token: &str, code: &str) -> Result<String> {
        let now = unix_now()?;
        let hash = challenge_hash(challenge_token);

        let challenge = sqlx::query(
            "SELECT user_id, expires_at, attempts FROM core.tb_mfa_challenge WHERE token_hash = $1",
        )
        .bind(&hash)
        .fetch_optional(&self.db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AuthError::InvalidToken {
            reason: "unknown challenge token".into(),
        })?;

        let user_id: String = challenge.get("user_id");
        let expires_at: i64 = challenge.get("expires_at");
        // The DELETE is what makes the challenge single-use, so its failure must
        // fail the verification (#984): the success path charges no attempt, so a
        // swallowed error leaves the token replayable with any current-window code
        // for the rest of CHALLENGE_TTL_SECS. Propagated on every path.
        let consume = |hash: Vec<u8>| async move {
            sqlx::query("DELETE FROM core.tb_mfa_challenge WHERE token_hash = $1")
                .bind(hash)
                .execute(&self.db)
                .await
                .map_err(db_err)
                .map(|_| ())
        };

        if now >= expires_at as u64 {
            consume(hash).await?;
            return Err(AuthError::InvalidToken {
                reason: "challenge token expired".into(),
            });
        }

        let row = self.load(&user_id).await?.ok_or_else(|| AuthError::InvalidToken {
            reason: "user has no MFA enrollment".into(),
        })?;
        if let Some(retry_after_secs) = Self::lockout_remaining(&row, now) {
            consume(hash).await?;
            return Err(AuthError::RateLimited { retry_after_secs });
        }
        if !row.confirmed {
            return Err(AuthError::InvalidToken {
                reason: "MFA enrollment not confirmed".into(),
            });
        }

        if verify_totp_code(&row.secret_base32, code)? {
            consume(hash).await?;
            self.clear_failures(&user_id).await?;
            return Ok(user_id);
        }

        // Recovery codes (bcrypt, slow — intentional). A consumed code is
        // removed from the array so it cannot be replayed.
        if let Some(i) = check_recovery_code(code, &row.recovery_code_hashes) {
            let mut remaining = row.recovery_code_hashes;
            remaining.remove(i);
            sqlx::query(
                "UPDATE core.tb_mfa_enrollment SET recovery_code_hashes = $2 WHERE user_id = $1",
            )
            .bind(&user_id)
            .bind(&remaining)
            .execute(&self.db)
            .await
            .map_err(db_err)?;
            consume(hash).await?;
            self.clear_failures(&user_id).await?;
            return Ok(user_id);
        }

        // Wrong code: charge the per-user budget and burn one challenge attempt,
        // consuming the challenge once they are exhausted.
        self.record_failure(&user_id, now).await?;
        let attempts = sqlx::query(
            "UPDATE core.tb_mfa_challenge SET attempts = attempts + 1 WHERE token_hash = $1
             RETURNING attempts",
        )
        .bind(&hash)
        .fetch_optional(&self.db)
        .await
        .map_err(db_err)?
        .map_or(i32::MAX, |r| r.get::<i32, _>("attempts"));
        #[allow(clippy::cast_possible_wrap)] // Reason: MAX_CHALLENGE_ATTEMPTS is a small constant
        if attempts >= MAX_CHALLENGE_ATTEMPTS as i32 {
            consume(hash).await?;
        }

        Err(AuthError::InvalidToken {
            reason: "invalid TOTP or recovery code".into(),
        })
    }

    async fn unenroll(&self, user_id: &str, code: &str) -> Result<()> {
        let now = unix_now()?;
        let row = self.load(user_id).await?.ok_or_else(|| AuthError::InvalidToken {
            reason: "user has no MFA enrollment".into(),
        })?;
        // unenroll accepts the same secrets as verify_challenge, so it is the
        // same brute-force surface and shares the same budget.
        if let Some(retry_after_secs) = Self::lockout_remaining(&row, now) {
            return Err(AuthError::RateLimited { retry_after_secs });
        }
        if !row.confirmed {
            return Err(AuthError::InvalidToken {
                reason: "MFA enrollment not confirmed".into(),
            });
        }

        let totp_ok = verify_totp_code(&row.secret_base32, code)?;
        let recovery_ok =
            !totp_ok && check_recovery_code(code, &row.recovery_code_hashes).is_some();
        if !totp_ok && !recovery_ok {
            self.record_failure(user_id, now).await?;
            return Err(AuthError::InvalidToken {
                reason: "re-authentication failed — invalid TOTP or recovery code".into(),
            });
        }

        sqlx::query("DELETE FROM core.tb_mfa_enrollment WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(db_err)?;
        sqlx::query("DELETE FROM core.tb_mfa_challenge WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn is_enrolled(&self, user_id: &str) -> bool {
        // Fail-closed on a database error: reporting "not enrolled" would let a
        // caller skip the second factor, so an unreachable store must read as
        // enrolled-unknown → the challenge endpoint 404s rather than bypassing.
        (self.load(user_id).await).is_ok_and(|row| row.is_some_and(|r| r.confirmed))
    }
}
