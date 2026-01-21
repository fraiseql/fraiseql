// PostgreSQL SessionStore implementation
use crate::auth::error::{AuthError, Result};
use crate::auth::session::{generate_refresh_token, hash_token, SessionData, SessionStore, TokenPair};
use async_trait::async_trait;
use sqlx::{postgres::PgPool, Row};

/// PostgreSQL-backed session store
pub struct PostgresSessionStore {
    db: PgPool,
}

impl PostgresSessionStore {
    /// Create a new PostgreSQL session store
    ///
    /// # Errors
    /// Returns error if database connection fails
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Initialize the sessions table
    ///
    /// This should be called once during server startup to ensure the table exists.
    ///
    /// # Errors
    /// Returns error if table creation fails
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
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
            "#,
        )
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::DatabaseError {
            message: format!("Failed to initialize sessions table: {}", e),
        })?;

        Ok(())
    }

    /// Generate a JWT access token (placeholder for real JWT generation)
    ///
    /// In a real implementation, this would use the JWT validator to create a proper JWT.
    /// For now, we return a placeholder that the middleware will exchange for a real JWT.
    fn generate_access_token(user_id: &str, expires_in: u64) -> String {
        format!(
            "access_token_{}_{}_{}",
            user_id,
            expires_in,
            uuid::Uuid::new_v4()
        )
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
            r#"
            INSERT INTO _system.sessions
            (user_id, refresh_token_hash, issued_at, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
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
        let access_token = Self::generate_access_token(user_id, expires_in);

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let row = sqlx::query(
            r#"
            SELECT user_id, issued_at, expires_at, refresh_token_hash
            FROM _system.sessions
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            "#,
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
            r#"
            UPDATE _system.sessions
            SET revoked_at = NOW()
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            "#,
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
            r#"
            UPDATE _system.sessions
            SET revoked_at = NOW()
            WHERE user_id = $1 AND revoked_at IS NULL
            "#,
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
    use super::*;

    // Note: Full integration tests would require a test database
    // These are unit tests for the logic

    #[test]
    fn test_generate_access_token() {
        let token1 = PostgresSessionStore::generate_access_token("user123", 3600);
        let token2 = PostgresSessionStore::generate_access_token("user123", 3600);

        // Each token should be unique (due to UUID)
        assert_ne!(token1, token2);
        assert!(token1.starts_with("access_token_user123_3600_"));
    }
}
