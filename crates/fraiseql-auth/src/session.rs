//! Session management — trait definition and helper functions.
#[cfg(test)]
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AuthError, Result};

/// Token pair returned after successful authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// JWT access token (short-lived, typically 15 min - 1 hour)
    pub access_token:  String,
    /// Refresh token (long-lived, typically 7-30 days)
    pub refresh_token: String,
    /// Time in seconds until access token expires
    pub expires_in:    u64,
}

/// SessionStore trait - implement this for your storage backend
///
/// # Examples
///
/// Implement for PostgreSQL:
/// ```no_run
/// // Requires: sqlx PgPool and live PostgreSQL connection.
/// use async_trait::async_trait;
/// use fraiseql_auth::session::{SessionStore, TokenPair};
/// use fraiseql_auth::error::Result;
///
/// pub struct PostgresSessionStore {
///     // pool: sqlx::PgPool,
/// }
///
/// #[async_trait]
/// impl SessionStore for PostgresSessionStore {
///     async fn create_session(&self, _user_id: &str, _expires_at: u64) -> Result<TokenPair> {
///         panic!("example stub")
///     }
///     async fn revoke_all_sessions(&self, _user_id: &str) -> Result<()> {
///         panic!("example stub")
///     }
/// }
/// ```
///
/// Implement for Redis:
/// ```no_run
/// // Requires: redis crate and live Redis connection.
/// use async_trait::async_trait;
/// use fraiseql_auth::session::{SessionStore, TokenPair};
/// use fraiseql_auth::error::Result;
///
/// pub struct RedisSessionStore {
///     // client: redis::Client,
/// }
///
/// #[async_trait]
/// impl SessionStore for RedisSessionStore {
///     async fn create_session(&self, _user_id: &str, _expires_at: u64) -> Result<TokenPair> {
///         panic!("example stub")
///     }
///     async fn revoke_all_sessions(&self, _user_id: &str) -> Result<()> {
///         panic!("example stub")
///     }
/// }
/// ```
// Reason: used as dyn Trait (Arc<dyn SessionStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
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

    /// Create a session for a user who has just passed the second factor.
    ///
    /// [`SecondFactorPassed`](crate::totp_mfa::SecondFactorPassed) is made only by
    /// redeeming a challenge, so this is the one way to a session for a user enrolled in
    /// `MFA`. A store that requires the second factor in [`Self::create_session`]
    /// overrides this to mint without asking again; a store that requires nothing has
    /// nothing to skip.
    ///
    /// # Errors
    /// Returns error if session creation fails
    async fn create_session_after_second_factor(
        &self,
        passed: crate::totp_mfa::SecondFactorPassed,
        expires_at: u64,
    ) -> Result<TokenPair> {
        self.create_session(passed.user_id(), expires_at).await
    }

    /// Revoke all sessions for a user
    ///
    /// # Arguments
    /// * `user_id` - The user identifier
    ///
    /// # Errors
    /// Returns error if revocation fails
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>;
}

/// Return the current Unix timestamp in seconds, or an error if the system clock
/// is broken (e.g., set before the Unix epoch).
///
/// Using saturating / `unwrap_or_default` is explicitly avoided here: a broken clock
/// would silently produce `issued_at = 0` (far in the past) or `expires_in = max`,
/// which is a fail-open security defect.  Callers must handle this error.
///
/// # Errors
///
/// Returns [`AuthError::SystemTimeError`] if the system clock is before the Unix epoch.
pub fn unix_now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| AuthError::SystemTimeError {
            message: format!("System clock is before Unix epoch: {e}"),
        })
}

/// Compute a SHA-256 hex digest of a refresh token for secure storage.
///
/// Refresh tokens are stored only as their SHA-256 hash so that a database
/// breach cannot be used to replay sessions.  The original token is returned
/// to the client and never persisted.
#[must_use]
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a cryptographically secure refresh token.
///
/// Returns 32 random bytes from [`rand::rng`] encoded as standard Base64.
/// The resulting token is 44 characters long and has approximately 256 bits of entropy.
#[must_use]
pub fn generate_refresh_token() -> String {
    use base64::Engine;
    use rand::Rng;
    // SECURITY: rand::rng() is backed by OS-level entropy for refresh tokens.
    let random_bytes: Vec<u8> = (0..32).map(|_| rand::rng().random()).collect();
    base64::engine::general_purpose::STANDARD.encode(&random_bytes)
}

/// In-memory session store for testing
#[cfg(test)]
pub struct InMemorySessionStore {
    /// Refresh-token hash → the user the session belongs to.
    sessions: Arc<dashmap::DashMap<String, String>>,
}

#[cfg(test)]
impl InMemorySessionStore {
    /// Create a new in-memory session store
    #[must_use]
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
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether the session behind `refresh_token` is still held.
    #[must_use]
    pub fn holds(&self, refresh_token: &str) -> bool {
        self.sessions.contains_key(&hash_token(refresh_token))
    }

    /// Check if there are no sessions
    #[must_use]
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
// Reason: SessionStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        // SECURITY: Propagate clock errors rather than accepting a bogus timestamp
        // (fail-closed). A broken clock would produce issued_at = 0, silently
        // extending sessions and defeating expiry checks.
        let now = unix_now()?;

        self.sessions.insert(refresh_token_hash, user_id.to_string());

        let expires_in = expires_at.saturating_sub(now);

        // For testing, generate a dummy JWT (in real impl, would come from claims)
        let access_token = format!("access_token_{}", refresh_token);

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        let mut to_remove = Vec::new();
        for entry in self.sessions.iter() {
            if entry.value() == user_id {
                to_remove.push(entry.key().clone());
            }
        }

        for key in to_remove {
            self.sessions.remove(&key);
        }

        Ok(())
    }
}
