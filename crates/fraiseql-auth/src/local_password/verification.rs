//! Email verification for local email + password accounts (#945).
//!
//! Before this, `FraiseQL` could only *consume* a verification claim someone else
//! asserted — a social provider's `email_verified`, gated by
//! [`TrustedEmailProviders`](crate::TrustedEmailProviders), or a completed email `OTP`.
//! A local-password signup passes `email_verified = false` (deliberately fail-closed),
//! which keys the identity on `(local, <email>)` and leaves `core.tb_user.email` `NULL`
//! forever: the account could never *become* verified, so the same person's password and
//! Google sign-ins stayed two accounts with no way to join them.
//!
//! This module issues the claim. A token is mailed to the address the account claims;
//! presenting it back proves control of that mailbox, which promotes the account's own
//! row into the merge-able `email:<normalized>` key space. A later trusted social
//! sign-in for the same address then links into it through the ordinary
//! [`AccountStore::link_or_create_user`](crate::AccountStore::link_or_create_user) path
//! — no new merge machinery, and the one account the user expected.
//!
//! # Security model
//!
//! - **Token discipline** — the same selector + verifier codec password reset uses: plaintext
//!   selector indexed, `sha256(verifier)` stored, constant-time comparison, single-use `used_at`
//!   stamped atomically, one-hour TTL ([`EMAIL_VERIFICATION_TOKEN_TTL_SECS`]). The token row also
//!   stores the **address it was mailed to**, so confirmation promotes exactly the address whose
//!   mailbox was proved, never one re-derived at redemption time.
//! - **Both halves are required.** `confirm` needs the token *and* an authenticated caller, and the
//!   token's subject must equal the caller's `user_id`
//!   ([`LocalPasswordAuthenticator::confirm_email_verification`]). The token alone proves mailbox
//!   control; the session proves account ownership. This is what defeats the confused-deputy shape:
//!   an attacker who signs up a local account under a victim's address causes a verification mail
//!   to land in the *victim's* inbox, and a victim who clicks it is not authenticated as the
//!   attacker, so the confirmation fails closed.
//! - **Promotion never absorbs another account** — see [`decide_promotion`], which is the gate.
//!
//! # Why there is no merge-into-an-existing-account path
//!
//! The obvious extension is: on confirm, if some *other* account already holds this
//! verified email, merge the two. That is refused, permanently and loudly
//! ([`AuthError::EmailClaimedByAnotherAccount`]), because the merge would move the local
//! account's password credential — chosen by whoever ran signup — onto an account it did
//! not previously reach. Signup for an arbitrary address is open by design (it keys on
//! `(local, email)` and is harmless precisely *because* it can never merge), so the merge
//! would complete this chain: sign up under `victim@example.com`, obtain the mailed code
//! once, and inherit the victim's existing Google-keyed account together with everything
//! on it.
//!
//! The promotion this module *does* perform has no such reach: it writes one address onto
//! the caller's own user row and moves no credential anywhere. The ordering the issue
//! actually describes — password signup first, social sign-in later — is served by it
//! exactly. The reverse ordering (social first, password second) is left to the safe
//! direction, which is to add a password from inside an authenticated session rather than
//! to pull an account toward an outside credential.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;

use super::{LOCAL_PROVIDER, LocalPasswordAuthenticator, db_error, opaque_token::OpaqueToken};
use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    constant_time::ConstantTimeOps,
    error::{AuthError, Result},
};

/// Verification-token lifetime in seconds (1 hour).
///
/// The same bound as [`RESET_TOKEN_TTL_SECS`](super::RESET_TOKEN_TTL_SECS), and for the
/// same reason: redemption performs an account-state change, so the window in which a
/// leaked mail is actionable stays short. Re-issuing is a single authenticated call, so
/// the strict bound costs a user nothing but a second click.
pub const EMAIL_VERIFICATION_TOKEN_TTL_SECS: i64 = 3600;

/// Flow label carried into [`OpaqueToken::parse`]'s diagnostic reasons.
const TOKEN_KIND: &str = "email verification";

/// Idempotent DDL for the email-verification token store.
///
/// Exposed so a migration runner can apply it explicitly;
/// [`LocalPasswordAuthenticator::init`](super::LocalPasswordAuthenticator::init) runs it
/// after the #411 identity DDL (the table FK-references `core.tb_user`). Mirrors
/// `core.tb_password_reset_token`: Trinity `pk_`/`id` columns, deny-by-default RLS
/// (`ENABLE`, not `FORCE`, so the owning store bypasses while any other role reads zero
/// rows without the `fraiseql.tenant_id` GUC), and `REVOKE ALL … FROM PUBLIC`.
///
/// The one column the reset table does not have is `email`: the address the token was
/// mailed to. Storing it binds the proof to a specific mailbox rather than to whatever
/// the identity happens to say at redemption time.
pub const EMAIL_VERIFICATION_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_email_verification_token (
    pk_email_verification_token BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id            UUID NOT NULL DEFAULT gen_random_uuid(),
    fk_user       BIGINT NOT NULL REFERENCES core.tb_user (pk_user) ON DELETE CASCADE,
    user_id       TEXT NOT NULL,
    email         TEXT NOT NULL,
    selector      TEXT NOT NULL,
    verifier_hash BYTEA NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ,
    tenant_id     UUID,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (selector)
);
CREATE INDEX IF NOT EXISTS idx_email_verification_token_fk_user
    ON core.tb_email_verification_token (fk_user);

-- RLS deny-by-default (mirrors core.tb_user / core.tb_auth_identity / tb_password_reset_token).
ALTER TABLE core.tb_email_verification_token ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_email_verification_token_tenant_read ON core.tb_email_verification_token;
CREATE POLICY p_email_verification_token_tenant_read ON core.tb_email_verification_token
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_email_verification_token_insert ON core.tb_email_verification_token;
CREATE POLICY p_email_verification_token_insert ON core.tb_email_verification_token
    FOR INSERT WITH CHECK (true);

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON core.tb_email_verification_token FROM PUBLIC;
";

/// Delivers an email-verification link to the address being verified.
///
/// Defined in `fraiseql-auth` so the flow stays transport-agnostic and fully
/// unit-testable; the server provides the concrete implementation over the
/// `[auth.local] email_from` mailbox. The `token` is the full opaque token to embed in
/// the link — it is never persisted (only its selector and verifier hash are).
// async_trait: dyn-dispatch required (Arc<dyn VerificationEmailSender>); remove when
// RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait VerificationEmailSender: Send + Sync {
    /// Send the verification link carrying `token` to `to`.
    ///
    /// # Errors
    ///
    /// Returns an [`AuthError`] if delivery fails. The flow dispatches this in a spawned
    /// task and only logs failures, so a bad relay never turns into a caller-visible
    /// signal about account state.
    async fn send_verification_link(&self, to: &str, token: &str) -> Result<()>;
}

/// What [`LocalPasswordAuthenticator::confirm_email_verification`] settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailVerified {
    /// The account whose row now carries the verified address. Always the caller's own
    /// `user_id` — this flow never returns a different account than the one that asked.
    pub user_id: String,
    /// The normalized address that is now verified.
    pub email:   String,
}

/// The outcome of the promotion gate — what confirming a proved mailbox may do to the
/// account's `core.tb_user.email`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionDecision {
    /// The address is unclaimed and the account has no verified address yet: write it.
    /// The account enters the merge-able `email:<normalized>` key space, where a later
    /// trusted social sign-in for the same address links into it.
    Promote,
    /// The account already carries exactly this address. Confirming again is a no-op
    /// success, so a double-clicked link does not read as a failure.
    AlreadyVerified,
    /// **The security refusal.** Another account already holds this verified address, so
    /// promoting would put two accounts in one key slot — and the only way to resolve
    /// that is a merge, which would carry this account's password credential onto the
    /// other one. See the module docs for why that chain is refused rather than gated.
    RefuseClaimedByAnotherAccount,
    /// The account already has a *different* verified address. A verified address is the
    /// account's linking key, and this flow does not re-key an account: doing so would
    /// silently move it out of the key space a previous trusted sign-in placed it in.
    RefuseAccountHasDifferentEmail,
}

/// The promotion gate: given the account's current verified address, the address whose
/// mailbox was just proved, and whether any *other* account already holds that address,
/// decide what may be written.
///
/// This is a pure function on purpose — it is the security-relevant decision of the whole
/// flow, so it is stated once, unit-tested exhaustively, and consulted by the one caller
/// rather than being spread across SQL branches. It mirrors
/// [`effective_saml_email_verified`](crate::saml::effective_saml_email_verified), which
/// does the same job for the SAML path.
///
/// The refusal is checked **first**: a claimed address is refused whatever state the
/// caller's own account is in.
#[must_use]
pub fn decide_promotion(
    account_email: Option<&str>,
    proved_email: &str,
    claimed_by_other_account: bool,
) -> PromotionDecision {
    if claimed_by_other_account {
        return PromotionDecision::RefuseClaimedByAnotherAccount;
    }
    match account_email {
        None => PromotionDecision::Promote,
        Some(existing) if existing == proved_email => PromotionDecision::AlreadyVerified,
        Some(_) => PromotionDecision::RefuseAccountHasDifferentEmail,
    }
}

/// The generic error returned to the caller for any unredeemable token. The precise
/// reason is recorded in the audit log, never disclosed to the caller.
fn invalid_verification_token() -> AuthError {
    AuthError::InvalidToken {
        reason: "invalid, expired, or already-used email verification token".to_string(),
    }
}

impl LocalPasswordAuthenticator {
    /// Attach the [`VerificationEmailSender`] used to deliver verification links.
    ///
    /// Without it, [`start_email_verification`](Self::start_email_verification) still
    /// issues and persists a token but logs a warning instead of delivering it.
    #[must_use]
    pub fn with_verification_email_sender(
        mut self,
        sender: Arc<dyn VerificationEmailSender>,
    ) -> Self {
        self.verification_sender = Some(sender);
        self
    }

    /// Begin email verification for the authenticated account `user_id`.
    ///
    /// The address is taken from the account's **own local identity**, never from the
    /// request: a caller cannot aim a verification mail at an address their account does
    /// not already claim. An account with no local identity, or one already verified for
    /// that address, is a silent no-op — the caller cannot tell the three apart, so this
    /// is not a probe for another account's state.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] only on infrastructure failure — never
    /// "no such account".
    pub async fn start_email_verification(&self, user_id: &str) -> Result<()> {
        let logger = get_audit_logger();

        let row = sqlx::query(
            "SELECT u.pk_user, u.email AS verified_email, i.provider_id AS claimed_email \
             FROM core.tb_user u \
             JOIN core.tb_auth_identity i ON i.fk_user = u.pk_user \
             WHERE u.user_id = $1 AND i.provider = $2",
        )
        .bind(user_id)
        .bind(LOCAL_PROVIDER)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("lookup local identity for verification", &e))?;

        let Some(row) = row else {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_start",
                "no_local_identity",
            );
            return Ok(());
        };

        let fk_user: i64 = row.get("pk_user");
        let verified_email: Option<String> = row.get("verified_email");
        let claimed_email: String = row.get("claimed_email");

        if verified_email.as_deref() == Some(claimed_email.as_str()) {
            logger.log_success(
                AuditEventType::AuthSuccess,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_start:already_verified",
            );
            return Ok(());
        }

        let token = OpaqueToken::generate();
        let expires_at = Utc::now() + Duration::seconds(EMAIL_VERIFICATION_TOKEN_TTL_SECS);

        sqlx::query(
            "INSERT INTO core.tb_email_verification_token \
             (fk_user, user_id, email, selector, verifier_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(fk_user)
        .bind(user_id)
        .bind(&claimed_email)
        .bind(token.selector_b64())
        .bind(token.verifier_hash())
        .bind(expires_at)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("insert verification token", &e))?;

        // Dispatch in a spawned task: a slow or failing relay must not become a
        // caller-visible signal, and delivery failure is logged rather than surfaced.
        if let Some(sender) = self.verification_sender.clone() {
            let to = claimed_email;
            let token_str = token.to_token_string();
            tokio::spawn(async move {
                if let Err(e) = sender.send_verification_link(&to, &token_str).await {
                    tracing::warn!(
                        "email_verification_start: verification email dispatch failed: {e}"
                    );
                }
            });
        } else {
            tracing::warn!(
                "email_verification_start: token issued but no VerificationEmailSender is \
                 configured; the verification link was not delivered"
            );
        }

        logger.log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id.to_string()),
            "email_verification_start",
        );
        Ok(())
    }

    /// Redeem a verification `token` on behalf of the authenticated account `user_id`.
    ///
    /// Both halves are required and must agree: the token proves control of the mailbox,
    /// `user_id` proves the caller owns the account the token was issued for. A token
    /// presented by anyone else is rejected exactly like a forged one.
    ///
    /// On success the proved address is written to the account's `core.tb_user.email`,
    /// which is what places it in the cross-provider linking key space. No rows move
    /// between accounts — see [`decide_promotion`] and the module docs.
    ///
    /// The token is consumed on **every** decided outcome, refusals included: the refusal
    /// is deterministic, so leaving the token live would only offer a replay surface.
    ///
    /// # Errors
    ///
    /// - [`AuthError::InvalidToken`] for any unredeemable token (unknown / malformed / expired /
    ///   used / wrong verifier / issued to a different account) — one generic error; the audit log
    ///   records which.
    /// - [`AuthError::EmailClaimedByAnotherAccount`] when another account already holds the proved
    ///   address, or when this account already has a different verified address.
    /// - [`AuthError::DatabaseError`] / [`AuthError::Internal`] on a storage failure.
    pub async fn confirm_email_verification(
        &self,
        user_id: &str,
        token: &str,
    ) -> Result<EmailVerified> {
        let logger = get_audit_logger();

        let Ok(parsed) = OpaqueToken::parse(TOKEN_KIND, token) else {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "malformed_token",
            );
            return Err(invalid_verification_token());
        };

        let row = sqlx::query(
            "SELECT pk_email_verification_token AS pk, fk_user, user_id, email, verifier_hash, \
                    expires_at, used_at \
             FROM core.tb_email_verification_token WHERE selector = $1",
        )
        .bind(&parsed.selector)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("lookup verification token", &e))?;

        let Some(row) = row else {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "unknown_selector",
            );
            return Err(invalid_verification_token());
        };

        let pk: i64 = row.get("pk");
        let fk_user: i64 = row.get("fk_user");
        let token_user_id: String = row.get("user_id");
        let email: String = row.get("email");
        let stored_hash: Vec<u8> = row.get("verifier_hash");
        let expires_at: DateTime<Utc> = row.get("expires_at");
        let used_at: Option<DateTime<Utc>> = row.get("used_at");

        // Constant-time verifier comparison. The selector is high-entropy and known to
        // the holder, so an early return on a missing row leaks nothing; the secret check
        // is the verifier hash, which is always compared in constant time when a row
        // exists.
        if !ConstantTimeOps::compare(&stored_hash, &parsed.verifier_hash) {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "bad_verifier",
            );
            return Err(invalid_verification_token());
        }

        // The session half. A valid token presented by an account it was not issued to is
        // the confused-deputy shape this flow exists to refuse: the mail lands in the
        // address owner's inbox, and only the account that asked for it may spend it.
        if token_user_id != user_id {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "wrong_subject",
            );
            return Err(invalid_verification_token());
        }

        if used_at.is_some() {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "used",
            );
            return Err(invalid_verification_token());
        }
        if expires_at <= Utc::now() {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "expired",
            );
            return Err(invalid_verification_token());
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| db_error("begin verification transaction", &e))?;

        // Atomic single-use guard: mark THIS token used only if still unused and
        // unexpired. A concurrent redemption that already consumed it affects zero rows.
        let consumed = sqlx::query(
            "UPDATE core.tb_email_verification_token SET used_at = now() \
             WHERE pk_email_verification_token = $1 AND used_at IS NULL AND expires_at > now()",
        )
        .bind(pk)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("consume verification token", &e))?;

        if consumed.rows_affected() == 0 {
            tx.rollback().await.map_err(|e| db_error("rollback verification", &e))?;
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.to_string()),
                "email_verification_confirm",
                "race",
            );
            return Err(invalid_verification_token());
        }

        // Lock the account row so the gate below decides against state that cannot move
        // out from under it, then ask whether any *other* account holds the address.
        let account_email: Option<String> =
            sqlx::query("SELECT email FROM core.tb_user WHERE pk_user = $1 FOR UPDATE")
                .bind(fk_user)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| db_error("lock user row", &e))?
                .ok_or_else(|| AuthError::Internal {
                    message: "verification token resolved to a missing user row".to_string(),
                })?
                .get("email");

        let claimed_by_other =
            sqlx::query("SELECT 1 AS claimed FROM core.tb_user WHERE email = $1 AND pk_user <> $2")
                .bind(&email)
                .bind(fk_user)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| db_error("check email ownership", &e))?
                .is_some();

        let decision = decide_promotion(account_email.as_deref(), &email, claimed_by_other);

        match decision {
            PromotionDecision::Promote => {
                sqlx::query(
                    "UPDATE core.tb_user SET email = $1, updated_at = now() WHERE pk_user = $2",
                )
                .bind(&email)
                .bind(fk_user)
                .execute(&mut *tx)
                .await
                .map_err(|e| db_error("promote verified email", &e))?;
            },
            PromotionDecision::AlreadyVerified => {},
            PromotionDecision::RefuseClaimedByAnotherAccount
            | PromotionDecision::RefuseAccountHasDifferentEmail => {
                // Commit so the spent token stays spent, then refuse. Rolling back would
                // hand the presenter an unlimited-retry token for a decision that cannot
                // change on its own.
                tx.commit().await.map_err(|e| db_error("commit verification refusal", &e))?;
                logger.log_failure(
                    AuditEventType::AuthFailure,
                    SecretType::SessionToken,
                    Some(user_id.to_string()),
                    "email_verification_confirm",
                    if decision == PromotionDecision::RefuseClaimedByAnotherAccount {
                        "email_claimed_by_another_account"
                    } else {
                        "account_has_a_different_verified_email"
                    },
                );
                return Err(AuthError::EmailClaimedByAnotherAccount);
            },
        }

        // Invalidate the account's other outstanding verification tokens (the consumed
        // one already has used_at set, so this excludes it).
        sqlx::query(
            "UPDATE core.tb_email_verification_token SET used_at = now() \
             WHERE fk_user = $1 AND used_at IS NULL",
        )
        .bind(fk_user)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("invalidate sibling verification tokens", &e))?;

        tx.commit().await.map_err(|e| db_error("commit verification transaction", &e))?;

        logger.log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id.to_string()),
            "email_verification_confirm",
        );

        Ok(EmailVerified {
            user_id: user_id.to_string(),
            email,
        })
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
