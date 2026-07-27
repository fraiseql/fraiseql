//! PostgreSQL-backed [`SessionStore`] implementation.
use async_trait::async_trait;
use sqlx::{Row, postgres::PgPool};

use crate::{
    error::{AuthError, Result},
    session::{SessionData, SessionStore, TokenPair, generate_refresh_token, hash_token, unix_now},
};

/// How this store signs the access tokens it issues.
///
/// Both variants hold key material that outlives the token, so the corresponding
/// validator can actually verify what was signed.
enum SigningKey {
    /// RSA private key in PEM format; tokens are signed RS256.
    Rs256(Vec<u8>),
    /// Shared HMAC secret; tokens are signed HS256. The same secret must be given
    /// to the validating side (e.g. `Hs256AuthState`).
    Hs256(Vec<u8>),
}

/// PostgreSQL-backed session store
pub struct PostgresSessionStore {
    db:          PgPool,
    /// Key used to sign access tokens. `None` means signing is not configured and
    /// [`SessionStore::create_session`] will fail rather than mint an unverifiable token.
    signing_key: Option<SigningKey>,
}

impl PostgresSessionStore {
    /// Create a new PostgreSQL session store **without** JWT signing configured.
    ///
    /// Refresh-token bookkeeping ([`SessionStore::get_session`],
    /// [`SessionStore::revoke_session`], [`SessionStore::revoke_all_sessions`]) works,
    /// but [`SessionStore::create_session`] will return
    /// [`AuthError::ConfigError`] because there is no key to sign the access token
    /// with. Use [`Self::with_rs256_key`] or [`Self::with_hs256_secret`] for a store
    /// that can issue sessions.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self {
            db,
            signing_key: None,
        }
    }

    /// Create a new PostgreSQL session store with RS256 JWT signing
    ///
    /// # Arguments
    /// * `db` - PostgreSQL connection pool
    /// * `private_key_pem` - RSA private key in PEM format
    #[must_use]
    pub const fn with_rs256_key(db: PgPool, private_key_pem: Vec<u8>) -> Self {
        Self {
            db,
            signing_key: Some(SigningKey::Rs256(private_key_pem)),
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
    pub const fn with_hs256_secret(db: PgPool, secret: Vec<u8>) -> Self {
        Self {
            db,
            signing_key: Some(SigningKey::Hs256(secret)),
        }
    }

    /// Initialize the sessions table
    ///
    /// This should be called once during server startup to ensure the table exists.
    ///
    /// # Errors
    /// Returns error if table creation fails
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS _system.sessions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL UNIQUE,
                issued_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                revoked_at TIMESTAMPTZ
            );

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
    fn generate_access_token(&self, user_id: &str, expires_in: u64) -> Result<String> {
        // SECURITY: Propagate clock errors; unwrap_or_default would produce iat=0.
        let now = unix_now()?;

        let exp = now + expires_in;

        let mut claims = crate::Claims {
            sub: user_id.to_string(),
            iat: now,
            exp,
            nbf: None,
            iss: "fraiseql".to_string(),
            aud: vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        // Add JTI (JWT ID) for uniqueness
        claims
            .extra
            .insert("jti".to_string(), serde_json::json!(uuid::Uuid::new_v4().to_string()));

        match &self.signing_key {
            Some(SigningKey::Rs256(private_key)) => {
                crate::jwt::generate_rs256_token(&claims, private_key)
            },
            Some(SigningKey::Hs256(secret)) => crate::jwt::generate_hs256_token(&claims, secret),
            None => Err(AuthError::ConfigError {
                message: "JWT signing is not configured for this PostgresSessionStore — \
                          construct it with with_rs256_key or with_hs256_secret. Refusing to \
                          issue an access token that no validator could verify."
                    .to_string(),
            }),
        }
    }
}

// Reason: SessionStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        // SECURITY: Propagate clock errors; unwrap_or_default would produce issued_at=0.
        let now = unix_now()?;
        let expires_in = expires_at.saturating_sub(now);

        // Mint the access token before writing the session row: if signing is not
        // configured this fails, and doing it first keeps an orphan row out of
        // _system.sessions for a session that was never handed to anyone.
        let access_token = self.generate_access_token(user_id, expires_in)?;

        sqlx::query(
            r"
            INSERT INTO _system.sessions
            (user_id, refresh_token_hash, issued_at, expires_at)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(user_id)
        .bind(&refresh_token_hash)
        .bind(now.cast_signed())
        .bind(expires_at.cast_signed())
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
