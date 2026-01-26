// CSRF state store - trait definition and implementations
// Prevents OAuth state parameter reuse in distributed systems

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::auth::error::Result;

/// StateStore trait - implement this for different storage backends
///
/// Stores OAuth state parameters with expiration for CSRF protection.
/// In distributed deployments, use a persistent backend (Redis) instead of in-memory.
///
/// # Examples
///
/// Use in-memory store for single-instance deployments:
/// ```ignore
/// let state_store = Arc::new(InMemoryStateStore::new());
/// ```
///
/// Use Redis for distributed deployments:
/// ```ignore
/// let state_store = Arc::new(RedisStateStore::new(redis_client).await?);
/// ```
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Store a state value with provider and expiration
    ///
    /// # Arguments
    /// * `state` - The state parameter value
    /// * `provider` - OAuth provider name
    /// * `expiry_secs` - Unix timestamp when this state expires
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()>;

    /// Retrieve and remove a state value
    ///
    /// Returns (provider, expiry_secs) if state exists and is valid
    /// Returns error if state doesn't exist or is invalid
    async fn retrieve(&self, state: &str) -> Result<(String, u64)>;
}

/// In-memory state store using DashMap
///
/// **Warning**: Only suitable for single-instance deployments!
/// For distributed systems, use RedisStateStore instead.
#[derive(Debug)]
pub struct InMemoryStateStore {
    // Map of state -> (provider, expiry_secs)
    states: Arc<DashMap<String, (String, u64)>>,
}

impl InMemoryStateStore {
    /// Create a new in-memory state store
    pub fn new() -> Self {
        Self {
            states: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()> {
        self.states.insert(state, (provider, expiry_secs));
        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        let (_key, value) = self
            .states
            .remove(state)
            .ok_or_else(|| crate::auth::error::AuthError::InvalidState)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_state_store() {
        let store = InMemoryStateStore::new();

        // Store a state
        store
            .store(
                "state123".to_string(),
                "google".to_string(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 600,
            )
            .await
            .unwrap();

        // Retrieve it
        let (provider, _expiry) = store.retrieve("state123").await.unwrap();
        assert_eq!(provider, "google");

        // Should be gone now (consumed)
        let result = store.retrieve("state123").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_state_not_found() {
        let store = InMemoryStateStore::new();
        let result = store.retrieve("nonexistent").await;
        assert!(result.is_err());
    }
}
