//! SCIM provisioning credentials (#946).
//!
//! A provisioning token is **not** the admin token. The issue is explicit about why: an IdP
//! is handed this credential and configured to use it forever, so if it doubled as the admin
//! bearer, every SCIM integration would also carry the ability to rewrite roles and IdPs.
//! Separation here is structural — nothing but the SCIM router reads this table.
//!
//! Storage follows the API-key discipline (#627): only `sha256(token)` is persisted, so a
//! database disclosure cannot be replayed, and the lookup is by hash with a constant-time
//! comparison so it cannot be turned into a timing oracle.

use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use subtle::ConstantTimeEq as _;
use uuid::Uuid;

use crate::error::{AuthError, Result};

/// Bytes of entropy in a minted provisioning token.
const TOKEN_BYTES: usize = 32;

/// A provisioning credential, as the admin API reports it. Never carries the secret except
/// in [`MintedScimToken`], returned exactly once at creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimTokenRecord {
    /// Surrogate identifier used to revoke it.
    pub id:           Uuid,
    /// The IdP this credential provisions for.
    pub idp_name:     String,
    /// Tenant every operation under this credential is scoped to.
    pub tenant_id:    Option<Uuid>,
    /// Operator note.
    pub description:  Option<String>,
    /// Creation time.
    pub created_at:   DateTime<Utc>,
    /// Last successful authentication, for spotting a credential that has gone quiet.
    pub last_used_at: Option<DateTime<Utc>>,
}

/// A freshly minted token: the record plus the one and only sight of the secret.
#[derive(Debug, Clone)]
pub struct MintedScimToken {
    /// The stored record.
    pub record: ScimTokenRecord,
    /// The bearer token, shown once. Not persisted in this form.
    pub token:  String,
}

/// What a request proved by presenting a valid token: which IdP, and which tenant every
/// operation is confined to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimPrincipal {
    /// The IdP this credential belongs to.
    pub idp_name:  String,
    /// The tenant scope. Taken from the credential, **never** from the request, so one
    /// IdP cannot provision into another's tenant.
    pub tenant_id: Option<Uuid>,
}

/// Postgres-backed provisioning-token store.
#[derive(Debug, Clone)]
pub struct PgScimTokenStore {
    db: PgPool,
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn db_error(context: &str, e: &sqlx::Error) -> AuthError {
    AuthError::DatabaseError {
        message: format!("{context}: {e}"),
    }
}

impl PgScimTokenStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Mint a provisioning token for `idp_name`, scoped to `tenant_id`.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] if the insert fails.
    pub async fn mint(
        &self,
        idp_name: &str,
        tenant_id: Option<Uuid>,
        description: Option<&str>,
    ) -> Result<MintedScimToken> {
        use base64::Engine as _;
        use rand::RngCore as _;

        let mut bytes = [0u8; TOKEN_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

        let row = sqlx::query(
            "INSERT INTO core.tb_scim_token (token_hash, idp_name, tenant_id, description) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, idp_name, tenant_id, description, created_at, last_used_at",
        )
        .bind(hash_token(&token))
        .bind(idp_name)
        .bind(tenant_id)
        .bind(description)
        .fetch_one(&self.db)
        .await
        .map_err(|e| db_error("mint SCIM token", &e))?;

        Ok(MintedScimToken {
            record: decode(&row),
            token,
        })
    }

    /// Authenticate a presented bearer token.
    ///
    /// Returns the principal it proves, or [`AuthError::InvalidToken`]. The lookup is by
    /// hash — an attacker who somehow reads the table still cannot present a token — and
    /// the final comparison is constant-time so response timing carries no information
    /// about how much of a guess was right.
    ///
    /// # Errors
    ///
    /// [`AuthError::InvalidToken`] if the token is unknown or revoked;
    /// [`AuthError::DatabaseError`] if the lookup fails.
    pub async fn authenticate(&self, token: &str) -> Result<ScimPrincipal> {
        let presented = hash_token(token);
        let row = sqlx::query(
            "SELECT id, token_hash, idp_name, tenant_id, description, created_at, last_used_at \
             FROM core.tb_scim_token WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&presented)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_error("authenticate SCIM token", &e))?
        .ok_or_else(|| AuthError::InvalidToken {
            reason: "unknown or revoked SCIM provisioning token".to_string(),
        })?;

        let stored: String = row.get("token_hash");
        if stored.as_bytes().ct_eq(presented.as_bytes()).unwrap_u8() != 1 {
            return Err(AuthError::InvalidToken {
                reason: "SCIM provisioning token hash mismatch".to_string(),
            });
        }

        let id: Uuid = row.get("id");
        // Best-effort: a failed touch must not fail an otherwise-valid authentication.
        let _ = sqlx::query("UPDATE core.tb_scim_token SET last_used_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await;

        Ok(ScimPrincipal {
            idp_name:  row.get("idp_name"),
            tenant_id: row.get("tenant_id"),
        })
    }

    /// Every live provisioning credential.
    ///
    /// # Errors
    ///
    /// [`AuthError::DatabaseError`] if the read fails.
    pub async fn list(&self) -> Result<Vec<ScimTokenRecord>> {
        let rows = sqlx::query(
            "SELECT id, idp_name, tenant_id, description, created_at, last_used_at \
             FROM core.tb_scim_token WHERE revoked_at IS NULL ORDER BY created_at",
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_error("list SCIM tokens", &e))?;
        Ok(rows.iter().map(decode).collect())
    }

    /// Revoke a credential. Idempotent from the caller's view only in that a second call
    /// reports it was already gone.
    ///
    /// # Errors
    ///
    /// [`AuthError::TokenNotFound`] if no live credential has that id.
    pub async fn revoke(&self, id: Uuid) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE core.tb_scim_token SET revoked_at = now() \
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.db)
        .await
        .map_err(|e| db_error("revoke SCIM token", &e))?
        .rows_affected();
        if affected == 0 {
            return Err(AuthError::TokenNotFound);
        }
        Ok(())
    }
}

fn decode(row: &sqlx::postgres::PgRow) -> ScimTokenRecord {
    ScimTokenRecord {
        id:           row.get("id"),
        idp_name:     row.get("idp_name"),
        tenant_id:    row.get("tenant_id"),
        description:  row.get("description"),
        created_at:   row.get("created_at"),
        last_used_at: row.get("last_used_at"),
    }
}
