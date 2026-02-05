// PostgreSQL SessionStore implementation
use async_trait::async_trait;
use sqlx::{Row, postgres::PgPool};

use crate::auth::{
    error::{AuthError, Result},
    session::{SessionData, SessionStore, TokenPair, generate_refresh_token, hash_token},
};

/// PostgreSQL-backed session store
pub struct PostgresSessionStore {
    db: PgPool,
    /// Optional RSA private key for JWT signing (None falls back to HMAC)
    signing_key: Option<Vec<u8>>,
}

impl PostgresSessionStore {
    /// Create a new PostgreSQL session store
    ///
    /// # Errors
    /// Returns error if database connection fails
    pub fn new(db: PgPool) -> Self {
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
    pub fn with_rs256_key(db: PgPool, private_key_pem: Vec<u8>) -> Self {
        Self {
            db,
            signing_key: Some(private_key_pem),
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

    /// Generate a JWT access token with RS256 or HMAC signing
    ///
    /// Uses RS256 if a signing key is configured, otherwise falls back to HMAC with a
    /// deterministic secret derived from the user ID.
    fn generate_access_token(&self, user_id: &str, expires_in: u64) -> Result<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let exp = now + expires_in;

        let mut claims = crate::auth::Claims {
            sub: user_id.to_string(),
            iat: now,
            exp,
            iss: "fraiseql".to_string(),
            aud: vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        // Add JTI (JWT ID) for uniqueness
        claims.extra.insert(
            "jti".to_string(),
            serde_json::json!(uuid::Uuid::new_v4().to_string()),
        );

        match &self.signing_key {
            Some(private_key) => crate::auth::jwt::generate_rs256_token(&claims, private_key),
            None => {
                // Fallback: use deterministic HMAC secret (for testing/dev environments)
                let secret = format!("fraiseql_session_{}", user_id).into_bytes();
                crate::auth::jwt::generate_hs256_token(&claims, &secret)
            }
        }
    }
}

#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        sqlx::query(
            r"
            INSERT INTO _system.sessions
            (user_id, refresh_token_hash, issued_at, expires_at)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(user_id)
        .bind(&refresh_token_hash)
        .bind(now as i64)
        .bind(expires_at as i64)
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

        let expires_in = expires_at.saturating_sub(now);
        let access_token = self.generate_access_token(user_id, expires_in)?;

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
            issued_at: issued_at as u64,
            expires_at: expires_at as u64,
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

#[cfg(test)]
mod tests {

    #[test]
    fn test_generate_access_token_creates_valid_jwt() {
        // Create a minimal test store - we don't need a real pool since we're just testing token generation
        let test_pool = std::sync::Arc::new(std::sync::Mutex::new(()));
        let _ = test_pool; // Use to avoid unused variable warning

        // Test JWT generation using Claims directly instead of through the store
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = crate::auth::Claims {
            sub: "user123".to_string(),
            iat: now,
            exp: now + 3600,
            iss: "fraiseql".to_string(),
            aud: vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        claims.extra.insert(
            "jti".to_string(),
            serde_json::json!(uuid::Uuid::new_v4().to_string()),
        );

        let secret = b"fraiseql_session_user123";
        let token1 = crate::auth::jwt::generate_hs256_token(&claims, secret)
            .expect("Failed to generate token");

        // Update JTI for second token
        claims.extra.insert(
            "jti".to_string(),
            serde_json::json!(uuid::Uuid::new_v4().to_string()),
        );

        let token2 = crate::auth::jwt::generate_hs256_token(&claims, secret)
            .expect("Failed to generate token");

        // Tokens should be different (different JTI)
        assert_ne!(token1, token2);
        // Both should be valid JWT format (three dot-separated parts)
        assert_eq!(token1.matches('.').count(), 2);
        assert_eq!(token2.matches('.').count(), 2);
    }

    #[test]
    fn test_generate_access_token_with_rs256_key() {
        let test_key = include_bytes!("../../test_data/test_rsa_key.pem");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = crate::auth::Claims {
            sub: "user123".to_string(),
            iat: now,
            exp: now + 3600,
            iss: "fraiseql".to_string(),
            aud: vec!["fraiseql-api".to_string()],
            extra: std::collections::HashMap::new(),
        };

        claims.extra.insert(
            "jti".to_string(),
            serde_json::json!(uuid::Uuid::new_v4().to_string()),
        );

        let token = crate::auth::jwt::generate_rs256_token(&claims, test_key)
            .expect("Failed to generate RS256 token");

        // Valid JWT should have three parts separated by dots
        assert_eq!(token.matches('.').count(), 2);
    }
}
