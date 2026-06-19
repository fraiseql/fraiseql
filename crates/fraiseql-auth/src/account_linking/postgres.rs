//! PostgreSQL-backed [`AccountStore`] — durable user / identity persistence.
//!
//! This is the production backend for [`AccountStore`](super::AccountStore); the
//! [`InMemoryAccountStore`](super::InMemoryAccountStore) loses all linkage on
//! restart. It is a drop-in replacement (same trait, same `"user_<uuid>"`
//! identifier format, so it joins the existing `_system.sessions.user_id`), so
//! `multi_provider` / `phone_otp` need no change beyond which `Arc<dyn AccountStore>`
//! they are handed.
//!
//! # Schema
//!
//! - `core.tb_user` — one row per stable account (`user_id`, optional verified email).
//! - `core.tb_auth_identity` — one row per linked `(provider, provider_id)`, FK to a user.
//!
//! Both carry a `tenant_id` and RLS deny-by-default (mirroring the change-log RLS in
//! observers migration `12`). RLS is `ENABLE`, not `FORCE`: this store runs as the
//! table owner and bypasses the policies — exactly like the executor/poller for the
//! change-log — while any other (non-`BYPASSRLS`) role reads zero rows unless it sets
//! the `fraiseql.tenant_id` GUC. v1 operates single-tenant (`tenant_id` NULL, since the
//! [`AccountStore`](super::AccountStore) trait carries no tenant parameter); per-tenant
//! scoping is a forward-compatible extension.

use async_trait::async_trait;
use sqlx::{Row, postgres::PgPool};
use uuid::Uuid;

use super::{AccountLinkResult, AccountRecord, AccountStore, ProviderLink, normalize_email};
use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
};

/// Idempotent DDL for the user / identity store. Exposed so a migration runner can
/// apply it explicitly; [`PostgresAccountStore::init`] runs the same statements.
pub const SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_user (
    pk_user    BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id         UUID NOT NULL DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL UNIQUE,
    email      TEXT,
    tenant_id  UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_email ON core.tb_user (email) WHERE email IS NOT NULL;

CREATE TABLE IF NOT EXISTS core.tb_auth_identity (
    pk_auth_identity BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid(),
    fk_user     BIGINT NOT NULL REFERENCES core.tb_user (pk_user) ON DELETE CASCADE,
    user_id     TEXT NOT NULL,
    provider    TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    tenant_id   UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_id)
);
CREATE INDEX IF NOT EXISTS idx_auth_identity_user    ON core.tb_auth_identity (fk_user);
CREATE INDEX IF NOT EXISTS idx_auth_identity_user_id ON core.tb_auth_identity (user_id);

-- RLS deny-by-default (mirrors observers migration 12). ENABLE not FORCE so the
-- owner (this store) and BYPASSRLS roles operate freely; a non-owner role reads a
-- row only once it has set fraiseql.tenant_id to that row's tenant (fail-closed).
ALTER TABLE core.tb_user          ENABLE ROW LEVEL SECURITY;
ALTER TABLE core.tb_auth_identity ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_user_tenant_read ON core.tb_user;
CREATE POLICY p_user_tenant_read ON core.tb_user
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_user_insert ON core.tb_user;
CREATE POLICY p_user_insert ON core.tb_user FOR INSERT WITH CHECK (true);

DROP POLICY IF EXISTS p_auth_identity_tenant_read ON core.tb_auth_identity;
CREATE POLICY p_auth_identity_tenant_read ON core.tb_auth_identity
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_auth_identity_insert ON core.tb_auth_identity;
CREATE POLICY p_auth_identity_insert ON core.tb_auth_identity FOR INSERT WITH CHECK (true);

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON core.tb_user          FROM PUBLIC;
REVOKE ALL ON core.tb_auth_identity FROM PUBLIC;
";

/// PostgreSQL-backed account store.
///
/// Persists user accounts and their linked provider identities, so account linking
/// survives a process restart. See the module-level documentation for the schema and
/// RLS posture.
pub struct PostgresAccountStore {
    db: PgPool,
}

impl PostgresAccountStore {
    /// Create a new store over an existing pool.
    ///
    /// The pool's role must own (or `BYPASSRLS`) the `core.tb_user` /
    /// `core.tb_auth_identity` tables — it runs the trusted login path and must not be
    /// constrained by the deny-by-default RLS. Calling [`init`](Self::init) once on
    /// startup creates the tables (so the connecting role owns them by construction).
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create the `core.tb_user` / `core.tb_auth_identity` schema (idempotent).
    ///
    /// Call once on startup. Safe to re-run and safe on a database that predates this
    /// store (the `CREATE … IF NOT EXISTS` form is the back-compat path for existing
    /// deployments that have no user table).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the DDL fails.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(SCHEMA_SQL).execute(&self.db).await.map_err(|e| {
            AuthError::DatabaseError {
                message: format!("Failed to initialize identity store: {e}"),
            }
        })?;
        Ok(())
    }
}

/// Generate a fresh stable user identifier, matching the in-memory store's format so
/// the two backends are interchangeable and the value joins `_system.sessions.user_id`.
fn new_user_id() -> String {
    format!("user_{}", Uuid::new_v4().as_simple())
}

fn db_error(context: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("{context}: {e}"),
    }
}

// Reason: AccountStore is defined with #[async_trait]; the impl must match its
// transformed signatures. async_trait: dyn-dispatch required; remove when RTN + Send
// is stable (RFC 3425).
#[async_trait]
impl AccountStore for PostgresAccountStore {
    async fn link_or_create_user(
        &self,
        email: Option<&str>,
        email_verified: bool,
        provider: &str,
        provider_id: &str,
    ) -> Result<AccountLinkResult> {
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin tx", &e))?;

        // 1. A known (provider, provider_id) is an idempotent re-login: same user, no new link. The
        //    UNIQUE(provider, provider_id) constraint makes this the authoritative lookup.
        if let Some(row) = sqlx::query(
            "SELECT user_id FROM core.tb_auth_identity WHERE provider = $1 AND provider_id = $2",
        )
        .bind(provider)
        .bind(provider_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_error("lookup identity", &e))?
        {
            let user_id: String = row.get("user_id");
            tx.commit().await.map_err(|e| db_error("commit", &e))?;
            return Ok(AccountLinkResult {
                user_id,
                is_new: false,
                linked: false,
            });
        }

        // 2. Resolve the linking key. A verified, non-empty email links across providers; anything
        //    else is keyed on (provider, provider_id) so distinct identities can never collapse
        //    (H26).
        let verified_email = email.map(normalize_email).filter(|e| !e.is_empty() && email_verified);

        // 3. Find the email-keyed user, or create a fresh account.
        let (user_id, pk_user, is_new, linked) = if let Some(em) = verified_email.as_deref() {
            if let Some(row) =
                sqlx::query("SELECT pk_user, user_id FROM core.tb_user WHERE email = $1")
                    .bind(em)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| db_error("lookup user by email", &e))?
            {
                let pk_user: i64 = row.get("pk_user");
                let user_id: String = row.get("user_id");
                (user_id, pk_user, false, true)
            } else {
                let user_id = new_user_id();
                let pk_user = insert_user(&mut tx, &user_id, Some(em)).await?;
                (user_id, pk_user, true, false)
            }
        } else {
            let user_id = new_user_id();
            let pk_user = insert_user(&mut tx, &user_id, None).await?;
            (user_id, pk_user, true, false)
        };

        // 4. Link the provider identity (new for this account by construction — step 1 ruled out an
        //    existing one).
        sqlx::query(
            "INSERT INTO core.tb_auth_identity (fk_user, user_id, provider, provider_id) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(pk_user)
        .bind(&user_id)
        .bind(provider)
        .bind(provider_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| db_error("insert identity", &e))?;

        tx.commit().await.map_err(|e| db_error("commit", &e))?;

        let logger = get_audit_logger();
        if is_new {
            logger.log_success(
                AuditEventType::SessionTokenCreated,
                SecretType::SessionToken,
                Some(user_id.clone()),
                &format!("account_created:{provider}"),
            );
        } else {
            logger.log_success(
                AuditEventType::AuthSuccess,
                SecretType::SessionToken,
                Some(user_id.clone()),
                &format!("account_linked:{provider}"),
            );
        }

        Ok(AccountLinkResult {
            user_id,
            is_new,
            linked,
        })
    }

    async fn get_account(&self, user_id: &str) -> Result<AccountRecord> {
        let email: Option<String> =
            sqlx::query("SELECT email FROM core.tb_user WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| db_error("lookup user", &e))?
                .ok_or(AuthError::TokenNotFound)?
                .get("email");

        let providers = sqlx::query(
            "SELECT provider, provider_id FROM core.tb_auth_identity \
             WHERE user_id = $1 ORDER BY pk_auth_identity",
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("lookup identities", &e))?
        .into_iter()
        .map(|row| ProviderLink {
            provider:    row.get("provider"),
            provider_id: row.get("provider_id"),
        })
        .collect();

        Ok(AccountRecord {
            user_id: user_id.to_string(),
            email,
            providers,
        })
    }
}

/// Insert a new user row and return its `pk_user`.
async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    email: Option<&str>,
) -> Result<i64> {
    let row =
        sqlx::query("INSERT INTO core.tb_user (user_id, email) VALUES ($1, $2) RETURNING pk_user")
            .bind(user_id)
            .bind(email)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| db_error("insert user", &e))?;
    Ok(row.get("pk_user"))
}
