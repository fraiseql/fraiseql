//! Password-reset flow for local email + password accounts (#367).
//!
//! Extends [`LocalPasswordAuthenticator`](super::LocalPasswordAuthenticator) with a
//! single-use, TTL-bounded, non-enumerable password reset on top of #412's Argon2
//! credentials and #411's identity store. Reset tokens live in a new
//! `core.tb_password_reset_token` table mirroring the #411/#412 RLS posture.
//!
//! This ships the reset **primitive** as a library service — no HTTP routes and no
//! concrete SMTP sender — matching #412's service-only precedent. Email delivery is
//! abstracted behind the [`ResetEmailSender`] trait; the server wires a concrete impl.
//!
//! # Token security model
//!
//! - **Selector + verifier.** The token is `b64url(16B selector) "." b64url(32B verifier)`. The
//!   store keeps the `selector` (non-secret, indexed) and `verifier_hash = sha256(verifier)`.
//!   Redemption fetches the row `WHERE selector = $1` — no secret in the `WHERE`, so the lookup is
//!   not an existence oracle — then compares the SHA-256 of the presented verifier against the
//!   stored hash in **constant time** ([`ConstantTimeOps::compare`]). A full database read cannot
//!   forge a usable token: it would require a SHA-256 preimage of a 256-bit CSPRNG verifier.
//!   SHA-256 (not Argon2) is sufficient precisely because the verifier is high-entropy — there is
//!   no brute-force surface that Argon2's cost would defend.
//! - **Single-use.** A `used_at` column is stamped atomically on redemption (`UPDATE … WHERE
//!   used_at IS NULL AND expires_at > now()`); a concurrent second redemption sees zero affected
//!   rows and is rejected. On success the user's *other* outstanding tokens are invalidated too.
//! - **Short TTL.** Tokens expire one hour after issuance ([`RESET_TOKEN_TTL_SECS`]).
//! - **Non-enumerable start.**
//!   [`start_password_reset`](super::LocalPasswordAuthenticator::start_password_reset) always
//!   returns `Ok(())`. The email → credential lookup runs on every path, and the email is
//!   dispatched in a spawned task, so a "no such account" path returns indistinguishably from one
//!   that issued a token. A token is issued only for an email that has a local credential; unknown
//!   / OAuth-only emails are a silent no-op.
//! - **Audit asymmetry.** The caller sees one generic [`AuthError::InvalidToken`] for any
//!   bad/expired/used token; the audit log records the precise reason (`unknown_selector` /
//!   `bad_verifier` / `expired` / `used` / `race`).
//!
//! ## Deferred (named, not unconsidered)
//!
//! - **HTTP endpoints** and a concrete [`ResetEmailSender`] (lettre / bridging the #349 observer
//!   SMTP path) — deferred to the step that wires #412's login/signup routes.
//! - **Rate limiting** on `start` (token-issuance flooding) — the same follow-up as #412's login
//!   rate limiting.
//! - **Residual start timing.** The token `INSERT` runs only on the account-exists path; that
//!   sub-millisecond delta is dwarfed by the spawned email dispatch (equal on both paths) and is an
//!   accepted trade-off, consistent with standard reset designs.

use std::sync::Arc;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::{LOCAL_PROVIDER, LocalPasswordAuthenticator, db_error, validate_password};
use crate::{
    account_linking::normalize_email,
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    constant_time::ConstantTimeOps,
    error::{AuthError, Result},
    session::SessionStore,
};

/// Reset-token lifetime in seconds (1 hour, per #367).
pub const RESET_TOKEN_TTL_SECS: i64 = 3600;

/// Selector length in bytes (the non-secret, indexed lookup key).
const SELECTOR_LEN: usize = 16;
/// Verifier length in bytes (the secret; only its SHA-256 is stored).
const VERIFIER_LEN: usize = 32;

/// Idempotent DDL for the password-reset token store.
///
/// Exposed so a migration runner can apply it explicitly;
/// [`LocalPasswordAuthenticator::init`](super::LocalPasswordAuthenticator::init) runs it
/// after the #411 identity DDL (the table FK-references `core.tb_user`). Mirrors the
/// #411/#412 tables: Trinity `pk_`/`id` columns, deny-by-default RLS (`ENABLE`, not
/// `FORCE`, so the owning store bypasses while any other role reads zero rows without the
/// `fraiseql.tenant_id` GUC), and `REVOKE ALL … FROM PUBLIC`.
pub const PASSWORD_RESET_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_password_reset_token (
    pk_password_reset_token BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id            UUID NOT NULL DEFAULT gen_random_uuid(),
    fk_user       BIGINT NOT NULL REFERENCES core.tb_user (pk_user) ON DELETE CASCADE,
    user_id       TEXT NOT NULL,
    selector      TEXT NOT NULL,
    verifier_hash BYTEA NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ,
    tenant_id     UUID,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (selector)
);
CREATE INDEX IF NOT EXISTS idx_password_reset_token_fk_user
    ON core.tb_password_reset_token (fk_user);

-- RLS deny-by-default (mirrors core.tb_user / core.tb_auth_identity / tb_password_credential).
ALTER TABLE core.tb_password_reset_token ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_password_reset_token_tenant_read ON core.tb_password_reset_token;
CREATE POLICY p_password_reset_token_tenant_read ON core.tb_password_reset_token
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_password_reset_token_insert ON core.tb_password_reset_token;
CREATE POLICY p_password_reset_token_insert ON core.tb_password_reset_token
    FOR INSERT WITH CHECK (true);

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON core.tb_password_reset_token FROM PUBLIC;
";

/// Delivers a password-reset link to a user's email address.
///
/// Defined in `fraiseql-auth` so the reset flow stays transport-agnostic and fully
/// unit-testable; the server provides a concrete implementation (e.g. `lettre`, or
/// bridging the #349 observer SMTP path). The `token` is the full opaque reset token to
/// embed in the link — it is never persisted (only its selector and verifier hash are).
// async_trait: dyn-dispatch required (Arc<dyn ResetEmailSender>); remove when RTN + Send
// is stable (RFC 3425)
#[async_trait]
pub trait ResetEmailSender: Send + Sync {
    /// Send the reset link carrying `token` to `to`.
    ///
    /// # Errors
    ///
    /// Returns an [`AuthError`] if delivery fails. The reset flow dispatches this in a
    /// spawned task and only logs failures, so an error never leaks account existence to
    /// the requester.
    async fn send_reset_link(&self, to: &str, token: &str) -> Result<()>;
}

/// A freshly generated reset token: a non-secret selector plus a secret verifier.
struct ResetToken {
    selector: [u8; SELECTOR_LEN],
    verifier: [u8; VERIFIER_LEN],
}

/// The redemption-relevant parts of a presented token: the selector (for lookup) and the
/// SHA-256 of the verifier (for constant-time comparison against the stored hash).
struct ParsedToken {
    selector:      String,
    verifier_hash: Vec<u8>,
}

impl ResetToken {
    /// Generate a token from the OS-seeded CSPRNG ([`rand::rng`], as used for refresh
    /// tokens).
    fn generate() -> Self {
        use rand::RngCore as _;
        let mut selector = [0u8; SELECTOR_LEN];
        let mut verifier = [0u8; VERIFIER_LEN];
        rand::rng().fill_bytes(&mut selector);
        rand::rng().fill_bytes(&mut verifier);
        Self { selector, verifier }
    }

    /// The base64url selector, stored as the indexed lookup key.
    fn selector_b64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.selector)
    }

    /// SHA-256 of the verifier, the only verifier-derived value persisted.
    fn verifier_hash(&self) -> Vec<u8> {
        Sha256::digest(self.verifier).to_vec()
    }

    /// The opaque token string handed to the user: `selector "." verifier`.
    fn to_token_string(&self) -> String {
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(self.selector),
            URL_SAFE_NO_PAD.encode(self.verifier)
        )
    }

    /// Parse a presented token into its lookup selector and verifier hash.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the token is not `selector.verifier`, either
    /// half is not valid base64url, or either decodes to the wrong length.
    fn parse(token: &str) -> Result<ParsedToken> {
        let (selector_b64, verifier_b64) =
            token.split_once('.').ok_or_else(|| AuthError::InvalidToken {
                reason: "reset token is not in selector.verifier form".to_string(),
            })?;
        let selector =
            URL_SAFE_NO_PAD.decode(selector_b64).map_err(|_| AuthError::InvalidToken {
                reason: "reset token selector is not valid base64url".to_string(),
            })?;
        let verifier =
            URL_SAFE_NO_PAD.decode(verifier_b64).map_err(|_| AuthError::InvalidToken {
                reason: "reset token verifier is not valid base64url".to_string(),
            })?;
        if selector.len() != SELECTOR_LEN || verifier.len() != VERIFIER_LEN {
            return Err(AuthError::InvalidToken {
                reason: "reset token has an unexpected length".to_string(),
            });
        }
        Ok(ParsedToken {
            selector:      selector_b64.to_string(),
            verifier_hash: Sha256::digest(&verifier).to_vec(),
        })
    }
}

/// The generic error returned to the caller for any unredeemable token. The precise
/// reason is recorded in the audit log, never disclosed to the caller.
fn invalid_reset_token() -> AuthError {
    AuthError::InvalidToken {
        reason: "invalid, expired, or already-used password reset token".to_string(),
    }
}

impl LocalPasswordAuthenticator {
    /// Attach the [`ResetEmailSender`] used to deliver reset links.
    ///
    /// Without it, [`start_password_reset`](Self::start_password_reset) still issues and
    /// persists a token but logs a warning instead of delivering it.
    #[must_use]
    pub fn with_email_sender(mut self, sender: Arc<dyn ResetEmailSender>) -> Self {
        self.email_sender = Some(sender);
        self
    }

    /// Attach the session store whose sessions are revoked on a successful reset.
    ///
    /// Without it, [`confirm_password_reset`](Self::confirm_password_reset) changes the
    /// password but logs a warning that outstanding sessions were not revoked.
    #[must_use]
    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    /// Begin a password reset for `email`. Always returns `Ok(())` (non-enumerable).
    ///
    /// Resolves the local credential for `email`; if one exists, issues a single-use,
    /// one-hour token, persists its selector + verifier hash, and dispatches the reset
    /// link via the configured [`ResetEmailSender`] in a spawned task. An unknown or
    /// OAuth-only email is a silent no-op. The return value and timing do not reveal
    /// whether an account exists.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] only if the credential lookup or the token
    /// insert fails — i.e. infrastructure errors, never "account does not exist".
    pub async fn start_password_reset(&self, email: &str) -> Result<()> {
        let normalized = normalize_email(email);
        let logger = get_audit_logger();

        // The lookup runs on every path so a missing account cannot be timed apart.
        let row = sqlx::query(
            "SELECT c.fk_user, c.user_id \
             FROM core.tb_password_credential c \
             JOIN core.tb_auth_identity i ON i.fk_user = c.fk_user \
             WHERE i.provider = $1 AND i.provider_id = $2",
        )
        .bind(LOCAL_PROVIDER)
        .bind(&normalized)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("lookup credential for reset", &e))?;

        let Some(row) = row else {
            // No local credential — silent no-op. Audited, not surfaced.
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "password_reset_start",
                "no_local_credential",
            );
            return Ok(());
        };

        let fk_user: i64 = row.get("fk_user");
        let user_id: String = row.get("user_id");

        let token = ResetToken::generate();
        let expires_at = Utc::now() + Duration::seconds(RESET_TOKEN_TTL_SECS);

        sqlx::query(
            "INSERT INTO core.tb_password_reset_token \
             (fk_user, user_id, selector, verifier_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(fk_user)
        .bind(&user_id)
        .bind(token.selector_b64())
        .bind(token.verifier_hash())
        .bind(expires_at)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("insert reset token", &e))?;

        // Dispatch in a spawned task: the email I/O latency never leaks existence, and a
        // delivery failure is logged rather than surfaced to the requester.
        if let Some(sender) = self.email_sender.clone() {
            let to = normalized;
            let token_str = token.to_token_string();
            tokio::spawn(async move {
                if let Err(e) = sender.send_reset_link(&to, &token_str).await {
                    tracing::warn!("password_reset_start: reset email dispatch failed: {e}");
                }
            });
        } else {
            tracing::warn!(
                "password_reset_start: token issued but no ResetEmailSender is configured; \
                 the reset link was not delivered"
            );
        }

        logger.log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id),
            "password_reset_start",
        );
        Ok(())
    }

    /// Redeem a reset `token` and set `new_password`.
    ///
    /// Validates the new password's length policy, looks the token up by selector,
    /// verifies the verifier in constant time, and rejects it if expired or already used.
    /// On success it sets the new Argon2id hash, marks the token used, invalidates the
    /// user's other outstanding tokens, and revokes the user's sessions (if a session
    /// store is wired) — all in one transaction for the credential changes.
    ///
    /// # Errors
    ///
    /// - [`AuthError::InvalidRegistration`] if `new_password` violates the length policy.
    /// - [`AuthError::InvalidToken`] for any unredeemable token (unknown / malformed / expired /
    ///   used / wrong verifier) — one generic error; the audit log records the precise reason.
    /// - [`AuthError::DatabaseError`] / [`AuthError::Internal`] on a storage failure.
    pub async fn confirm_password_reset(&self, token: &str, new_password: &str) -> Result<()> {
        validate_password(new_password)?;
        let logger = get_audit_logger();

        let Ok(parsed) = ResetToken::parse(token) else {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "password_reset_confirm",
                "malformed_token",
            );
            return Err(invalid_reset_token());
        };

        let row = sqlx::query(
            "SELECT pk_password_reset_token AS pk, fk_user, user_id, verifier_hash, \
                    expires_at, used_at \
             FROM core.tb_password_reset_token WHERE selector = $1",
        )
        .bind(&parsed.selector)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("lookup reset token", &e))?;

        let Some(row) = row else {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "password_reset_confirm",
                "unknown_selector",
            );
            return Err(invalid_reset_token());
        };

        let pk: i64 = row.get("pk");
        let fk_user: i64 = row.get("fk_user");
        let user_id: String = row.get("user_id");
        let stored_hash: Vec<u8> = row.get("verifier_hash");
        let expires_at: DateTime<Utc> = row.get("expires_at");
        let used_at: Option<DateTime<Utc>> = row.get("used_at");

        // Constant-time verifier comparison. The selector is high-entropy and known to the
        // holder, so an early return on a missing row leaks nothing; the secret check is
        // the verifier hash, which is always compared in constant time when a row exists.
        if !ConstantTimeOps::compare(&stored_hash, &parsed.verifier_hash) {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "password_reset_confirm",
                "bad_verifier",
            );
            return Err(invalid_reset_token());
        }

        if used_at.is_some() {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "password_reset_confirm",
                "used",
            );
            return Err(invalid_reset_token());
        }
        if expires_at <= Utc::now() {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "password_reset_confirm",
                "expired",
            );
            return Err(invalid_reset_token());
        }

        let new_hash = self.hash_password(new_password)?;

        let mut tx = self.db.begin().await.map_err(|e| db_error("begin reset transaction", &e))?;

        // Atomic single-use guard: mark THIS token used only if still unused and unexpired.
        // A concurrent redemption that already consumed it affects zero rows -> abort.
        let consumed = sqlx::query(
            "UPDATE core.tb_password_reset_token SET used_at = now() \
             WHERE pk_password_reset_token = $1 AND used_at IS NULL AND expires_at > now()",
        )
        .bind(pk)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("consume reset token", &e))?;

        if consumed.rows_affected() == 0 {
            tx.rollback().await.map_err(|e| db_error("rollback reset transaction", &e))?;
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "password_reset_confirm",
                "race",
            );
            return Err(invalid_reset_token());
        }

        let updated = sqlx::query(
            "UPDATE core.tb_password_credential SET password_hash = $1, updated_at = now() \
             WHERE fk_user = $2",
        )
        .bind(&new_hash)
        .bind(fk_user)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("update credential on reset", &e))?;

        if updated.rows_affected() == 0 {
            tx.rollback().await.map_err(|e| db_error("rollback reset transaction", &e))?;
            return Err(AuthError::Internal {
                message: "reset token resolved to a user with no local credential".to_string(),
            });
        }

        // Invalidate the user's other outstanding tokens (the consumed one is already
        // used_at IS NOT NULL, so this excludes it).
        sqlx::query(
            "UPDATE core.tb_password_reset_token SET used_at = now() \
             WHERE fk_user = $1 AND used_at IS NULL",
        )
        .bind(fk_user)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("invalidate sibling reset tokens", &e))?;

        tx.commit().await.map_err(|e| db_error("commit reset transaction", &e))?;

        // Best-effort session revocation (a different store/schema; outside the tx).
        if let Some(store) = self.session_store.as_ref() {
            if let Err(e) = store.revoke_all_sessions(&user_id).await {
                tracing::warn!(
                    "password_reset_confirm: session revocation failed for {user_id}: {e}"
                );
            }
        } else {
            tracing::warn!(
                "password_reset_confirm: no session store configured; outstanding sessions for \
                 {user_id} were not revoked"
            );
        }

        logger.log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id),
            "password_reset_confirm",
        );
        Ok(())
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
