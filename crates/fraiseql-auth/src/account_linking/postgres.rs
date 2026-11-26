//! PostgreSQL-backed [`AccountStore`] — durable user / identity persistence.
//!
//! This is the production backend for [`AccountStore`](super::AccountStore); the
//! [`InMemoryAccountStore`](super::InMemoryAccountStore) loses all linkage on
//! restart. It is a drop-in replacement (same trait, same `"user_<uuid>"`
//! identifier format, so it joins the existing `auth.sessions.user_id`), so
//! `multi_provider` needs no change beyond which `Arc<dyn AccountStore>` it is
//! handed.
//!
//! # Schema
//!
//! - `auth.tb_user` — one row per stable account (`user_id`, optional verified email).
//! - `auth.tb_auth_identity` — one row per linked `(provider, provider_id)`, FK to a user.
//!
//! Both carry a `tenant_id` and RLS deny-by-default (mirroring the change-log RLS in
//! observers migration `12`). RLS is `ENABLE`, not `FORCE`: this store runs as the
//! table owner and bypasses the policies — exactly like the executor/poller for the
//! change-log — while any other (non-`BYPASSRLS`) role reads zero rows unless it sets
//! the `fraiseql.tenant_id` GUC.
//!
//! # Account spaces (#1088)
//!
//! `tenant_id` partitions accounts: `NULL` is the platform, a UUID is that tenant. Every key
//! is unique *within* a space — email, SCIM `userName`, `(provider, provider_id)` — so the
//! same address in two tenants is two accounts, and a merge can never cross a space. Each
//! key is one unique index whose `tenant_id` column is `NULLS NOT DISTINCT`, so the platform
//! is a space like any tenant rather than a set of rows that never collide. The key columns
//! lead and `tenant_id` comes last: lookups match the key with `=` and the space with
//! `IS NOT DISTINCT FROM`, which an index cannot use as a leading column. [`SCHEMA_SQL`]
//! drops the global keys an earlier release created, so `init` migrates an existing
//! database in place without moving any row: existing accounts are platform accounts, as
//! before.

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
pub const SCHEMA_SQL: &str = concat!(
    claim_auth_schema!(),
    r"

CREATE TABLE IF NOT EXISTS auth.tb_user (
    pk_user    BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id         UUID NOT NULL DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL UNIQUE,
    email      TEXT,
    tenant_id  UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Keys are unique per account space (#1088): see the module docs. The global index an
-- earlier release created is replaced in place.
DROP INDEX IF EXISTS auth.uq_user_email;
DROP INDEX IF EXISTS auth.uq_user_email_platform;
DROP INDEX IF EXISTS auth.uq_user_email_tenant;
DROP INDEX IF EXISTS auth.uq_user_user_name;
DROP INDEX IF EXISTS auth.uq_user_user_name_platform;
DROP INDEX IF EXISTS auth.uq_user_user_name_tenant;

-- Every replica runs this at boot while the others serve sign-ins. `ADD COLUMN IF NOT
-- EXISTS` takes an ACCESS EXCLUSIVE lock and `CREATE INDEX IF NOT EXISTS` a SHARE lock even
-- when the column or index is there, so each is guarded by the catalog and a boot over a
-- current schema takes neither.
DO $user_shape$
BEGIN
    -- SCIM 2.0 provisioning (#946): a database that predates provisioning upgrades in
    -- place.
    --
    -- `active` is the load-bearing one: it is what an IdP flips when it offboards someone,
    -- and it must block *every* credential on the account, not merely SAML. It defaults
    -- TRUE so every pre-existing row stays exactly as active as it was.
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'active' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN active BOOLEAN NOT NULL DEFAULT TRUE;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'user_name' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN user_name TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'external_id' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN external_id TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'given_name' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN given_name TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'family_name' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN family_name TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'display_name' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN display_name TEXT;
    END IF;
    -- Monotonic per-row counter behind the SCIM `meta.version` / `ETag`, so a provisioning
    -- client's If-Match can detect a lost update. A timestamp would not: two writes inside
    -- one clock tick would share a version.
    IF NOT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid = 'auth.tb_user'::regclass
                   AND attname = 'version' AND NOT attisdropped) THEN
        ALTER TABLE auth.tb_user ADD COLUMN version BIGINT NOT NULL DEFAULT 1;
    END IF;

    IF to_regclass('auth.uq_user_email_per_space') IS NULL THEN
        CREATE UNIQUE INDEX uq_user_email_per_space
            ON auth.tb_user (email, tenant_id) NULLS NOT DISTINCT WHERE email IS NOT NULL;
    END IF;
    IF to_regclass('auth.uq_user_user_name_per_space') IS NULL THEN
        CREATE UNIQUE INDEX uq_user_user_name_per_space
            ON auth.tb_user (user_name, tenant_id) NULLS NOT DISTINCT WHERE user_name IS NOT NULL;
    END IF;
    IF to_regclass('auth.idx_user_external_id') IS NULL THEN
        CREATE INDEX idx_user_external_id
            ON auth.tb_user (external_id) WHERE external_id IS NOT NULL;
    END IF;
END
$user_shape$;

CREATE TABLE IF NOT EXISTS auth.tb_auth_identity (
    pk_auth_identity BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id          UUID NOT NULL DEFAULT gen_random_uuid(),
    fk_user     BIGINT NOT NULL REFERENCES auth.tb_user (pk_user) ON DELETE CASCADE,
    user_id     TEXT NOT NULL,
    provider    TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    tenant_id   UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
DROP INDEX IF EXISTS auth.uq_auth_identity_platform;
DROP INDEX IF EXISTS auth.uq_auth_identity_tenant;
DO $auth_identity_shape$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint
               WHERE conrelid = 'auth.tb_auth_identity'::regclass
                 AND conname = 'tb_auth_identity_provider_provider_id_key') THEN
        ALTER TABLE auth.tb_auth_identity
            DROP CONSTRAINT tb_auth_identity_provider_provider_id_key;
    END IF;
    IF to_regclass('auth.uq_auth_identity_per_space') IS NULL THEN
        CREATE UNIQUE INDEX uq_auth_identity_per_space
            ON auth.tb_auth_identity (provider, provider_id, tenant_id) NULLS NOT DISTINCT;
    END IF;
    IF to_regclass('auth.idx_auth_identity_user') IS NULL THEN
        CREATE INDEX idx_auth_identity_user ON auth.tb_auth_identity (fk_user);
    END IF;
    IF to_regclass('auth.idx_auth_identity_user_id') IS NULL THEN
        CREATE INDEX idx_auth_identity_user_id ON auth.tb_auth_identity (user_id);
    END IF;
END
$auth_identity_shape$;

-- RLS deny-by-default (mirrors observers migration 12). ENABLE not FORCE so the
-- owner (this store) and BYPASSRLS roles operate freely; a non-owner role reads a
-- row only once it has set fraiseql.tenant_id to that row's tenant (fail-closed).
-- `ENABLE ROW LEVEL SECURITY`, `DROP POLICY` and `CREATE POLICY` each take an ACCESS
-- EXCLUSIVE lock even when they change nothing, so each is guarded by the catalog: RLS by
-- `relrowsecurity`, each policy by its whole definition in `pg_policies`, and a policy
-- whose definition differs is replaced. `tenant_read` is the read policy's expression as
-- PostgreSQL 18 (the supported floor) stores it; should it differ from what the CREATE
-- below stores, every boot replaces the policy under its lock and the boot-lock test fails.
DO $user_rls$
DECLARE
    tenant_read CONSTANT text :=
        $q$(tenant_id = (NULLIF(current_setting('fraiseql.tenant_id'::text, true), ''::text))::uuid)$q$;
BEGIN
    IF NOT (SELECT relrowsecurity FROM pg_class WHERE oid = 'auth.tb_user'::regclass) THEN
        ALTER TABLE auth.tb_user ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'auth' AND tablename = 'tb_user' AND policyname = 'p_user_tenant_read'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'SELECT'
          AND qual = tenant_read AND with_check IS NULL
    ) THEN
        DROP POLICY IF EXISTS p_user_tenant_read ON auth.tb_user;
        CREATE POLICY p_user_tenant_read ON auth.tb_user
            FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'auth' AND tablename = 'tb_user' AND policyname = 'p_user_insert'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'INSERT'
          AND qual IS NULL AND with_check = 'true'
    ) THEN
        DROP POLICY IF EXISTS p_user_insert ON auth.tb_user;
        CREATE POLICY p_user_insert ON auth.tb_user FOR INSERT WITH CHECK (true);
    END IF;
END
$user_rls$;

-- `ENABLE ROW LEVEL SECURITY`, `DROP POLICY` and `CREATE POLICY` each take an ACCESS
-- EXCLUSIVE lock even when they change nothing, so each is guarded by the catalog: RLS by
-- `relrowsecurity`, each policy by its whole definition in `pg_policies`, and a policy
-- whose definition differs is replaced. `tenant_read` is the read policy's expression as
-- PostgreSQL 18 (the supported floor) stores it; should it differ from what the CREATE
-- below stores, every boot replaces the policy under its lock and the boot-lock test fails.
DO $auth_identity_rls$
DECLARE
    tenant_read CONSTANT text :=
        $q$(tenant_id = (NULLIF(current_setting('fraiseql.tenant_id'::text, true), ''::text))::uuid)$q$;
BEGIN
    IF NOT (SELECT relrowsecurity FROM pg_class WHERE oid = 'auth.tb_auth_identity'::regclass) THEN
        ALTER TABLE auth.tb_auth_identity ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'auth' AND tablename = 'tb_auth_identity' AND policyname = 'p_auth_identity_tenant_read'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'SELECT'
          AND qual = tenant_read AND with_check IS NULL
    ) THEN
        DROP POLICY IF EXISTS p_auth_identity_tenant_read ON auth.tb_auth_identity;
        CREATE POLICY p_auth_identity_tenant_read ON auth.tb_auth_identity
            FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'auth' AND tablename = 'tb_auth_identity' AND policyname = 'p_auth_identity_insert'
          AND permissive = 'PERMISSIVE' AND roles = '{public}'::name[] AND cmd = 'INSERT'
          AND qual IS NULL AND with_check = 'true'
    ) THEN
        DROP POLICY IF EXISTS p_auth_identity_insert ON auth.tb_auth_identity;
        CREATE POLICY p_auth_identity_insert ON auth.tb_auth_identity FOR INSERT WITH CHECK (true);
    END IF;
END
$auth_identity_rls$;

-- Least-privilege baseline: never world-readable. RLS is defence-in-depth on top.
REVOKE ALL ON auth.tb_user          FROM PUBLIC;
REVOKE ALL ON auth.tb_auth_identity FROM PUBLIC;
"
);

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
    /// The pool's role must own (or `BYPASSRLS`) the `auth.tb_user` /
    /// `auth.tb_auth_identity` tables — it runs the trusted login path and must not be
    /// constrained by the deny-by-default RLS. Calling [`init`](Self::init) once on
    /// startup creates the tables (so the connecting role owns them by construction).
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create the `auth.tb_user` / `auth.tb_auth_identity` schema (idempotent).
    ///
    /// Call once on startup. Safe to re-run and safe on a database that predates this
    /// store (the `CREATE … IF NOT EXISTS` form is the back-compat path for existing
    /// deployments that have no user table).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the DDL fails.
    pub async fn init(&self) -> Result<()> {
        // Reason: the crate's own DDL constant, behind the workspace's fixed lock statement.
        sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(SCHEMA_SQL)))
            .execute(&self.db)
            .await
            .map_err(|e| AuthError::DatabaseError {
                message: format!("Failed to initialize identity store: {e}"),
            })?;
        Ok(())
    }
}

/// Generate a fresh stable user identifier, matching the in-memory store's format so
/// the two backends are interchangeable and the value joins `auth.sessions.user_id`.
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
        tenant: Option<Uuid>,
        email: Option<&str>,
        email_verified: bool,
        provider: &str,
        provider_id: &str,
    ) -> Result<AccountLinkResult> {
        let mut tx = self.db.begin().await.map_err(|e| db_error("begin tx", &e))?;

        // 1. A known (provider, provider_id) in this account space is an idempotent re-login: same
        //    user, no new link. The per-space unique index makes this the authoritative lookup.
        if let Some(row) = sqlx::query(
            "SELECT user_id FROM auth.tb_auth_identity \
             WHERE provider = $1 AND provider_id = $2 AND tenant_id IS NOT DISTINCT FROM $3",
        )
        .bind(provider)
        .bind(provider_id)
        .bind(tenant)
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

        // 3. Find the email-keyed user in this account space, or create a fresh account.
        let (user_id, pk_user, is_new, linked) = if let Some(em) = verified_email.as_deref() {
            if let Some(row) = sqlx::query(
                "SELECT pk_user, user_id FROM auth.tb_user \
                 WHERE email = $1 AND tenant_id IS NOT DISTINCT FROM $2",
            )
            .bind(em)
            .bind(tenant)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| db_error("lookup user by email", &e))?
            {
                let pk_user: i64 = row.get("pk_user");
                let user_id: String = row.get("user_id");
                (user_id, pk_user, false, true)
            } else {
                let user_id = new_user_id();
                let pk_user = insert_user(&mut tx, &user_id, Some(em), tenant).await?;
                (user_id, pk_user, true, false)
            }
        } else {
            let user_id = new_user_id();
            let pk_user = insert_user(&mut tx, &user_id, None, tenant).await?;
            (user_id, pk_user, true, false)
        };

        // 4. Link the provider identity (new for this account by construction — step 1 ruled out an
        //    existing one).
        sqlx::query(
            "INSERT INTO auth.tb_auth_identity (fk_user, user_id, provider, provider_id, tenant_id) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(pk_user)
        .bind(&user_id)
        .bind(provider)
        .bind(provider_id)
        .bind(tenant)
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
            sqlx::query("SELECT email FROM auth.tb_user WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| db_error("lookup user", &e))?
                .ok_or(AuthError::TokenNotFound)?
                .get("email");

        let providers = sqlx::query(
            "SELECT provider, provider_id FROM auth.tb_auth_identity \
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

/// Insert a new user row in `tenant`'s account space and return its `pk_user`.
async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    email: Option<&str>,
    tenant: Option<Uuid>,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO auth.tb_user (user_id, email, tenant_id) VALUES ($1, $2, $3) RETURNING pk_user",
    )
    .bind(user_id)
    .bind(email)
    .bind(tenant)
    .fetch_one(&mut **tx)
            .await
            .map_err(|e| db_error("insert user", &e))?;
    Ok(row.get("pk_user"))
}
