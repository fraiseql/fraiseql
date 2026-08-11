//! Durable per-tenant SAML IdP storage (#947).
//!
//! The `[saml.idps.*]` config-file path resolves IdPs once at boot, which makes three
//! things impossible: adding or rotating an IdP without a restart, letting each tenant
//! manage its own, and scoping `/auth/saml/login` so one tenant cannot start a login with
//! another's IdP. This module is the storage half; [`super::registry`] composes it with the
//! config-file IdPs and owns resolution.
//!
//! # Why the IdP name is globally unique, and why deletes are soft
//!
//! The logical IdP name *is* the account-store provider namespace: an assertion from
//! `okta` links onto `("saml:okta", NameID)` (see [`super::saml_provider_key`]). Two tenants
//! both naming their IdP `okta` would therefore share one identity namespace, and a
//! `NameID` collision across those two IdPs would resolve to a **single account** — a
//! cross-tenant takeover with no attacker sophistication required. So the name is unique
//! across every tenant, and a collision is refused at write time rather than silently
//! merged.
//!
//! Uniqueness among *live* rows is not enough. If `acme` deleted `okta` and `globex` then
//! created `okta`, the new IdP would inherit every `saml:okta` identity row `acme` left
//! behind — the same namespace-recycling defect, just delayed. Deletion is therefore a
//! tombstone (`deleted_at`) and the unique index spans live and deleted rows alike: a name,
//! once used, is never reissued.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use uuid::Uuid;

use super::SamlError;

/// Idempotent DDL for the per-tenant IdP store. Exposed so a migration runner can apply it
/// explicitly; [`PgSamlIdpStore::init`] runs the same statements.
///
/// The unique index deliberately carries **no** `WHERE deleted_at IS NULL` predicate — see
/// the module docs for why a deleted name must never be reissued.
pub const PG_SAML_IDP_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;

CREATE TABLE IF NOT EXISTS core.tb_saml_idp (
    pk_saml_idp            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id                     UUID NOT NULL DEFAULT gen_random_uuid(),
    idp_name               TEXT NOT NULL,
    tenant_id              UUID,
    sp_entity_id           TEXT NOT NULL,
    acs_url                TEXT NOT NULL,
    metadata_xml           TEXT NOT NULL,
    idp_entity_id          TEXT NOT NULL,
    trust_asserted_email   BOOLEAN NOT NULL DEFAULT FALSE,
    certificate_expires_at TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at             TIMESTAMPTZ
);

-- Spans deleted rows on purpose: a retired name must never be reissued to another
-- tenant, or the new IdP inherits the old one's `saml:<name>` identity rows.
CREATE UNIQUE INDEX IF NOT EXISTS uq_saml_idp_name ON core.tb_saml_idp (idp_name);
CREATE INDEX IF NOT EXISTS idx_saml_idp_tenant ON core.tb_saml_idp (tenant_id)
    WHERE deleted_at IS NULL;

-- RLS deny-by-default, mirroring core.tb_user / core.tb_auth_identity. ENABLE not FORCE:
-- the owner (this store, running the trusted admin path) operates freely, while any other
-- role reads a row only once it has set fraiseql.tenant_id to that row's tenant.
ALTER TABLE core.tb_saml_idp ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS p_saml_idp_tenant_read ON core.tb_saml_idp;
CREATE POLICY p_saml_idp_tenant_read ON core.tb_saml_idp
    FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid);
DROP POLICY IF EXISTS p_saml_idp_insert ON core.tb_saml_idp;
CREATE POLICY p_saml_idp_insert ON core.tb_saml_idp FOR INSERT WITH CHECK (true);

REVOKE ALL ON core.tb_saml_idp FROM PUBLIC;
";

/// A stored IdP as the operator declared it, plus what parsing its metadata revealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamlIdpRecord {
    /// Surrogate identifier, stable across renames of nothing (the name is immutable).
    pub id:                     Uuid,
    /// Logical IdP name — globally unique, and the `saml:<name>` provider namespace.
    pub idp_name:               String,
    /// Tenant this IdP provisions for. `None` = an untenanted (single-tenant) IdP.
    pub tenant_id:              Option<Uuid>,
    /// SP entity ID (the audience the IdP asserts to).
    pub sp_entity_id:           String,
    /// Assertion Consumer Service URL.
    pub acs_url:                String,
    /// The IdP's SAML metadata XML.
    pub metadata_xml:           String,
    /// IdP entity ID, parsed out of the metadata.
    pub idp_entity_id:          String,
    /// Whether a verified assertion's email may be used as a cross-provider linking key.
    ///
    /// Stored, but subject to [`super::effective_saml_email_verified`] — which keeps
    /// returning `false` for every tenant-bound IdP while the account store keys verified
    /// email globally. For a stored, tenant-bound IdP this flag is therefore recorded and
    /// **inert**; see that function's docs and #1088.
    pub trust_asserted_email:   bool,
    /// Earliest `NotAfter` among the IdP's signing certificates, parsed from the metadata.
    pub certificate_expires_at: Option<DateTime<Utc>>,
    /// Row creation time.
    pub created_at:             DateTime<Utc>,
    /// Last update time.
    pub updated_at:             DateTime<Utc>,
}

/// What an operator supplies to create or update an IdP. The parsed fields
/// (`idp_entity_id`, `certificate_expires_at`) are derived, never accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamlIdpSpec {
    /// Logical IdP name.
    pub idp_name:             String,
    /// Tenant binding (`None` = untenanted).
    pub tenant_id:            Option<Uuid>,
    /// SP entity ID.
    pub sp_entity_id:         String,
    /// Assertion Consumer Service URL.
    pub acs_url:              String,
    /// IdP metadata XML.
    pub metadata_xml:         String,
    /// Email-linking opt-in (see [`SamlIdpRecord::trust_asserted_email`]).
    pub trust_asserted_email: bool,
}

/// Durable IdP storage.
///
/// Every write validates the metadata through the *same* [`super::SamlIdpConfigBuilder`]
/// path boot uses, so a stored IdP can never be in a shape that boot would have refused —
/// which is the whole point of the fail-loud parse rules being in the builder.
// Reason: dyn-dispatched behind Arc so the registry is backend-agnostic; remove when RTN +
// Send is stable (RFC 3425).
#[async_trait]
pub trait SamlIdpStore: Send + Sync {
    /// Every live IdP, ordered by name.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if the backing store fails.
    async fn list(&self) -> Result<Vec<SamlIdpRecord>, SamlError>;

    /// One live IdP by name, or `None`.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if the backing store fails.
    async fn get(&self, idp_name: &str) -> Result<Option<SamlIdpRecord>, SamlError>;

    /// Create an IdP.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if the metadata does not parse or declares no HTTP-Redirect
    /// SSO binding; [`SamlError::NameTaken`] if the name is already in use — including by a
    /// deleted IdP, whose namespace must never be reissued; [`SamlError::Store`] otherwise.
    async fn create(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError>;

    /// Replace a live IdP's metadata and policy. The name and tenant are immutable: both
    /// are identity, not settings, and changing either would silently rehome existing
    /// `saml:<name>` accounts.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if the metadata does not parse; [`SamlError::NotFound`] if no
    /// live IdP has that name; [`SamlError::Store`] otherwise.
    async fn update(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError>;

    /// Tombstone a live IdP. It stops serving; its name stays reserved forever.
    ///
    /// # Errors
    ///
    /// [`SamlError::NotFound`] if no live IdP has that name; [`SamlError::Store`] otherwise.
    async fn delete(&self, idp_name: &str) -> Result<(), SamlError>;
}

/// PostgreSQL-backed [`SamlIdpStore`].
#[derive(Debug, Clone)]
pub struct PgSamlIdpStore {
    db: PgPool,
}

/// Columns every read projects, in one place so the row decoder cannot drift from them.
const COLUMNS: &str = "id, idp_name, tenant_id, sp_entity_id, acs_url, metadata_xml, \
                       idp_entity_id, trust_asserted_email, certificate_expires_at, \
                       created_at, updated_at";

impl PgSamlIdpStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create `core.tb_saml_idp` (idempotent). Call once on startup.
    ///
    /// # Errors
    ///
    /// [`SamlError::Store`] if the DDL fails.
    pub async fn init(&self) -> Result<(), SamlError> {
        sqlx::raw_sql(PG_SAML_IDP_SCHEMA_SQL)
            .execute(&self.db)
            .await
            .map_err(|e| SamlError::Store(format!("initialize SAML IdP store: {e}")))?;
        Ok(())
    }

    fn decode(row: &sqlx::postgres::PgRow) -> SamlIdpRecord {
        SamlIdpRecord {
            id:                     row.get("id"),
            idp_name:               row.get("idp_name"),
            tenant_id:              row.get("tenant_id"),
            sp_entity_id:           row.get("sp_entity_id"),
            acs_url:                row.get("acs_url"),
            metadata_xml:           row.get("metadata_xml"),
            idp_entity_id:          row.get("idp_entity_id"),
            trust_asserted_email:   row.get("trust_asserted_email"),
            certificate_expires_at: row.get("certificate_expires_at"),
            created_at:             row.get("created_at"),
            updated_at:             row.get("updated_at"),
        }
    }
}

/// Validate a spec through the production builder and return what parsing revealed.
///
/// This is the fail-loud gate: metadata that boot would refuse (unparseable, or with no
/// HTTP-Redirect SSO binding) is refused here too, so no write can land a row whose only
/// possible behaviour is a 500 at login.
fn derive(spec: &SamlIdpSpec) -> Result<(String, Option<DateTime<Utc>>), SamlError> {
    let config = super::SamlIdpConfig::builder(
        spec.idp_name.clone(),
        spec.sp_entity_id.clone(),
        spec.acs_url.clone(),
    )
    .idp_metadata_xml(&spec.metadata_xml)?
    .tenant_id(spec.tenant_id.map(|t| t.to_string()))
    .trust_asserted_email(spec.trust_asserted_email)
    .build()?;

    Ok((config.idp_entity_id().to_string(), config.signing_certificate_expiry()))
}

// Reason: SamlIdpStore is defined with #[async_trait]; the impl must match its transformed
// signatures.
#[async_trait]
impl SamlIdpStore for PgSamlIdpStore {
    async fn list(&self) -> Result<Vec<SamlIdpRecord>, SamlError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM core.tb_saml_idp WHERE deleted_at IS NULL ORDER BY idp_name"
        ))
        .fetch_all(&self.db)
        .await
        .map_err(|e| SamlError::Store(format!("list SAML IdPs: {e}")))?;
        Ok(rows.iter().map(Self::decode).collect())
    }

    async fn get(&self, idp_name: &str) -> Result<Option<SamlIdpRecord>, SamlError> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM core.tb_saml_idp WHERE idp_name = $1 AND deleted_at IS NULL"
        ))
        .bind(idp_name)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| SamlError::Store(format!("get SAML IdP: {e}")))?;
        Ok(row.as_ref().map(Self::decode))
    }

    async fn create(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError> {
        let (idp_entity_id, expires_at) = derive(spec)?;

        let row = sqlx::query(&format!(
            "INSERT INTO core.tb_saml_idp \
             (idp_name, tenant_id, sp_entity_id, acs_url, metadata_xml, idp_entity_id, \
              trust_asserted_email, certificate_expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {COLUMNS}"
        ))
        .bind(&spec.idp_name)
        .bind(spec.tenant_id)
        .bind(&spec.sp_entity_id)
        .bind(&spec.acs_url)
        .bind(&spec.metadata_xml)
        .bind(&idp_entity_id)
        .bind(spec.trust_asserted_email)
        .bind(expires_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| match &e {
            // 23505 = unique_violation. The only unique index is on idp_name, and it spans
            // tombstones, so this is exactly "that namespace is spoken for".
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
                SamlError::NameTaken(spec.idp_name.clone())
            },
            _ => SamlError::Store(format!("create SAML IdP: {e}")),
        })?;
        Ok(Self::decode(&row))
    }

    async fn update(&self, spec: &SamlIdpSpec) -> Result<SamlIdpRecord, SamlError> {
        let (idp_entity_id, expires_at) = derive(spec)?;

        let row = sqlx::query(&format!(
            "UPDATE core.tb_saml_idp SET sp_entity_id = $2, acs_url = $3, metadata_xml = $4, \
             idp_entity_id = $5, trust_asserted_email = $6, certificate_expires_at = $7, \
             updated_at = now() \
             WHERE idp_name = $1 AND deleted_at IS NULL RETURNING {COLUMNS}"
        ))
        .bind(&spec.idp_name)
        .bind(&spec.sp_entity_id)
        .bind(&spec.acs_url)
        .bind(&spec.metadata_xml)
        .bind(&idp_entity_id)
        .bind(spec.trust_asserted_email)
        .bind(expires_at)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| SamlError::Store(format!("update SAML IdP: {e}")))?
        .ok_or_else(|| SamlError::NotFound(spec.idp_name.clone()))?;
        Ok(Self::decode(&row))
    }

    async fn delete(&self, idp_name: &str) -> Result<(), SamlError> {
        let affected = sqlx::query(
            "UPDATE core.tb_saml_idp SET deleted_at = now(), updated_at = now() \
                         WHERE idp_name = $1 AND deleted_at IS NULL",
        )
        .bind(idp_name)
        .execute(&self.db)
        .await
        .map_err(|e| SamlError::Store(format!("delete SAML IdP: {e}")))?
        .rows_affected();
        if affected == 0 {
            return Err(SamlError::NotFound(idp_name.to_string()));
        }
        Ok(())
    }
}
