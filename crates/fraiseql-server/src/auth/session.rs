// Session management - trait definition and implementations
use crate::auth::error::Result;
use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use crate::auth::error::AuthError;
#[cfg(test)]
use std::sync::Arc;

/// Session data stored in the backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// User ID (unique per user)
    pub user_id: String,
    /// Session issued timestamp (Unix seconds)
    pub issued_at: u64,
    /// Session expiration timestamp (Unix seconds)
    pub expires_at: u64,
    /// Hash of the refresh token (stored securely)
    pub refresh_token_hash: String,
}

impl SessionData {
    /// Check if session is expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.expires_at <= now
    }
}

/// Token pair returned after successful authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// JWT access token (short-lived, typically 15 min - 1 hour)
    pub access_token: String,
    /// Refresh token (long-lived, typically 7-30 days)
    pub refresh_token: String,
    /// Time in seconds until access token expires
    pub expires_in: u64,
}

/// SessionStore trait - implement this for your storage backend
///
/// # Examples
///
/// Implement for PostgreSQL:
/// ```ignore
/// pub struct PostgresSessionStore {
///     pool: PgPool,
/// }
///
/// #[async_trait]
/// impl SessionStore for PostgresSessionStore {
///     async fn create_session(...) -> Result<TokenPair> { ... }
///     // ... other methods
/// }
/// ```
///
/// Implement for Redis:
/// ```ignore
/// pub struct RedisSessionStore {
///     client: redis::Client,
/// }
///
/// #[async_trait]
/// impl SessionStore for RedisSessionStore {
///     async fn create_session(...) -> Result<TokenPair> { ... }
///     // ... other methods
/// }
/// ```
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create a new session and return token pair
    ///
    /// # Arguments
    /// * `user_id` - The user identifier
    /// * `expires_at` - When the session should expire (Unix seconds)
    ///
    /// # Returns
    /// TokenPair with access_token and refresh_token
    ///
    /// # Errors
    /// Returns error if session creation fails
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair>;

    /// Get session data by refresh token hash
    ///
    /// # Arguments
    /// * `refresh_token_hash` - Hash of the refresh token
    ///
    /// # Returns
    /// SessionData if session exists and is not revoked
    ///
    /// # Errors
    /// Returns SessionError if session not found or revoked
    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData>;

    /// Revoke a single session
    ///
    /// # Arguments
    /// * `refresh_token_hash` - Hash of the refresh token to revoke
    ///
    /// # Errors
    /// Returns error if revocation fails
    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()>;

    /// Revoke all sessions for a user
    ///
    /// # Arguments
    /// * `user_id` - The user identifier
    ///
    /// # Errors
    /// Returns error if revocation fails
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>;
}

/// Hash a refresh token for secure storage
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a cryptographically secure refresh token
pub fn generate_refresh_token() -> String {
    use base64::Engine;
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::engine::general_purpose::STANDARD.encode(&random_bytes)
}

/// In-memory session store for testing
#[cfg(test)]
pub struct InMemorySessionStore {
    sessions: Arc<dashmap::DashMap<String, SessionData>>,
}

#[cfg(test)]
impl InMemorySessionStore {
    /// Create a new in-memory session store
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Clear all sessions (useful for tests)
    pub fn clear(&self) {
        self.sessions.clear();
    }

    /// Get number of sessions (useful for tests)
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Check if there are no sessions
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id: user_id.to_string(),
            issued_at: now,
            expires_at,
            refresh_token_hash: refresh_token_hash.clone(),
        };

        self.sessions.insert(refresh_token_hash, session);

        let expires_in = expires_at.saturating_sub(now);

        // For testing, generate a dummy JWT (in real impl, would come from claims)
        let access_token = format!("access_token_{}", refresh_token);

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        self.sessions
            .get(refresh_token_hash)
            .map(|entry| entry.clone())
            .ok_or(AuthError::TokenNotFound)
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        self.sessions
            .remove(refresh_token_hash)
            .ok_or(AuthError::SessionError {
                message: "Session not found".to_string(),
            })?;
        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        let mut to_remove = Vec::new();
        for entry in self.sessions.iter() {
            if entry.user_id == user_id {
                to_remove.push(entry.key().clone());
            }
        }

        for key in to_remove {
            self.sessions.remove(&key);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token() {
        let token = "my_secret_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);

        // Same token should produce same hash
        assert_eq!(hash1, hash2);

        // Different token should produce different hash
        let different_hash = hash_token("different_token");
        assert_ne!(hash1, different_hash);
    }

    #[test]
    fn test_generate_refresh_token() {
        let token1 = generate_refresh_token();
        let token2 = generate_refresh_token();

        // Tokens should be random and different
        assert_ne!(token1, token2);
        // Should be non-empty
        assert!(!token1.is_empty());
        assert!(!token2.is_empty());
    }

    #[test]
    fn test_session_data_not_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id: "user123".to_string(),
            issued_at: now,
            expires_at: now + 3600,
            refresh_token_hash: "hash".to_string(),
        };

        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_data_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id: "user123".to_string(),
            issued_at: now - 3600,
            expires_at: now - 100,
            refresh_token_hash: "hash".to_string(),
        };

        assert!(session.is_expired());
    }

    #[tokio::test]
    async fn test_in_memory_store_create_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let result = store.create_session("user123", now + 3600).await;
        assert!(result.is_ok());

        let tokens = result.unwrap();
        assert!(!tokens.access_token.is_empty());
        assert!(!tokens.refresh_token.is_empty());
        assert!(tokens.expires_in > 0);
    }

    #[tokio::test]
    async fn test_in_memory_store_get_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let refresh_token_hash = hash_token(&tokens.refresh_token);

        let session = store.get_session(&refresh_token_hash).await;
        assert!(session.is_ok());
        assert_eq!(session.unwrap().user_id, "user123");
    }

    #[tokio::test]
    async fn test_in_memory_store_revoke_session() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let refresh_token_hash = hash_token(&tokens.refresh_token);

        assert!(store.revoke_session(&refresh_token_hash).await.is_ok());

        let session = store.get_session(&refresh_token_hash).await;
        assert!(session.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_store_revoke_all_sessions() {
        let store = InMemorySessionStore::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Create multiple sessions for same user
        let tokens1 = store.create_session("user123", now + 3600).await.unwrap();
        let tokens2 = store.create_session("user123", now + 3600).await.unwrap();

        // Create session for different user
        let tokens3 = store.create_session("user456", now + 3600).await.unwrap();

        assert_eq!(store.len(), 3);

        // Revoke all for user123
        assert!(store.revoke_all_sessions("user123").await.is_ok());

        // user456 session should still exist
        let hash3 = hash_token(&tokens3.refresh_token);
        assert!(store.get_session(&hash3).await.is_ok());

        // user123 sessions should be gone
        let hash1 = hash_token(&tokens1.refresh_token);
        let hash2 = hash_token(&tokens2.refresh_token);
        assert!(store.get_session(&hash1).await.is_err());
        assert!(store.get_session(&hash2).await.is_err());
    }
}
