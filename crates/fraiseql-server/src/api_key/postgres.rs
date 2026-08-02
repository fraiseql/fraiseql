//! Postgres-backed API-key store (#627).
//!
//! Keys use the selector + verifier discipline (the same shape as the
//! password-reset tokens): the full key is `fqlk_<selector>_<verifier>`.
//!
//! The database stores the plaintext **selector** (indexed lookup) and only
//! the SHA-256 **hash of the verifier**. Resolution looks the row up by
//! selector — no secret material in the `WHERE` — and compares the verifier
//! hash in constant time, so a full database read cannot forge a usable key
//! and the lookup cost does not scale with the number of keys.
//!
//! Lifecycle: keys are created with optional expiry, revoked by setting
//! `revoked_at` (never deleted — the row is the audit record), and rotated by
//! atomically replacing the verifier hash so the key identity (selector, name,
//! scopes) survives while every copy of the old secret stops working.

use chrono::{DateTime, Utc};
use rand::RngCore;
use sqlx::{PgPool, Row};

use super::sha256_hash;

/// DDL for the API-key table.
///
/// Executed at boot by `provision_persistent_schemas` when
/// `[security.api_keys] storage = "postgres"` — and by the live-PG tests,
/// which is what keeps it valid PostgreSQL (#748 precedent).
pub const PG_API_KEY_SCHEMA_SQL: &str = r"
CREATE SCHEMA IF NOT EXISTS core;
CREATE TABLE IF NOT EXISTS core.tb_api_key (
    pk_api_key      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    selector        TEXT NOT NULL UNIQUE,
    verifier_hash   BYTEA NOT NULL,
    name            TEXT NOT NULL,
    scopes          TEXT[] NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ,
    revoked_at      TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ
);
";

/// Prefix identifying a Postgres-stored key. Static (env-hashed) keys have no
/// required shape; a key that does not parse as `fqlk_<selector>_<verifier>`
/// is simply not looked up in the database.
const KEY_PREFIX: &str = "fqlk";
/// Selector length in random bytes (hex-encoded to 24 chars).
const SELECTOR_BYTES: usize = 12;
/// Verifier length in random bytes (hex-encoded to 48 chars).
const VERIFIER_BYTES: usize = 24;

/// Error type for API-key store operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum ApiKeyStoreError {
    /// The caller supplied something malformed (an unknown selector, an empty
    /// name). Maps to a 4xx at the admin surface.
    InvalidInput(String),
    /// The selector does not exist.
    NotFound,
    /// Database error. The server's fault, not the caller's.
    Database(String),
}

impl std::fmt::Display for ApiKeyStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "{msg}"),
            Self::NotFound => write!(f, "API key not found"),
            Self::Database(msg) => write!(f, "Database error: {msg}"),
        }
    }
}

impl std::error::Error for ApiKeyStoreError {}

impl From<sqlx::Error> for ApiKeyStoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// A stored key's public metadata — everything except secret material.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKeyRecord {
    /// The indexed lookup half of the key. Public: it appears in the full key
    /// but is useless without the verifier.
    pub selector:     String,
    /// Human-readable key name (audit identity).
    pub name:         String,
    /// OAuth-style scopes granted by this key.
    pub scopes:       Vec<String>,
    /// Creation time.
    pub created_at:   DateTime<Utc>,
    /// Optional expiry; a key past this instant is rejected.
    pub expires_at:   Option<DateTime<Utc>>,
    /// Revocation time; a revoked key is rejected. Rows are never deleted.
    pub revoked_at:   Option<DateTime<Utc>>,
    /// Last successful authentication with this key.
    pub last_used_at: Option<DateTime<Utc>>,
}

/// A resolved row for authentication: verifier hash plus the gate fields.
pub(super) struct ResolvedDbKey {
    pub verifier_hash: Vec<u8>,
    pub name:          String,
    pub scopes:        Vec<String>,
    pub expires_at:    Option<DateTime<Utc>>,
    pub revoked_at:    Option<DateTime<Utc>>,
}

/// Split a raw header value into `(selector, verifier)` when it has the
/// Postgres-stored key shape.
pub(super) fn parse_key(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix(KEY_PREFIX)?.strip_prefix('_')?;
    let (selector, verifier) = rest.split_once('_')?;
    if selector.len() == SELECTOR_BYTES * 2 && verifier.len() == VERIFIER_BYTES * 2 {
        Some((selector, verifier))
    } else {
        None
    }
}

/// Postgres-backed API-key store.
#[derive(Debug, Clone)]
pub struct PgApiKeyStore {
    pool: PgPool,
}

impl PgApiKeyStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute the table DDL. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`ApiKeyStoreError::Database`] when a statement fails.
    pub async fn ensure_schema(&self) -> Result<(), ApiKeyStoreError> {
        // One statement per execute — the adapter contract from P22 holds for
        // sqlx simple queries with multiple statements too; split defensively.
        for stmt in PG_API_KEY_SCHEMA_SQL.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&self.pool).await?;
        }
        Ok(())
    }

    /// Create a new key. Returns the **full key** (shown exactly once) and the
    /// stored record.
    ///
    /// # Errors
    ///
    /// [`ApiKeyStoreError::InvalidInput`] for an empty name;
    /// [`ApiKeyStoreError::Database`] on query failure.
    pub async fn create_key(
        &self,
        name: &str,
        scopes: &[String],
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(String, ApiKeyRecord), ApiKeyStoreError> {
        if name.trim().is_empty() {
            return Err(ApiKeyStoreError::InvalidInput("key name must not be empty".into()));
        }
        let (selector, verifier, full_key) = generate_key_material();
        let verifier_hash = sha256_hash(verifier.as_bytes()).to_vec();

        let row = sqlx::query(
            "INSERT INTO core.tb_api_key (selector, verifier_hash, name, scopes, expires_at)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING created_at",
        )
        .bind(&selector)
        .bind(&verifier_hash)
        .bind(name)
        .bind(scopes)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;

        let record = ApiKeyRecord {
            selector,
            name: name.to_string(),
            scopes: scopes.to_vec(),
            created_at: row.get("created_at"),
            expires_at,
            revoked_at: None,
            last_used_at: None,
        };
        Ok((full_key, record))
    }

    /// List all keys (public metadata only — no hashes).
    ///
    /// # Errors
    ///
    /// [`ApiKeyStoreError::Database`] on query failure.
    pub async fn list_keys(&self) -> Result<Vec<ApiKeyRecord>, ApiKeyStoreError> {
        let rows = sqlx::query(
            "SELECT selector, name, scopes, created_at, expires_at, revoked_at, last_used_at
             FROM core.tb_api_key ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ApiKeyRecord {
                selector:     r.get("selector"),
                name:         r.get("name"),
                scopes:       r.get("scopes"),
                created_at:   r.get("created_at"),
                expires_at:   r.get("expires_at"),
                revoked_at:   r.get("revoked_at"),
                last_used_at: r.get("last_used_at"),
            })
            .collect())
    }

    /// Revoke a key by selector. Idempotent on already-revoked keys.
    ///
    /// # Errors
    ///
    /// [`ApiKeyStoreError::NotFound`] for an unknown selector;
    /// [`ApiKeyStoreError::Database`] on query failure.
    pub async fn revoke(&self, selector: &str) -> Result<(), ApiKeyStoreError> {
        let result = sqlx::query(
            "UPDATE core.tb_api_key SET revoked_at = COALESCE(revoked_at, now())
             WHERE selector = $1",
        )
        .bind(selector)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ApiKeyStoreError::NotFound);
        }
        Ok(())
    }

    /// Rotate a key: atomically replace its verifier, keeping the selector,
    /// name and scopes. Every copy of the old secret stops working the moment
    /// the update commits. Refuses to rotate a revoked key (rotation would
    /// silently un-revoke it).
    ///
    /// Returns the new full key (shown exactly once).
    ///
    /// # Errors
    ///
    /// [`ApiKeyStoreError::NotFound`] for an unknown or revoked selector;
    /// [`ApiKeyStoreError::Database`] on query failure.
    pub async fn rotate(&self, selector: &str) -> Result<String, ApiKeyStoreError> {
        let (_, verifier, _) = generate_key_material();
        let verifier_hash = sha256_hash(verifier.as_bytes()).to_vec();
        let result = sqlx::query(
            "UPDATE core.tb_api_key SET verifier_hash = $2
             WHERE selector = $1 AND revoked_at IS NULL",
        )
        .bind(selector)
        .bind(&verifier_hash)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ApiKeyStoreError::NotFound);
        }
        Ok(assemble_full_key(selector, &verifier))
    }

    /// Resolve a selector for authentication. Returns the verifier hash and
    /// gate fields; the caller performs the constant-time comparison so that
    /// this query stays secret-free.
    pub(super) async fn resolve(
        &self,
        selector: &str,
    ) -> Result<Option<ResolvedDbKey>, ApiKeyStoreError> {
        let row = sqlx::query(
            "SELECT verifier_hash, name, scopes, expires_at, revoked_at
             FROM core.tb_api_key WHERE selector = $1",
        )
        .bind(selector)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| ResolvedDbKey {
            verifier_hash: r.get("verifier_hash"),
            name:          r.get("name"),
            scopes:        r.get("scopes"),
            expires_at:    r.get("expires_at"),
            revoked_at:    r.get("revoked_at"),
        }))
    }

    /// Stamp a successful use. Best-effort: an error here must not fail the
    /// authenticated request, so the caller logs rather than propagates.
    pub(super) async fn touch(&self, selector: &str) -> Result<(), ApiKeyStoreError> {
        sqlx::query("UPDATE core.tb_api_key SET last_used_at = now() WHERE selector = $1")
            .bind(selector)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// Generate `(selector, verifier, full_key)` from OS randomness.
fn generate_key_material() -> (String, String, String) {
    let mut selector_bytes = [0u8; SELECTOR_BYTES];
    let mut verifier_bytes = [0u8; VERIFIER_BYTES];
    rand::rng().fill_bytes(&mut selector_bytes);
    rand::rng().fill_bytes(&mut verifier_bytes);
    let selector = hex::encode(selector_bytes);
    let verifier = hex::encode(verifier_bytes);
    let full = assemble_full_key(&selector, &verifier);
    (selector, verifier, full)
}

/// Assemble the wire form `fqlk_<selector>_<verifier>`.
fn assemble_full_key(selector: &str, verifier: &str) -> String {
    format!("{KEY_PREFIX}_{selector}_{verifier}")
}
