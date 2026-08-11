//! Postgres storage for SCIM 2.0 provisioning (#946).
//!
//! SCIM users are `core.tb_user` rows — the *same* rows every credential path resolves to,
//! not a parallel directory. That is the whole point: an IdP deactivating a user has to
//! reach the account a local password or a social link would authenticate, or offboarding
//! is cosmetic.
//!
//! SCIM groups are RBAC roles. Membership is a role assignment. A provisioning credential
//! can therefore put a person *into* a role, which is what group-driven access needs, but
//! creating a group never grants it any permission — see [`ScimStore::create_group`].

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use uuid::Uuid;

use crate::error::{AuthError, Result};

/// Idempotent DDL for SCIM groups, membership and provisioning tokens (#946).
///
/// The user side needs no table of its own: SCIM users are `core.tb_user` rows, whose SCIM
/// columns are created by
/// [`PostgresAccountStore::init`](crate::PostgresAccountStore::init).
pub const PG_SCIM_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_scim_group (
    pk_scim_group BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id            UUID NOT NULL DEFAULT gen_random_uuid(),
    display_name  TEXT NOT NULL,
    external_id   TEXT,
    tenant_id     UUID,
    version       BIGINT NOT NULL DEFAULT 1,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_scim_group_id ON core.tb_scim_group (id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_scim_group_name
    ON core.tb_scim_group (tenant_id, display_name) NULLS NOT DISTINCT;

CREATE TABLE IF NOT EXISTS core.tb_scim_group_member (
    pk_scim_group_member BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fk_scim_group BIGINT NOT NULL REFERENCES core.tb_scim_group (pk_scim_group)
        ON DELETE CASCADE,
    user_id       TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (fk_scim_group, user_id)
);
CREATE INDEX IF NOT EXISTS idx_scim_group_member_user
    ON core.tb_scim_group_member (user_id);

-- Provisioning credentials. Distinct from the admin token by construction: nothing reads
-- this table except the SCIM router, so holding one grants provisioning and nothing else.
CREATE TABLE IF NOT EXISTS core.tb_scim_token (
    pk_scim_token BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id            UUID NOT NULL DEFAULT gen_random_uuid(),
    token_hash    TEXT NOT NULL UNIQUE,
    idp_name      TEXT NOT NULL,
    tenant_id     UUID,
    description   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_scim_token_live
    ON core.tb_scim_token (token_hash) WHERE revoked_at IS NULL;

ALTER TABLE core.tb_scim_group        ENABLE ROW LEVEL SECURITY;
ALTER TABLE core.tb_scim_group_member ENABLE ROW LEVEL SECURITY;
ALTER TABLE core.tb_scim_token        ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_scim_group_tenant_read ON core.tb_scim_group;
CREATE POLICY p_scim_group_tenant_read ON core.tb_scim_group
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_scim_group_insert ON core.tb_scim_group;
CREATE POLICY p_scim_group_insert ON core.tb_scim_group FOR INSERT WITH CHECK (true);

REVOKE ALL ON core.tb_scim_group        FROM PUBLIC;
REVOKE ALL ON core.tb_scim_group_member FROM PUBLIC;
REVOKE ALL ON core.tb_scim_token        FROM PUBLIC;
";

/// A provisioned user, as SCIM sees it. Backed by one `core.tb_user` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimUser {
    /// Stable identifier — the account-store `user_id`, so a SCIM `id` and the `sub` of
    /// every session for that person are the same string.
    pub id:           String,
    /// SCIM `userName`. Unique across the deployment.
    pub user_name:    String,
    /// SCIM `externalId` — the IdP's own key for this person.
    pub external_id:  Option<String>,
    /// Primary email address.
    pub email:        Option<String>,
    /// `name.givenName`.
    pub given_name:   Option<String>,
    /// `name.familyName`.
    pub family_name:  Option<String>,
    /// `displayName`.
    pub display_name: Option<String>,
    /// SCIM `active`. `false` revokes sessions and blocks sign-in.
    pub active:       bool,
    /// Monotonic row version behind `meta.version` / `ETag`.
    pub version:      i64,
    /// `meta.created`.
    pub created_at:   DateTime<Utc>,
    /// `meta.lastModified`.
    pub updated_at:   DateTime<Utc>,
}

/// A provisioned group. Backed by `core.tb_scim_group` and mirrored into an RBAC role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimGroup {
    /// Stable identifier.
    pub id:           Uuid,
    /// SCIM `displayName`. Unique within a tenant.
    pub display_name: String,
    /// SCIM `externalId`.
    pub external_id:  Option<String>,
    /// Member `user_id`s.
    pub members:      Vec<String>,
    /// Monotonic row version.
    pub version:      i64,
    /// `meta.created`.
    pub created_at:   DateTime<Utc>,
    /// `meta.lastModified`.
    pub updated_at:   DateTime<Utc>,
}

/// What a provisioning client supplies when creating or replacing a user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScimUserWrite {
    /// SCIM `userName`.
    pub user_name:    String,
    /// SCIM `externalId`.
    pub external_id:  Option<String>,
    /// Primary email.
    pub email:        Option<String>,
    /// `name.givenName`.
    pub given_name:   Option<String>,
    /// `name.familyName`.
    pub family_name:  Option<String>,
    /// `displayName`.
    pub display_name: Option<String>,
    /// SCIM `active`.
    pub active:       bool,
}

/// One page of a SCIM list response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimPage<T> {
    /// The requested slice.
    pub resources:     Vec<T>,
    /// Total matching the filter, before pagination — SCIM's `totalResults`.
    pub total_results: i64,
}

/// Provisioning storage.
// Reason: dyn-dispatched behind Arc so the SCIM router is backend-agnostic and testable;
// remove when RTN + Send is stable (RFC 3425).
#[async_trait]
pub trait ScimStore: Send + Sync {
    /// Create the SCIM schema (idempotent).
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] if the DDL fails.
    async fn init(&self) -> Result<()>;

    /// List users, optionally filtered by `userName eq "…"` and paginated.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] on failure.
    async fn list_users(
        &self,
        user_name: Option<&str>,
        start_index: i64,
        count: i64,
    ) -> Result<ScimPage<ScimUser>>;

    /// One user by `id`.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] on failure.
    async fn get_user(&self, id: &str) -> Result<Option<ScimUser>>;

    /// Create a user.
    ///
    /// # Errors
    ///
    /// [`AuthError::EmailAlreadyRegistered`] if the `userName` or email is taken —
    /// SCIM's `uniqueness` conflict; [`AuthError::DatabaseError`] otherwise.
    async fn create_user(&self, write: &ScimUserWrite) -> Result<ScimUser>;

    /// Replace a user (SCIM `PUT`).
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown; otherwise as
    /// [`create_user`](Self::create_user).
    async fn replace_user(&self, id: &str, write: &ScimUserWrite) -> Result<ScimUser>;

    /// Set a user's `active` flag, returning the updated user.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown.
    async fn set_user_active(&self, id: &str, active: bool) -> Result<ScimUser>;

    /// Delete a user and everything hanging off them.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown.
    async fn delete_user(&self, id: &str) -> Result<()>;

    /// List groups.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] on failure.
    async fn list_groups(
        &self,
        display_name: Option<&str>,
        start_index: i64,
        count: i64,
    ) -> Result<ScimPage<ScimGroup>>;

    /// One group by `id`.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] on failure.
    async fn get_group(&self, id: Uuid) -> Result<Option<ScimGroup>>;

    /// Create a group with the given members.
    ///
    /// # Errors
    ///
    /// [`AuthError::EmailAlreadyRegistered`] if the display name is taken in this tenant.
    async fn create_group(
        &self,
        display_name: &str,
        external_id: Option<&str>,
        members: &[String],
    ) -> Result<ScimGroup>;

    /// Replace a group's name and membership.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown.
    async fn replace_group(
        &self,
        id: Uuid,
        display_name: &str,
        external_id: Option<&str>,
        members: &[String],
    ) -> Result<ScimGroup>;

    /// Add and remove members without touching the rest of the group.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown.
    async fn patch_group_members(
        &self,
        id: Uuid,
        add: &[String],
        remove: &[String],
    ) -> Result<ScimGroup>;

    /// Delete a group. Membership rows cascade; the users themselves are untouched.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if `id` is unknown.
    async fn delete_group(&self, id: Uuid) -> Result<()>;

    /// Group display names a user belongs to — SCIM's `groups` sub-attribute, and the
    /// bridge to RBAC role assignment.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] on failure.
    async fn groups_of_user(&self, user_id: &str) -> Result<Vec<String>>;
}

/// PostgreSQL-backed [`ScimStore`], scoped to one tenant.
///
/// The tenant comes from the provisioning token, never from the request body, so one IdP's
/// credential cannot provision into another's tenant.
#[derive(Debug, Clone)]
pub struct PgScimStore {
    db:        PgPool,
    tenant_id: Option<Uuid>,
}

impl PgScimStore {
    /// Create a store bound to `tenant_id` (`None` = the untenanted deployment).
    #[must_use]
    pub const fn new(db: PgPool, tenant_id: Option<Uuid>) -> Self {
        Self { db, tenant_id }
    }

    /// Borrow the pool, for callers that must run in the same database (session
    /// revocation on deactivation).
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.db
    }
}

fn db_error(context: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("{context}: {e}"),
    }
}

/// `23505` = unique_violation: a `userName`, email or group name already taken. SCIM calls
/// this a `uniqueness` conflict and expects `409`.
fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

const fn user_columns() -> &'static str {
    "user_id, user_name, external_id, email, given_name, family_name, display_name, \
     active, version, created_at, updated_at"
}

fn decode_user(row: &sqlx::postgres::PgRow) -> ScimUser {
    ScimUser {
        id:           row.get("user_id"),
        user_name:    row.get::<Option<String>, _>("user_name").unwrap_or_default(),
        external_id:  row.get("external_id"),
        email:        row.get("email"),
        given_name:   row.get("given_name"),
        family_name:  row.get("family_name"),
        display_name: row.get("display_name"),
        active:       row.get("active"),
        version:      row.get("version"),
        created_at:   row.get("created_at"),
        updated_at:   row.get("updated_at"),
    }
}

/// A fresh account identifier in the account store's format, so a SCIM-provisioned user is
/// indistinguishable from one created by a login — which is what lets the two meet.
fn new_user_id() -> String {
    format!("user_{}", Uuid::new_v4().as_simple())
}

// Reason: ScimStore is declared with #[async_trait]; the impl must match its signatures.
#[async_trait]
impl ScimStore for PgScimStore {
    async fn init(&self) -> Result<()> {
        sqlx::raw_sql(PG_SCIM_SCHEMA_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("initialize SCIM schema", &e))?;
        Ok(())
    }

    async fn list_users(
        &self,
        user_name: Option<&str>,
        start_index: i64,
        count: i64,
    ) -> Result<ScimPage<ScimUser>> {
        // `start_index` is 1-based in SCIM; the offset is one less.
        let offset = (start_index - 1).max(0);
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM core.tb_user WHERE ($1::text IS NULL OR user_name = $1)",
        )
        .bind(user_name)
        .fetch_one(&self.db)
        .await
        .map_err(|e| db_error("count SCIM users", &e))?;

        let rows = sqlx::query(&format!(
            "SELECT {} FROM core.tb_user WHERE ($1::text IS NULL OR user_name = $1) \
             ORDER BY pk_user LIMIT $2 OFFSET $3",
            user_columns()
        ))
        .bind(user_name)
        .bind(count)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("list SCIM users", &e))?;

        Ok(ScimPage {
            resources:     rows.iter().map(decode_user).collect(),
            total_results: total,
        })
    }

    async fn get_user(&self, id: &str) -> Result<Option<ScimUser>> {
        let row =
            sqlx::query(&format!("SELECT {} FROM core.tb_user WHERE user_id = $1", user_columns()))
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| db_error("get SCIM user", &e))?;
        Ok(row.as_ref().map(decode_user))
    }

    async fn create_user(&self, write: &ScimUserWrite) -> Result<ScimUser> {
        let user_id = new_user_id();
        let row = sqlx::query(&format!(
            "INSERT INTO core.tb_user \
             (user_id, user_name, external_id, email, given_name, family_name, display_name, \
              active, tenant_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {}",
            user_columns()
        ))
        .bind(&user_id)
        .bind(&write.user_name)
        .bind(write.external_id.as_deref())
        .bind(write.email.as_deref())
        .bind(write.given_name.as_deref())
        .bind(write.family_name.as_deref())
        .bind(write.display_name.as_deref())
        .bind(write.active)
        .bind(self.tenant_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AuthError::EmailAlreadyRegistered
            } else {
                db_error("create SCIM user", &e)
            }
        })?;
        Ok(decode_user(&row))
    }

    async fn replace_user(&self, id: &str, write: &ScimUserWrite) -> Result<ScimUser> {
        let row = sqlx::query(&format!(
            "UPDATE core.tb_user SET user_name = $2, external_id = $3, email = $4, \
             given_name = $5, family_name = $6, display_name = $7, active = $8, \
             version = version + 1, updated_at = now() \
             WHERE user_id = $1 RETURNING {}",
            user_columns()
        ))
        .bind(id)
        .bind(&write.user_name)
        .bind(write.external_id.as_deref())
        .bind(write.email.as_deref())
        .bind(write.given_name.as_deref())
        .bind(write.family_name.as_deref())
        .bind(write.display_name.as_deref())
        .bind(write.active)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AuthError::EmailAlreadyRegistered
            } else {
                db_error("replace SCIM user", &e)
            }
        })?
        .ok_or(AuthError::TokenNotFound)?;
        Ok(decode_user(&row))
    }

    async fn set_user_active(&self, id: &str, active: bool) -> Result<ScimUser> {
        let row = sqlx::query(&format!(
            "UPDATE core.tb_user SET active = $2, version = version + 1, updated_at = now() \
             WHERE user_id = $1 RETURNING {}",
            user_columns()
        ))
        .bind(id)
        .bind(active)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("set SCIM user active", &e))?
        .ok_or(AuthError::TokenNotFound)?;
        Ok(decode_user(&row))
    }

    async fn delete_user(&self, id: &str) -> Result<()> {
        let affected = sqlx::query("DELETE FROM core.tb_user WHERE user_id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("delete SCIM user", &e))?
            .rows_affected();
        if affected == 0 {
            return Err(AuthError::TokenNotFound);
        }
        Ok(())
    }

    async fn list_groups(
        &self,
        display_name: Option<&str>,
        start_index: i64,
        count: i64,
    ) -> Result<ScimPage<ScimGroup>> {
        let offset = (start_index - 1).max(0);
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM core.tb_scim_group \
             WHERE tenant_id IS NOT DISTINCT FROM $1 \
               AND ($2::text IS NULL OR display_name = $2)",
        )
        .bind(self.tenant_id)
        .bind(display_name)
        .fetch_one(&self.db)
        .await
        .map_err(|e| db_error("count SCIM groups", &e))?;

        let rows = sqlx::query(
            "SELECT pk_scim_group, id, display_name, external_id, version, created_at, \
             updated_at FROM core.tb_scim_group \
             WHERE tenant_id IS NOT DISTINCT FROM $1 \
               AND ($2::text IS NULL OR display_name = $2) \
             ORDER BY pk_scim_group LIMIT $3 OFFSET $4",
        )
        .bind(self.tenant_id)
        .bind(display_name)
        .bind(count)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("list SCIM groups", &e))?;

        let mut resources = Vec::with_capacity(rows.len());
        for row in &rows {
            resources.push(self.hydrate_group(row).await?);
        }
        Ok(ScimPage {
            resources,
            total_results: total,
        })
    }

    async fn get_group(&self, id: Uuid) -> Result<Option<ScimGroup>> {
        let row = sqlx::query(
            "SELECT pk_scim_group, id, display_name, external_id, version, created_at, \
             updated_at FROM core.tb_scim_group \
             WHERE id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
        )
        .bind(id)
        .bind(self.tenant_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("get SCIM group", &e))?;
        match row {
            Some(row) => Ok(Some(self.hydrate_group(&row).await?)),
            None => Ok(None),
        }
    }

    async fn create_group(
        &self,
        display_name: &str,
        external_id: Option<&str>,
        members: &[String],
    ) -> Result<ScimGroup> {
        let row = sqlx::query(
            "INSERT INTO core.tb_scim_group (display_name, external_id, tenant_id) \
             VALUES ($1, $2, $3) \
             RETURNING pk_scim_group, id, display_name, external_id, version, created_at, \
             updated_at",
        )
        .bind(display_name)
        .bind(external_id)
        .bind(self.tenant_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AuthError::EmailAlreadyRegistered
            } else {
                db_error("create SCIM group", &e)
            }
        })?;
        let pk: i64 = row.get("pk_scim_group");
        self.set_members(pk, members).await?;
        self.hydrate_group(&row).await
    }

    async fn replace_group(
        &self,
        id: Uuid,
        display_name: &str,
        external_id: Option<&str>,
        members: &[String],
    ) -> Result<ScimGroup> {
        let row = sqlx::query(
            "UPDATE core.tb_scim_group SET display_name = $3, external_id = $4, \
             version = version + 1, updated_at = now() \
             WHERE id = $1 AND tenant_id IS NOT DISTINCT FROM $2 \
             RETURNING pk_scim_group, id, display_name, external_id, version, created_at, \
             updated_at",
        )
        .bind(id)
        .bind(self.tenant_id)
        .bind(display_name)
        .bind(external_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AuthError::EmailAlreadyRegistered
            } else {
                db_error("replace SCIM group", &e)
            }
        })?
        .ok_or(AuthError::TokenNotFound)?;
        let pk: i64 = row.get("pk_scim_group");
        sqlx::query("DELETE FROM core.tb_scim_group_member WHERE fk_scim_group = $1")
            .bind(pk)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("clear SCIM group members", &e))?;
        self.set_members(pk, members).await?;
        self.hydrate_group(&row).await
    }

    async fn patch_group_members(
        &self,
        id: Uuid,
        add: &[String],
        remove: &[String],
    ) -> Result<ScimGroup> {
        let row = sqlx::query(
            "UPDATE core.tb_scim_group SET version = version + 1, updated_at = now() \
             WHERE id = $1 AND tenant_id IS NOT DISTINCT FROM $2 \
             RETURNING pk_scim_group, id, display_name, external_id, version, created_at, \
             updated_at",
        )
        .bind(id)
        .bind(self.tenant_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("patch SCIM group", &e))?
        .ok_or(AuthError::TokenNotFound)?;
        let pk: i64 = row.get("pk_scim_group");

        if !remove.is_empty() {
            sqlx::query(
                "DELETE FROM core.tb_scim_group_member \
                 WHERE fk_scim_group = $1 AND user_id = ANY($2)",
            )
            .bind(pk)
            .bind(remove)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("remove SCIM group members", &e))?;
        }
        self.set_members(pk, add).await?;
        self.hydrate_group(&row).await
    }

    async fn delete_group(&self, id: Uuid) -> Result<()> {
        let affected = sqlx::query(
            "DELETE FROM core.tb_scim_group WHERE id = $1 AND tenant_id IS NOT DISTINCT FROM $2",
        )
        .bind(id)
        .bind(self.tenant_id)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("delete SCIM group", &e))?
        .rows_affected();
        if affected == 0 {
            return Err(AuthError::TokenNotFound);
        }
        Ok(())
    }

    async fn groups_of_user(&self, user_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT g.display_name FROM core.tb_scim_group_member m \
             JOIN core.tb_scim_group g ON g.pk_scim_group = m.fk_scim_group \
             WHERE m.user_id = $1 AND g.tenant_id IS NOT DISTINCT FROM $2 \
             ORDER BY g.display_name",
        )
        .bind(user_id)
        .bind(self.tenant_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("list groups of user", &e))?;
        Ok(rows.iter().map(|r| r.get("display_name")).collect())
    }
}

impl PgScimStore {
    /// Insert membership rows, ignoring duplicates so PATCH-add is idempotent.
    async fn set_members(&self, pk_group: i64, members: &[String]) -> Result<()> {
        for user_id in members {
            sqlx::query(
                "INSERT INTO core.tb_scim_group_member (fk_scim_group, user_id) \
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(pk_group)
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(|e| db_error("add SCIM group member", &e))?;
        }
        Ok(())
    }

    /// Attach the membership list to a group row.
    async fn hydrate_group(&self, row: &sqlx::postgres::PgRow) -> Result<ScimGroup> {
        let pk: i64 = row.get("pk_scim_group");
        let members = sqlx::query(
            "SELECT user_id FROM core.tb_scim_group_member \
             WHERE fk_scim_group = $1 ORDER BY pk_scim_group_member",
        )
        .bind(pk)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("list SCIM group members", &e))?;

        Ok(ScimGroup {
            id:           row.get("id"),
            display_name: row.get("display_name"),
            external_id:  row.get("external_id"),
            members:      members.iter().map(|r| r.get("user_id")).collect(),
            version:      row.get("version"),
            created_at:   row.get("created_at"),
            updated_at:   row.get("updated_at"),
        })
    }
}
