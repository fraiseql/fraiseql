//! Who is administering: the deployment, or one tenant (#1089).
//!
//! The deployment `admin_token` is one credential with no tenant. A router that takes the
//! tenant as a request field and is gated only by that token cannot be handed to a tenant:
//! whoever holds the token manages every tenant. This module adds the second principal:
//!
//! - [`AdminPrincipal::Platform`]: the deployment `admin_token`. Unchanged behaviour.
//! - [`AdminPrincipal::Tenant`]: a **tenant admin token**, minted by the platform admin at
//!   `/api/admin-tokens` and confined to one tenant by the credential, never by the request. The
//!   design follows the SCIM provisioning token (#946).
//!
//! [`admin_principal_middleware`](crate::middleware::admin_principal_middleware) authenticates
//! either credential and inserts the principal. A router opts in to tenant principals by
//! mounting behind it. Every other admin router keeps the platform-only bearer gate, where a
//! tenant admin token is just a wrong token, so a router added later cannot inherit tenant
//! access by accident.
//!
//! A router that opts in enforces the principal with the two helpers below. Both intersect;
//! neither ever widens:
//!
//! - [`AdminPrincipal::scope`] for a tenant named in the request;
//! - [`AdminPrincipal::may_see`] for a row loaded by id.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Row as _};
use subtle::ConstantTimeEq as _;
use uuid::Uuid;

/// The authenticated administrator of a request to a tenant-aware admin router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminPrincipal {
    /// The deployment `admin_token`: every tenant, and the platform's own rows.
    Platform,
    /// A tenant admin token: this tenant's rows, and nothing else.
    Tenant(Uuid),
}

/// A request named a tenant its principal does not administer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignTenant;

impl AdminPrincipal {
    /// The tenant a request may act on, given the tenant it names.
    ///
    /// The platform gets what it asked for (`None` = every tenant, as before #1089). A tenant
    /// principal gets its own tenant whether it named it or named none, and is refused if it
    /// named another one.
    ///
    /// # Errors
    ///
    /// [`ForeignTenant`] when a tenant principal names a different tenant.
    pub fn scope(self, requested: Option<Uuid>) -> Result<Option<Uuid>, ForeignTenant> {
        match (self, requested) {
            (Self::Platform, requested) => Ok(requested),
            (Self::Tenant(own), None) => Ok(Some(own)),
            (Self::Tenant(own), Some(named)) if named == own => Ok(Some(own)),
            (Self::Tenant(_), Some(_)) => Err(ForeignTenant),
        }
    }

    /// Whether this principal may see a row owned by `row_tenant` (`None` = a platform row).
    ///
    /// A router answers `404` when this is false, exactly as for a missing row, so a tenant
    /// cannot probe for another tenant's identifiers.
    #[must_use]
    pub fn may_see(self, row_tenant: Option<Uuid>) -> bool {
        match self {
            Self::Platform => true,
            Self::Tenant(own) => row_tenant == Some(own),
        }
    }

    /// Whether this is the deployment administrator.
    #[must_use]
    pub const fn is_platform(self) -> bool {
        matches!(self, Self::Platform)
    }
}

/// The `403` a router answers for [`ForeignTenant`].
#[must_use]
pub fn foreign_tenant_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "this credential administers a different tenant" })),
    )
        .into_response()
}

// ─── Credential store ─────────────────────────────────────────────────────────

/// Bytes of entropy in a minted tenant admin token.
const TOKEN_BYTES: usize = 32;

/// Prefix of every tenant admin token, so a leaked one is recognisable to secret scanners
/// and to an operator reading a log.
pub const TOKEN_PREFIX: &str = "fraiseql_ta_";

/// Idempotent DDL for the tenant admin credential table.
pub const SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

-- Tenant admin credentials (#1089). Only sha256(token) is stored, so reading the table
-- cannot yield a usable credential. Every row is confined to one tenant: a deployment-wide
-- administrator is the admin_token, never a row here.
CREATE TABLE IF NOT EXISTS core.tb_admin_token (
    pk_admin_token BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id             UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    token_hash     TEXT NOT NULL UNIQUE,
    tenant_id      UUID NOT NULL,
    description    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at   TIMESTAMPTZ,
    revoked_at     TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_admin_token_tenant ON core.tb_admin_token (tenant_id);

-- Never world-readable; only the server's own role touches it.
REVOKE ALL ON core.tb_admin_token FROM PUBLIC;
";

/// A tenant admin credential, as the platform admin sees it. Never carries the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminTokenRecord {
    /// Identifier used to revoke it.
    pub id:           Uuid,
    /// The tenant it administers.
    pub tenant_id:    Uuid,
    /// Operator note.
    pub description:  Option<String>,
    /// Creation time.
    pub created_at:   DateTime<Utc>,
    /// Last successful authentication.
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Postgres-backed tenant admin credential store.
#[derive(Debug, Clone)]
pub struct PgAdminTokenStore {
    db: PgPool,
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest as _, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

impl PgAdminTokenStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create the credential table (idempotent).
    ///
    /// # Errors
    ///
    /// The database error if the DDL fails.
    pub async fn ensure_schema(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(SCHEMA_SQL).execute(&self.db).await.map(|_| ())
    }

    /// Mint a credential for `tenant_id`. Returns the record and the token, which is shown
    /// exactly once.
    ///
    /// # Errors
    ///
    /// The database error if the insert fails.
    pub async fn mint(
        &self,
        tenant_id: Uuid,
        description: Option<&str>,
    ) -> Result<(AdminTokenRecord, String), sqlx::Error> {
        use base64::Engine as _;
        use rand::RngCore as _;

        let mut bytes = [0u8; TOKEN_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        let token = format!(
            "{TOKEN_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        );
        let row = sqlx::query(
            "INSERT INTO core.tb_admin_token (token_hash, tenant_id, description) \
             VALUES ($1, $2, $3) \
             RETURNING id, tenant_id, description, created_at, last_used_at",
        )
        .bind(hash_token(&token))
        .bind(tenant_id)
        .bind(description)
        .fetch_one(&self.db)
        .await?;
        Ok((decode(&row), token))
    }

    /// The tenant a presented token administers, or `None` for an unknown or revoked one.
    ///
    /// The lookup is by hash and the final comparison is constant-time, as for SCIM tokens.
    ///
    /// # Errors
    ///
    /// The database error if the lookup fails.
    pub async fn authenticate(&self, token: &str) -> Result<Option<Uuid>, sqlx::Error> {
        if !token.starts_with(TOKEN_PREFIX) {
            return Ok(None);
        }
        let presented = hash_token(token);
        let Some(row) = sqlx::query(
            "SELECT id, token_hash, tenant_id FROM core.tb_admin_token \
             WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&presented)
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let stored: String = row.get("token_hash");
        if stored.as_bytes().ct_eq(presented.as_bytes()).unwrap_u8() != 1 {
            return Ok(None);
        }
        let id: Uuid = row.get("id");
        // Best-effort: a failed touch must not fail an otherwise-valid authentication.
        let _ = sqlx::query("UPDATE core.tb_admin_token SET last_used_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await;
        Ok(Some(row.get("tenant_id")))
    }

    /// Live credentials, optionally for one tenant.
    ///
    /// # Errors
    ///
    /// The database error if the read fails.
    pub async fn list(
        &self,
        tenant_id: Option<Uuid>,
    ) -> Result<Vec<AdminTokenRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, description, created_at, last_used_at \
             FROM core.tb_admin_token \
             WHERE revoked_at IS NULL AND ($1::uuid IS NULL OR tenant_id = $1) \
             ORDER BY created_at",
        )
        .bind(tenant_id)
        .fetch_all(&self.db)
        .await?;
        Ok(rows.iter().map(decode).collect())
    }

    /// Revoke a credential. Returns whether a live one was revoked.
    ///
    /// # Errors
    ///
    /// The database error if the update fails.
    pub async fn revoke(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let affected = sqlx::query(
            "UPDATE core.tb_admin_token SET revoked_at = now() \
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.db)
        .await?
        .rows_affected();
        Ok(affected > 0)
    }
}

fn decode(row: &sqlx::postgres::PgRow) -> AdminTokenRecord {
    AdminTokenRecord {
        id:           row.get("id"),
        tenant_id:    row.get("tenant_id"),
        description:  row.get("description"),
        created_at:   row.get("created_at"),
        last_used_at: row.get("last_used_at"),
    }
}

// ─── Platform-only management routes ──────────────────────────────────────────

/// State for `/api/admin-tokens`.
#[derive(Clone)]
pub struct AdminTokenManagementState {
    /// The credential store.
    pub tokens: Arc<PgAdminTokenStore>,
}

/// `POST /api/admin-tokens` body.
///
/// `tenant_id` is required: a tenant admin token without a tenant would be a second
/// deployment-wide credential. `deny_unknown_fields`, so a misspelled key is a `422`, not a
/// silently dropped one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintAdminTokenRequest {
    /// The tenant the credential administers.
    pub tenant_id:   Uuid,
    /// Operator note.
    #[serde(default)]
    pub description: Option<String>,
}

/// `GET /api/admin-tokens` query.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAdminTokensQuery {
    /// Only this tenant's credentials.
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
}

/// The router that mints, lists and revokes tenant admin credentials.
///
/// Mount it behind the **platform-only** bearer gate: only the deployment administrator
/// creates tenant administrators.
pub fn admin_token_management_router(state: AdminTokenManagementState) -> Router {
    Router::new()
        .route("/api/admin-tokens", axum::routing::post(mint).get(list))
        .route("/api/admin-tokens/{id}", axum::routing::delete(revoke))
        .with_state(Arc::new(state))
}

fn store_failure(context: &str, e: &sqlx::Error) -> Response {
    tracing::error!(error = %e, "{context}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": context }))).into_response()
}

fn record_json(r: &AdminTokenRecord) -> serde_json::Value {
    json!({
        "id":           r.id,
        "tenant_id":    r.tenant_id,
        "description":  r.description,
        "created_at":   r.created_at,
        "last_used_at": r.last_used_at,
    })
}

async fn mint(
    State(state): State<Arc<AdminTokenManagementState>>,
    Json(body): Json<MintAdminTokenRequest>,
) -> Response {
    match state.tokens.mint(body.tenant_id, body.description.as_deref()).await {
        Ok((record, token)) => {
            let mut out = record_json(&record);
            // Shown exactly once: only sha256(token) is stored.
            out["token"] = json!(token);
            (StatusCode::CREATED, Json(out)).into_response()
        },
        Err(e) => store_failure("could not mint tenant admin token", &e),
    }
}

async fn list(
    State(state): State<Arc<AdminTokenManagementState>>,
    axum::extract::Query(q): axum::extract::Query<ListAdminTokensQuery>,
) -> Response {
    match state.tokens.list(q.tenant_id).await {
        Ok(records) => Json(json!({
            "total":  records.len(),
            "tokens": records.iter().map(record_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => store_failure("could not list tenant admin tokens", &e),
    }
}

async fn revoke(
    State(state): State<Arc<AdminTokenManagementState>>,
    Path(id): Path<String>,
) -> Response {
    let not_found =
        || (StatusCode::NOT_FOUND, Json(json!({ "error": "no such token" }))).into_response();
    let Ok(id) = Uuid::parse_str(&id) else {
        return not_found();
    };
    match state.tokens.revoke(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(e) => store_failure("could not revoke tenant admin token", &e),
    }
}

#[cfg(test)]
mod tests;
