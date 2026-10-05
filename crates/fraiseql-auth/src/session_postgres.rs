//! PostgreSQL-backed [`SessionStore`] implementation.
use async_trait::async_trait;
use sqlx::{Row, postgres::PgPool};

use crate::{
    error::{AuthError, Result},
    session::{SessionData, SessionStore, TokenPair, generate_refresh_token, hash_token, unix_now},
};

/// Default `iss` claim for minted access tokens (see [`PostgresSessionStore::with_token_claims`]).
pub const DEFAULT_TOKEN_ISSUER: &str = "fraiseql";
/// Default `aud` claim for minted access tokens (see [`PostgresSessionStore::with_token_claims`]).
pub const DEFAULT_TOKEN_AUDIENCE: &str = "fraiseql-api";
/// Default claim a minted access token carries the account's tenant under (#1450): the
/// default of `[fraiseql.tenancy] tenant_claim`. See [`PostgresSessionStore::with_tenant_claim`].
pub const DEFAULT_TENANT_CLAIM: &str = "tenant_id";

/// PostgreSQL-backed session store
pub struct PostgresSessionStore {
    db:             PgPool,
    /// Shared HMAC secret access tokens are signed with (HS256). The same secret must
    /// be given to the validating side (e.g. `Hs256AuthState`). `None` means signing is
    /// not configured and [`SessionStore::create_session`] will fail rather than mint
    /// an unverifiable token.
    hs256_secret:   Option<Vec<u8>>,
    /// `iss` claim minted into access tokens. Must match what the validating
    /// side expects — see [`Self::with_token_claims`].
    token_issuer:   String,
    /// `aud` claim minted into access tokens. Must match what the validating
    /// side expects — see [`Self::with_token_claims`].
    token_audience: String,
    /// The claim the account's tenant is minted under — see [`Self::with_tenant_claim`].
    tenant_claim:   String,
}

impl PostgresSessionStore {
    /// Create a new PostgreSQL session store **without** JWT signing configured.
    ///
    /// Refresh-token bookkeeping ([`SessionStore::get_session`],
    /// [`SessionStore::revoke_session`], [`SessionStore::revoke_all_sessions`]) works,
    /// but [`SessionStore::create_session`] will return
    /// [`AuthError::ConfigError`] because there is no key to sign the access token
    /// with. Use [`Self::with_hs256_secret`] for a store that can issue sessions.
    #[must_use]
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            hs256_secret: None,
            token_issuer: DEFAULT_TOKEN_ISSUER.to_string(),
            token_audience: DEFAULT_TOKEN_AUDIENCE.to_string(),
            tenant_claim: DEFAULT_TENANT_CLAIM.to_string(),
        }
    }

    /// Create a new PostgreSQL session store with HS256 (HMAC) JWT signing.
    ///
    /// The secret is retained for the life of the store and **must** be the same
    /// secret configured on the validating side, otherwise the issued tokens will
    /// not verify.
    ///
    /// # Arguments
    /// * `db` - PostgreSQL connection pool
    /// * `secret` - Shared HMAC secret (use at least 32 bytes of entropy)
    #[must_use]
    pub fn with_hs256_secret(db: PgPool, secret: Vec<u8>) -> Self {
        Self {
            hs256_secret: Some(secret),
            ..Self::new(db)
        }
    }

    /// Mint the account's tenant under `claim`: the schema's
    /// `[fraiseql.tenancy] tenant_claim`, so a FraiseQL-minted token's tenant reaches
    /// `SecurityContext::tenant_id` exactly as an external IdP's does (#1450).
    #[must_use]
    pub fn with_tenant_claim(mut self, claim: impl Into<String>) -> Self {
        self.tenant_claim = claim.into();
        self
    }

    /// Set the `iss` / `aud` claims minted into access tokens.
    ///
    /// The defaults (`fraiseql` / `fraiseql-api`) only validate against a
    /// validator configured with exactly those values. A deployment whose
    /// `[auth_hs256]` declares its own issuer/audience **must** mint matching
    /// claims, or every login "succeeds" and then 401s on the first validated
    /// request — the mint/validate drift #368's server mount surfaced.
    #[must_use]
    pub fn with_token_claims(
        mut self,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        self.token_issuer = issuer.into();
        self.token_audience = audience.into();
        self
    }

    /// Initialize the sessions table
    ///
    /// This should be called once during server startup to ensure the table exists.
    ///
    /// # Errors
    /// Returns error if table creation fails
    pub async fn init(&self) -> Result<()> {
        // raw_sql: this is a multi-statement DDL batch, which the prepared-
        // statement path (`sqlx::query`) refuses at the protocol level.
        sqlx::raw_sql(
            r"
            CREATE SCHEMA IF NOT EXISTS _system;

            CREATE TABLE IF NOT EXISTS _system.sessions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL UNIQUE,
                issued_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                revoked_at TIMESTAMPTZ
            );

            -- The account's tenant when the session was minted (#1450), for revocation and
            -- audit. NULL for a platform account or a principal with no account row.
            ALTER TABLE _system.sessions ADD COLUMN IF NOT EXISTS tenant_id UUID;

            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON _system.sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON _system.sessions(expires_at);
            CREATE INDEX IF NOT EXISTS idx_sessions_revoked_at ON _system.sessions(revoked_at);
            ",
        )
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to initialize sessions table: {}", e),
        })?;

        Ok(())
    }

    /// Generate a JWT access token with the configured signing key.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] when no signing key is configured. Minting
    /// a token nobody can verify is worse than failing: it produces a login that
    /// "succeeds" and then 401s on every subsequent request.
    fn generate_access_token(
        &self,
        user_id: &str,
        tenant: Option<uuid::Uuid>,
        expires_in: u64,
    ) -> Result<String> {
        // SECURITY: Propagate clock errors; unwrap_or_default would produce iat=0.
        let now = unix_now()?;

        let exp = now + expires_in;

        let mut claims = crate::Claims {
            sub: user_id.to_string(),
            iat: now,
            exp,
            nbf: None,
            iss: self.token_issuer.clone(),
            aud: vec![self.token_audience.clone()],
            extra: std::collections::HashMap::new(),
        };

        // Add JTI (JWT ID) for uniqueness
        claims
            .extra
            .insert("jti".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));

        // The account's tenant, under the schema's tenant claim (#1450). The simple form
        // (32 hex digits) is the one rendering that is both a valid tenant key (registry,
        // `X-Tenant-ID`, schema mode) and a valid `::uuid` cast for the auth tables' RLS.
        if let Some(tenant) = tenant {
            claims.extra.insert(
                self.tenant_claim.clone(),
                serde_json::json!(tenant.as_simple().to_string()),
            );
        }

        match &self.hs256_secret {
            Some(secret) => crate::jwt::generate_hs256_token(&claims, secret),
            None => Err(AuthError::ConfigError {
                message: "JWT signing is not configured for this PostgresSessionStore — \
                          construct it with with_hs256_secret. Refusing to issue an access \
                          token that no validator could verify."
                    .to_string(),
            }),
        }
    }
}

/// The account a session is minted for: refused if SCIM deactivated it (#946), and its
/// tenant otherwise (#1450).
///
/// The tenant comes from the account row, which only a tenant authority can set (#1088),
/// and never from the request: no caller can choose the tenant a session carries. Every
/// credential path converges on `create_session`, so every one of them gets it.
///
/// # Why a missing table is allowed through
///
/// `core.tb_user` belongs to the account store, which a session-only embedder need not
/// have. Treating its absence as a refusal would break those deployments closed for a
/// reason unrelated to offboarding. So the *specific* Postgres `undefined_table` code is
/// the one condition that passes; every other error — including a readable row that says
/// `active = false` — refuses. A deployment running SCIM always has the table (SCIM
/// provisions into it), so the tolerated branch is unreachable there.
///
/// A user with no row is allowed, in no tenant: anonymous sessions and JWT-only
/// principals live in a different identity space and were never provisioned.
async fn account_for_session(db: &PgPool, user_id: &str) -> Result<Option<uuid::Uuid>> {
    let row = sqlx::query("SELECT active, tenant_id FROM core.tb_user WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await;

    let (active, tenant): (Option<bool>, Option<uuid::Uuid>) = match row {
        Ok(Some(row)) => (Some(row.get("active")), row.get("tenant_id")),
        Ok(None) => (None, None),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => {
            // No account store in this deployment; nothing was ever provisioned.
            return Ok(None);
        },
        Err(e) => {
            return Err(AuthError::DatabaseError {
                message: format!("Failed to check account activation: {e}"),
            });
        },
    };

    if active == Some(false) {
        tracing::warn!(
            user_id = %user_id,
            "refused a session for a deactivated account (SCIM active = false)"
        );
        return Err(AuthError::AccountDeactivated);
    }
    Ok(tenant)
}

// Reason: SessionStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        // Offboarding must block every credential, not just the one the IdP owns (#946).
        // This is the single place every credential path converges on — password login,
        // MFA's second factor, social callback, OTP, SAML ACS all end here — which is why
        // the check lives here rather than in each of them.
        let tenant = account_for_session(&self.db, user_id).await?;

        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        // SECURITY: Propagate clock errors; unwrap_or_default would produce issued_at=0.
        let now = unix_now()?;
        let expires_in = expires_at.saturating_sub(now);

        // Mint the access token before writing the session row: if signing is not
        // configured this fails, and doing it first keeps an orphan row out of
        // _system.sessions for a session that was never handed to anyone.
        let access_token = self.generate_access_token(user_id, tenant, expires_in)?;

        sqlx::query(
            r"
            INSERT INTO _system.sessions
            (user_id, refresh_token_hash, issued_at, expires_at, tenant_id)
            VALUES ($1, $2, $3, $4, $5)
            ",
        )
        .bind(user_id)
        .bind(&refresh_token_hash)
        .bind(now.cast_signed())
        .bind(expires_at.cast_signed())
        .bind(tenant)
        .execute(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                AuthError::SessionError {
                    message: "Refresh token already exists".to_string(),
                }
            } else {
                AuthError::DatabaseError {
                    message: format!("Failed to create session: {}", e),
                }
            }
        })?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let row = sqlx::query(
            r"
            SELECT user_id, issued_at, expires_at, refresh_token_hash
            FROM _system.sessions
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            ",
        )
        .bind(refresh_token_hash)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to get session: {}", e),
        })?
        .ok_or(AuthError::TokenNotFound)?;

        let user_id: String = row.get("user_id");
        let issued_at: i64 = row.get("issued_at");
        let expires_at: i64 = row.get("expires_at");
        let refresh_token_hash: String = row.get("refresh_token_hash");

        Ok(SessionData {
            user_id,
            issued_at: issued_at.cast_unsigned(),
            expires_at: expires_at.cast_unsigned(),
            refresh_token_hash,
        })
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        let result = sqlx::query(
            r"
            UPDATE _system.sessions
            SET revoked_at = NOW()
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            ",
        )
        .bind(refresh_token_hash)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to revoke session: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(AuthError::SessionError {
                message: "Session not found or already revoked".to_string(),
            });
        }

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        sqlx::query(
            r"
            UPDATE _system.sessions
            SET revoked_at = NOW()
            WHERE user_id = $1 AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to revoke all sessions: {}", e),
        })?;

        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
