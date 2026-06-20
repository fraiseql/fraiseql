//! Local email + password authentication using Argon2id.
//!
//! [`LocalPasswordAuthenticator`] adds an email/password sign-in method on top of the
//! #411 durable identity store. It is the durable counterpart to the OAuth/OIDC flows:
//! signup resolves or creates a user through the existing [`AccountStore`]
//! (provider `"local"`), and the password hash is stored separately in
//! `core.tb_password_credential` — never adjacent to plaintext, never in the user row.
//!
//! # Security design
//!
//! - **Argon2id**, memory-hard, with per-credential random salts. Verification is constant-time
//!   (the `password-hash` crate compares via `subtle`).
//! - **provider_id is the normalized email.** The local identity keys on `(provider = "local",
//!   provider_id = normalize_email(email))`, reusing the `UNIQUE (provider, provider_id)` index on
//!   `core.tb_auth_identity` as the login lookup key — one source of truth, no extra column.
//! - **Signup is fail-closed.** It links with `email_verified = false`, so a local signup keys its
//!   own `(local, email)` account and can never auto-merge into an existing verified-email account
//!   (the H26 protection against takeover via an unverified signup). `core.tb_user.email` therefore
//!   stays `NULL` until a future verification flow promotes it.
//! - **Login is non-enumerable.** An unknown user and a wrong password are indistinguishable: both
//!   return [`AuthError::InvalidCredentials`] with the same body, and both pay the full Argon2 cost
//!   — an unknown user is verified against a pre-computed dummy hash built from the *same*
//!   parameters as live credentials, so timing does not leak existence. The email → credential
//!   lookup runs on both paths, so the database round-trip cannot leak existence either.
//! - **Disabled is a deliberate, narrow disclosure.** [`AuthError::AccountDisabled`] is returned
//!   only when the supplied password is *correct*; a wrong password against a disabled account
//!   still returns [`AuthError::InvalidCredentials`]. Disclosing "this account is disabled" to a
//!   party already holding valid credentials is an accepted trade-off for this threat model
//!   ("disabled" = administratively suspended local sign-in). It is never reachable without the
//!   correct password, so it is not an existence oracle.
//! - **Audit asymmetry.** The client sees one merged error; the server audit log records the
//!   precise reason (`unknown_user` / `wrong_password` / `disabled`) under
//!   [`AuditEventType::AuthFailure`].
//! - **Rehash on policy change.** A successful login whose stored hash was produced with weaker
//!   parameters than the current policy is transparently re-hashed and updated.
//!
//! ## Deferred (intentionally out of scope for v1)
//!
//! - **Rate limiting / lockout** on repeated failures. Argon2's cost throttles online guessing only
//!   so far; per-account/IP backoff is a follow-up (a lockout is itself a disabled-state with the
//!   same disclosure trade-off as above).
//! - **Non-enumerable signup.** [`AuthError::EmailAlreadyRegistered`] is a signup existence oracle;
//!   the standard "we emailed you" mitigation needs the email-action path (#349), not yet shipped.
//! - **Password reset / email verification** — #367, reusing the #349 email path.

use std::sync::Arc;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use sqlx::{Row, postgres::PgPool};

use crate::{
    account_linking::{AccountStore, SCHEMA_SQL as IDENTITY_SCHEMA_SQL, normalize_email},
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
};

/// Provider name recorded for local-password identities in `core.tb_auth_identity`.
const LOCAL_PROVIDER: &str = "local";

/// Minimum password length in bytes. A floor, not a policy engine — see the module docs
/// for the deferred configurable-policy work.
const MIN_PASSWORD_LEN: usize = 12;

/// Maximum password length in bytes. Argon2 has no inherent maximum, but hashing an
/// unbounded input is a denial-of-service vector, so oversize passwords are rejected.
const MAX_PASSWORD_LEN: usize = 4096;

/// Fixed input hashed once at construction to produce the timing-equalization dummy
/// hash. Its value is irrelevant — the dummy hash exists only to make an unknown-user
/// verification pay the same Argon2 cost as a real one.
const DUMMY_PASSWORD: &[u8] = b"fraiseql-local-password-timing-equalization-dummy";

/// Idempotent DDL for the local-password credential store.
///
/// Exposed so a migration runner can apply it explicitly;
/// [`LocalPasswordAuthenticator::init`] runs it (after ensuring the #411 identity schema,
/// which it FK-references). Mirrors the #411 identity tables: Trinity `pk_`/`fk_`/`id`
/// columns, deny-by-default RLS (`ENABLE`, not `FORCE`, so the owning store bypasses while
/// any other role reads zero rows without the `fraiseql.tenant_id` GUC), and
/// `REVOKE ALL … FROM PUBLIC`.
pub const PASSWORD_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_password_credential (
    pk_password_credential BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id            UUID NOT NULL DEFAULT gen_random_uuid(),
    fk_user       BIGINT NOT NULL REFERENCES core.tb_user (pk_user) ON DELETE CASCADE,
    user_id       TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    disabled_at   TIMESTAMPTZ,
    tenant_id     UUID,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (fk_user)
);
CREATE INDEX IF NOT EXISTS idx_password_credential_user_id
    ON core.tb_password_credential (user_id);

-- RLS deny-by-default (mirrors core.tb_user / core.tb_auth_identity from #411).
ALTER TABLE core.tb_password_credential ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_password_credential_tenant_read ON core.tb_password_credential;
CREATE POLICY p_password_credential_tenant_read ON core.tb_password_credential
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_password_credential_insert ON core.tb_password_credential;
CREATE POLICY p_password_credential_insert ON core.tb_password_credential
    FOR INSERT WITH CHECK (true);

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON core.tb_password_credential FROM PUBLIC;
";

/// Email + password authenticator backed by Argon2id and the #411 identity store.
///
/// Construct with [`new`](Self::new) (OWASP-default parameters) or
/// [`with_params`](Self::with_params) (to tune cost), call [`init`](Self::init) once on
/// startup, then [`signup`](Self::signup) / [`login`](Self::login). The connecting
/// `PgPool` role must own (or `BYPASSRLS`) the `core` tables — calling `init` creates
/// them, so the connecting role owns them by construction.
pub struct LocalPasswordAuthenticator {
    db:         PgPool,
    /// Resolves/creates users at signup (provider `"local"`). Any [`AccountStore`] that
    /// persists into `core.tb_auth_identity` works; in practice this is
    /// [`PostgresAccountStore`](crate::PostgresAccountStore), since login resolves
    /// email → `user_id` through that table.
    accounts:   Arc<dyn AccountStore>,
    argon2:     Argon2<'static>,
    /// A real Argon2id hash, built from `argon2`'s parameters, used to equalize the
    /// verification cost of an unknown-user login with a real one.
    dummy_hash: String,
}

impl LocalPasswordAuthenticator {
    /// Create an authenticator with OWASP-default Argon2id parameters.
    #[must_use]
    pub fn new(db: PgPool, accounts: Arc<dyn AccountStore>) -> Self {
        Self::build(db, accounts, Params::DEFAULT)
    }

    /// Create an authenticator with explicit Argon2id cost parameters: `m_cost` (memory
    /// in KiB), `t_cost` (iterations), and `p_cost` (parallelism lanes).
    ///
    /// Use this to raise the cost over the default, or (in tests) to lower it. A login
    /// whose stored hash used different parameters is transparently rehashed to these on
    /// the next successful sign-in.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if the parameters are not a valid Argon2
    /// combination (e.g. `m_cost < 8 * p_cost`).
    pub fn with_params(
        db: PgPool,
        accounts: Arc<dyn AccountStore>,
        m_cost: u32,
        t_cost: u32,
        p_cost: u32,
    ) -> Result<Self> {
        let params =
            Params::new(m_cost, t_cost, p_cost, None).map_err(|e| AuthError::ConfigError {
                message: format!("invalid Argon2 parameters: {e}"),
            })?;
        Ok(Self::build(db, accounts, params))
    }

    /// Shared constructor: wrap a parameter set into an Argon2id context and pre-compute
    /// the timing-equalization dummy hash.
    fn build(db: PgPool, accounts: Arc<dyn AccountStore>, params: Params) -> Self {
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let dummy_hash = compute_dummy_hash(&argon2);
        Self {
            db,
            accounts,
            argon2,
            dummy_hash,
        }
    }

    /// Ensure the identity + credential schema exists (idempotent).
    ///
    /// Runs the #411 identity DDL first (the credential table FK-references
    /// `core.tb_user`) and then the credential DDL, so it is self-sufficient whether or
    /// not [`PostgresAccountStore::init`](crate::PostgresAccountStore::init) has already
    /// run. Call once on startup.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the DDL fails.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(IDENTITY_SCHEMA_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("initialize identity store (prerequisite)", &e))?;
        sqlx::raw_sql(PASSWORD_SCHEMA_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("initialize password credential store", &e))?;
        Ok(())
    }

    /// Register a new local email + password account. Returns the stable `user_id`.
    ///
    /// Validates the input, resolves or creates the user through the
    /// [`AccountStore`] with `email_verified = false` (fail-closed —
    /// no auto-link into a verified-email account), then stores the Argon2id hash.
    ///
    /// # Errors
    ///
    /// - [`AuthError::InvalidRegistration`] if the email is empty/malformed or the password
    ///   violates the length policy.
    /// - [`AuthError::EmailAlreadyRegistered`] if a local credential already exists for this email.
    /// - [`AuthError::DatabaseError`] / [`AuthError::Internal`] on a storage failure.
    pub async fn signup(&self, email: &str, password: &str) -> Result<String> {
        // Validate before any database work or hashing so bad input fails fast.
        validate_credentials(email, password)?;
        let normalized = normalize_email(email);

        // Fail-closed: email_verified = false keys the identity on (local, email) and
        // never merges into an existing verified-email account.
        let link = self
            .accounts
            .link_or_create_user(Some(&normalized), false, LOCAL_PROVIDER, &normalized)
            .await?;
        let user_id = link.user_id;

        let pk_user: i64 = sqlx::query("SELECT pk_user FROM core.tb_user WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| db_error("resolve user for credential", &e))?
            .ok_or_else(|| AuthError::Internal {
                message: "user row missing immediately after link_or_create_user".to_string(),
            })?
            .get("pk_user");

        let password_hash = self.hash_password(password)?;

        // UNIQUE (fk_user): a second local signup for the same account inserts no row.
        let result = sqlx::query(
            "INSERT INTO core.tb_password_credential (fk_user, user_id, password_hash) \
             VALUES ($1, $2, $3) ON CONFLICT (fk_user) DO NOTHING",
        )
        .bind(pk_user)
        .bind(&user_id)
        .bind(&password_hash)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("insert credential", &e))?;

        if result.rows_affected() == 0 {
            return Err(AuthError::EmailAlreadyRegistered);
        }

        get_audit_logger().log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id.clone()),
            "local_signup",
        );
        Ok(user_id)
    }

    /// Verify an email + password and return the stable `user_id` on success.
    ///
    /// Non-enumerable: an unknown user and a wrong password return the same
    /// [`AuthError::InvalidCredentials`] and pay the same Argon2 cost (unknown users are
    /// verified against a same-parameter dummy hash). A correct password on a disabled
    /// account returns [`AuthError::AccountDisabled`]; a wrong password on a disabled
    /// account returns [`AuthError::InvalidCredentials`] (no disabled disclosure). A
    /// successful login rehashes if the stored parameters are weaker than the current
    /// policy.
    ///
    /// # Errors
    ///
    /// - [`AuthError::InvalidCredentials`] for unknown user or wrong password.
    /// - [`AuthError::AccountDisabled`] for a disabled account with the correct password.
    /// - [`AuthError::DatabaseError`] / [`AuthError::Internal`] on a storage failure.
    pub async fn login(&self, email: &str, password: &str) -> Result<String> {
        let normalized = normalize_email(email);

        // Resolve email → credential FIRST so the database round-trip runs on every path
        // (a missing-row early return would itself be a timing oracle).
        let row = sqlx::query(
            "SELECT c.user_id, c.password_hash, (c.disabled_at IS NOT NULL) AS disabled \
             FROM core.tb_password_credential c \
             JOIN core.tb_auth_identity i ON i.user_id = c.user_id \
             WHERE i.provider = $1 AND i.provider_id = $2",
        )
        .bind(LOCAL_PROVIDER)
        .bind(&normalized)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("lookup credential", &e))?;

        // Verify against the real hash, or the dummy hash for an unknown user, so the
        // Argon2 cost (and thus timing) is identical either way.
        let hash_str: String =
            row.as_ref().map_or_else(|| self.dummy_hash.clone(), |r| r.get("password_hash"));
        let parsed = PasswordHash::new(&hash_str).map_err(|e| AuthError::Internal {
            message: format!("stored password hash is unparseable: {e}"),
        })?;
        let verified = self.argon2.verify_password(password.as_bytes(), &parsed).is_ok();

        let logger = get_audit_logger();
        let Some(row) = row else {
            // Unknown user — indistinguishable from a wrong password to the client.
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "local_login",
                "unknown_user",
            );
            return Err(AuthError::InvalidCredentials);
        };
        let user_id: String = row.get("user_id");

        if !verified {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "local_login",
                "wrong_password",
            );
            return Err(AuthError::InvalidCredentials);
        }

        // Disabled is disclosed only now — after the password is proven correct.
        let disabled: bool = row.get("disabled");
        if disabled {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id),
                "local_login",
                "disabled",
            );
            return Err(AuthError::AccountDisabled);
        }

        // Rehash transparently if the stored parameters are weaker than current policy.
        // A rehash failure must not fail the login — the password was correct; the next
        // login retries.
        if needs_rehash(&parsed, self.argon2.params()) {
            match self.hash_password(password) {
                Ok(new_hash) => {
                    if let Err(e) = self.update_hash(&user_id, &new_hash).await {
                        tracing::warn!("local_login: rehash update failed for {user_id}: {e}");
                    }
                },
                Err(e) => tracing::warn!("local_login: rehash failed for {user_id}: {e}"),
            }
        }

        logger.log_success(
            AuditEventType::AuthSuccess,
            SecretType::SessionToken,
            Some(user_id.clone()),
            "local_login",
        );
        Ok(user_id)
    }

    /// Enable or disable local-password sign-in for an account.
    ///
    /// Disabling stamps `disabled_at`; a subsequent [`login`](Self::login) with the
    /// correct password returns [`AuthError::AccountDisabled`]. Enabling clears it.
    ///
    /// # Errors
    ///
    /// - [`AuthError::TokenNotFound`] if the user has no local credential.
    /// - [`AuthError::DatabaseError`] on a storage failure.
    pub async fn set_password_disabled(&self, user_id: &str, disabled: bool) -> Result<()> {
        let result = sqlx::query(
            "UPDATE core.tb_password_credential \
             SET disabled_at = CASE WHEN $1 THEN now() ELSE NULL END, updated_at = now() \
             WHERE user_id = $2",
        )
        .bind(disabled)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("set credential disabled state", &e))?;

        if result.rows_affected() == 0 {
            return Err(AuthError::TokenNotFound);
        }
        Ok(())
    }

    /// Hash a password with the configured Argon2id parameters and a fresh random salt.
    fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AuthError::Internal {
                message: format!("password hashing failed: {e}"),
            })
    }

    /// Persist a re-hashed credential for an existing user.
    async fn update_hash(&self, user_id: &str, new_hash: &str) -> Result<()> {
        sqlx::query(
            "UPDATE core.tb_password_credential \
             SET password_hash = $1, updated_at = now() WHERE user_id = $2",
        )
        .bind(new_hash)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("update credential hash", &e))?;
        Ok(())
    }
}

/// Build the timing-equalization dummy hash from an authenticator's parameters.
fn compute_dummy_hash(argon2: &Argon2<'_>) -> String {
    let salt = SaltString::generate(&mut OsRng);
    argon2
        .hash_password(DUMMY_PASSWORD, &salt)
        // Hashing a fixed short input with structurally-valid Argon2 parameters cannot
        // fail; `Params` is validated at construction, so this is unreachable.
        .expect("Argon2id hashing of the fixed dummy password with valid parameters is infallible")
        .to_string()
}

/// Validate a signup email and password without touching the database.
fn validate_credentials(email: &str, password: &str) -> Result<()> {
    let trimmed = email.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return Err(AuthError::InvalidRegistration {
            reason: "email is empty or malformed".to_string(),
        });
    }
    let len = password.len();
    if len < MIN_PASSWORD_LEN {
        return Err(AuthError::InvalidRegistration {
            reason: format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        });
    }
    if len > MAX_PASSWORD_LEN {
        return Err(AuthError::InvalidRegistration {
            reason: format!("password exceeds the {MAX_PASSWORD_LEN}-byte maximum"),
        });
    }
    Ok(())
}

/// Whether a stored hash should be re-hashed because its parameters are weaker than (or
/// otherwise differ from) the current policy. An unparseable parameter set is treated as
/// stale.
fn needs_rehash(stored: &PasswordHash<'_>, current: &Params) -> bool {
    match Params::try_from(stored) {
        Ok(p) => {
            p.m_cost() != current.m_cost()
                || p.t_cost() != current.t_cost()
                || p.p_cost() != current.p_cost()
        },
        Err(_) => true,
    }
}

fn db_error(context: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("{context}: {e}"),
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
