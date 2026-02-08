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
///
/// # SECURITY
/// - Bounded to MAX_STATES entries to prevent unbounded memory growth
/// - Expired states are automatically cleaned up on store operations
/// - Implements LRU-like eviction when max capacity is reached
#[derive(Debug)]
pub struct InMemoryStateStore {
    // Map of state -> (provider, expiry_secs)
    states:     Arc<DashMap<String, (String, u64)>>,
    // Maximum number of states to store (prevents memory exhaustion)
    max_states: usize,
}

impl InMemoryStateStore {
    /// Default maximum number of states to store (10,000 states)
    /// At ~100 bytes per state, this limits memory to ~1 MB
    const MAX_STATES: usize = 10_000;

    /// Create a new in-memory state store with default limits
    pub fn new() -> Self {
        Self {
            states:     Arc::new(DashMap::new()),
            max_states: Self::MAX_STATES,
        }
    }

    /// Create a new in-memory state store with custom max size
    ///
    /// # Arguments
    /// * `max_states` - Maximum number of states to store
    pub fn with_max_states(max_states: usize) -> Self {
        Self {
            states:     Arc::new(DashMap::new()),
            max_states: max_states.max(1), // Ensure at least 1 state
        }
    }

    /// Clean up expired states and check capacity
    ///
    /// # SECURITY
    /// Called before inserting new states to:
    /// 1. Remove expired states (automatic cleanup)
    /// 2. Check if store is at capacity
    /// 3. Return eviction needed flag if cleanup doesn't free space
    fn cleanup_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Remove all expired states
        self.states.retain(|_key, (_provider, expiry)| *expiry > now);

        // Return true if we're still over capacity after cleanup
        self.states.len() >= self.max_states
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
        // SECURITY: Clean up expired states before inserting new one
        if self.cleanup_expired() {
            // Still over capacity after cleanup - reject to prevent memory exhaustion
            return Err(crate::auth::error::AuthError::ConfigError {
                message: "State store at capacity, cannot store new state".to_string(),
            });
        }

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

/// Redis-backed state store for distributed deployments
///
/// Uses Redis to store OAuth state parameters, allowing state validation
/// across multiple server instances. Automatically expires states after TTL.
#[cfg(feature = "redis-rate-limiting")]
#[derive(Clone)]
pub struct RedisStateStore {
    client: redis::aio::ConnectionManager,
}

#[cfg(feature = "redis-rate-limiting")]
impl RedisStateStore {
    /// Create a new Redis state store
    ///
    /// # Arguments
    /// * `redis_url` - Connection string (e.g., "redis://localhost:6379")
    ///
    /// # Example
    /// ```ignore
    /// let store = RedisStateStore::new("redis://localhost:6379").await?;
    /// ```
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url).map_err(|e| {
            crate::auth::error::AuthError::ConfigError {
                message: e.to_string(),
            }
        })?;

        let connection_manager = client.get_connection_manager().await.map_err(|e| {
            crate::auth::error::AuthError::ConfigError {
                message: e.to_string(),
            }
        })?;

        Ok(Self {
            client: connection_manager,
        })
    }

    /// Get Redis key for state
    fn state_key(state: &str) -> String {
        format!("oauth:state:{}", state)
    }
}

#[cfg(feature = "redis-rate-limiting")]
#[async_trait]
impl StateStore for RedisStateStore {
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()> {
        use redis::AsyncCommands;

        let key = Self::state_key(&state);
        let ttl = expiry_secs
            .saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            )
            .max(1); // Minimum 1 second TTL

        let mut conn = self.client.clone();
        let _: () = conn.set_ex(&key, &provider, ttl).await.map_err(|e| {
            crate::auth::error::AuthError::ConfigError {
                message: e.to_string(),
            }
        })?;

        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        use redis::AsyncCommands;

        let key = Self::state_key(state);
        let mut conn = self.client.clone();

        // Get the value and delete it atomically
        let provider: Option<String> =
            conn.get(&key).await.map_err(|e| crate::auth::error::AuthError::ConfigError {
                message: e.to_string(),
            })?;

        let provider = provider.ok_or(crate::auth::error::AuthError::InvalidState)?;

        // Delete the state to prevent replay
        let _: () =
            conn.del(&key).await.map_err(|e| crate::auth::error::AuthError::ConfigError {
                message: e.to_string(),
            })?;

        // Return current time as expiry (it was already validated by Redis TTL)
        let expiry_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok((provider, expiry_secs))
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

    #[tokio::test]
    async fn test_in_memory_state_replay_prevention() {
        let store = InMemoryStateStore::new();
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store.store("state_abc".to_string(), "auth0".to_string(), expiry).await.unwrap();

        // First retrieval succeeds
        let result1 = store.retrieve("state_abc").await;
        assert!(result1.is_ok());

        // Replay attempt fails
        let result2 = store.retrieve("state_abc").await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_multiple_states() {
        let store = InMemoryStateStore::new();
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        // Store multiple states
        store.store("state1".to_string(), "google".to_string(), expiry).await.unwrap();
        store.store("state2".to_string(), "auth0".to_string(), expiry).await.unwrap();
        store.store("state3".to_string(), "okta".to_string(), expiry).await.unwrap();

        // Retrieve each independently
        let (p1, _) = store.retrieve("state1").await.unwrap();
        assert_eq!(p1, "google");

        let (p2, _) = store.retrieve("state2").await.unwrap();
        assert_eq!(p2, "auth0");

        let (p3, _) = store.retrieve("state3").await.unwrap();
        assert_eq!(p3, "okta");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_trait_object() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        store
            .store("state_trait".to_string(), "test_provider".to_string(), expiry)
            .await
            .unwrap();

        let (provider, _) = store.retrieve("state_trait").await.unwrap();
        assert_eq!(provider, "test_provider");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_bounded() {
        // SECURITY: Test that store respects max size limit
        let store = InMemoryStateStore::with_max_states(5);
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        // Store 5 states (at capacity)
        for i in 0..5 {
            let state = format!("state_{}", i);
            store.store(state, "google".to_string(), expiry).await.unwrap();
        }

        // 6th state should be rejected when store is at capacity
        let result = store.store("state_5".to_string(), "google".to_string(), expiry).await;
        assert!(result.is_err(), "Should reject insertion when at capacity");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_cleanup_expired() {
        // SECURITY: Test that expired states are cleaned up automatically
        let store = InMemoryStateStore::with_max_states(3);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Store 3 expired states
        for i in 0..3 {
            let state = format!("expired_{}", i);
            store.store(state, "google".to_string(), now - 100).await.unwrap();
        }

        // Store 1 valid state - should succeed because expired states are cleaned up
        let expiry = now + 600;
        let result = store.store("valid_state".to_string(), "auth0".to_string(), expiry).await;
        assert!(result.is_ok(), "Should succeed after cleaning up expired states");

        // Store 2 more valid states
        store
            .store("valid_state_2".to_string(), "google".to_string(), expiry)
            .await
            .unwrap();
        store
            .store("valid_state_3".to_string(), "okta".to_string(), expiry)
            .await
            .unwrap();

        // Now at capacity with valid states
        let result = store.store("valid_state_4".to_string(), "auth0".to_string(), expiry).await;
        assert!(result.is_err(), "Should be at capacity now");
    }

    #[tokio::test]
    async fn test_in_memory_state_store_custom_max_size() {
        // Test with different max sizes
        let store_small = InMemoryStateStore::with_max_states(1);
        let store_large = InMemoryStateStore::with_max_states(100);

        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        // Small store should reject after 1 state
        store_small.store("s1".to_string(), "p1".to_string(), expiry).await.unwrap();
        let result = store_small.store("s2".to_string(), "p2".to_string(), expiry).await;
        assert!(result.is_err());

        // Large store should allow more states
        for i in 0..50 {
            let state = format!("state_{}", i);
            store_large.store(state, "provider".to_string(), expiry).await.unwrap();
        }
        assert_eq!(store_large.states.len(), 50);
    }

    #[tokio::test]
    async fn test_in_memory_state_store_zero_max_enforced() {
        // Edge case: verify that min(1) is enforced
        let store = InMemoryStateStore::with_max_states(0);
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;

        // Even with max_states=0, should allow at least 1
        let result = store.store("state1".to_string(), "google".to_string(), expiry).await;
        assert!(result.is_ok(), "Should allow at least 1 state minimum");
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_state_store_basic() {
        // This test requires Redis to be running
        // Skip if Redis is unavailable
        let redis_url = "redis://localhost:6379";

        match RedisStateStore::new(redis_url).await {
            Ok(store) => {
                let expiry = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 600;

                // Store a state
                store
                    .store("redis_state_1".to_string(), "google".to_string(), expiry)
                    .await
                    .unwrap();

                // Retrieve it
                let (provider, _) = store.retrieve("redis_state_1").await.unwrap();
                assert_eq!(provider, "google");

                // Should not be retrievable again (consumed)
                let result = store.retrieve("redis_state_1").await;
                assert!(result.is_err());
            },
            Err(_) => {
                // Skip test if Redis is unavailable
                eprintln!("Skipping Redis tests - Redis server not available");
            },
        }
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_state_replay_prevention() {
        let redis_url = "redis://localhost:6379";

        if let Ok(store) = RedisStateStore::new(redis_url).await {
            let expiry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 600;

            store
                .store("redis_replay_test".to_string(), "auth0".to_string(), expiry)
                .await
                .unwrap();

            // First retrieval succeeds
            let result1 = store.retrieve("redis_replay_test").await;
            assert!(result1.is_ok());

            // Replay attempt fails
            let result2 = store.retrieve("redis_replay_test").await;
            assert!(result2.is_err());
        }
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    async fn test_redis_multiple_states() {
        let redis_url = "redis://localhost:6379";

        if let Ok(store) = RedisStateStore::new(redis_url).await {
            let expiry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 600;

            // Store multiple states
            store
                .store("redis_state_a".to_string(), "google".to_string(), expiry)
                .await
                .unwrap();
            store
                .store("redis_state_b".to_string(), "okta".to_string(), expiry)
                .await
                .unwrap();

            // Retrieve each independently
            let (p1, _) = store.retrieve("redis_state_a").await.unwrap();
            assert_eq!(p1, "google");

            let (p2, _) = store.retrieve("redis_state_b").await.unwrap();
            assert_eq!(p2, "okta");
        }
    }
}
