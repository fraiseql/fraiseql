//! CSRF state store — trait definition and backends.
//!
//! Stores OAuth `state` parameters for the duration of an authorization flow and
//! removes them on first retrieval, preventing state-replay attacks.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use crate::error::Result;

/// StateStore trait - implement this for different storage backends
///
/// Stores OAuth state parameters with expiration for CSRF protection.
/// In distributed deployments, use a persistent backend (Redis) instead of in-memory.
///
/// # Examples
///
/// Use in-memory store for single-instance deployments:
/// ```rust
/// use std::sync::Arc;
/// use fraiseql_auth::state_store::InMemoryStateStore;
/// let state_store = Arc::new(InMemoryStateStore::new());
/// ```
///
/// Use Redis for distributed deployments:
/// ```no_run
/// // Requires: live Redis server.
/// use std::sync::Arc;
/// # async fn example() -> fraiseql_auth::error::Result<()> {
/// # #[cfg(feature = "redis-rate-limiting")] {
/// use fraiseql_auth::state_store::RedisStateStore;
/// let state_store = Arc::new(RedisStateStore::new("redis://localhost:6379").await?);
/// # }
/// # Ok(())
/// # }
/// ```
// Reason: used as dyn Trait (Arc<dyn StateStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
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
    pub(crate) states: Arc<DashMap<String, (String, u64)>>,
    // Maximum number of states to store (prevents memory exhaustion)
    max_states:        usize,
}

impl InMemoryStateStore {
    /// Default maximum number of states to store (10,000 states)
    /// At ~100 bytes per state, this limits memory to ~1 MB
    const MAX_STATES: usize = 10_000;

    /// Create a new in-memory state store with default limits
    #[must_use]
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
    #[must_use]
    pub fn with_max_states(max_states: usize) -> Self {
        Self {
            states:     Arc::new(DashMap::new()),
            max_states: max_states.max(1), // Ensure at least 1 state
        }
    }

    /// Remove expired states and report whether the store is still at capacity.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`](crate::error::AuthError::ConfigError) if the
    /// system clock cannot be read. This fails closed: a new state cannot be admitted
    /// when state TTLs cannot be validated, and existing (possibly valid) in-flight
    /// states are left intact rather than purged.
    fn cleanup_expired(&self) -> Result<bool> {
        let Ok(now) = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
        else {
            return Err(crate::error::AuthError::ConfigError {
                message: "system clock error: cannot validate state TTLs".to_string(),
            });
        };

        // Remove all expired states.
        self.states.retain(|_key, (_provider, expiry)| *expiry > now);

        // Report whether we're still at capacity after cleanup.
        Ok(self.states.len() >= self.max_states)
    }

    /// Remove the oldest (smallest-expiry) state. Returns `true` if one was removed.
    ///
    /// The iterator reference is dropped (the key is cloned) before `remove` is called,
    /// so this never deadlocks the `DashMap`.
    fn evict_oldest(&self) -> bool {
        let oldest = self.states.iter().min_by_key(|e| e.value().1).map(|e| e.key().clone());
        match oldest {
            Some(key) => self.states.remove(&key).is_some(),
            None => false,
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: StateStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()> {
        // Remove expired states first; fail closed if the clock cannot be read.
        if self.cleanup_expired()? {
            // Still at capacity after cleanup — evict the oldest (smallest-expiry) state
            // to admit the new authorization flow rather than rejecting it with a 500.
            // The map stays bounded (one out, one in), so the memory bound holds while
            // new logins keep working under load (L-state-store-doc: the struct docs
            // already promised LRU-style eviction).
            self.evict_oldest();
        }

        self.states.insert(state, (provider, expiry_secs));
        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        let (_key, value) =
            self.states.remove(state).ok_or_else(|| crate::error::AuthError::InvalidState)?;
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
    /// ```no_run
    /// // Requires: live Redis server.
    /// # async fn example() -> fraiseql_auth::error::Result<()> {
    /// use fraiseql_auth::state_store::RedisStateStore;
    /// let store = RedisStateStore::new("redis://localhost:6379").await?;
    /// # Ok(())
    /// # }
    /// ```
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`](crate::error::AuthError::ConfigError) if the Redis URL is
    /// invalid or if the connection manager cannot be established.
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client =
            redis::Client::open(redis_url).map_err(|e| crate::error::AuthError::ConfigError {
                message: e.to_string(),
            })?;

        let connection_manager = client.get_connection_manager().await.map_err(|e| {
            crate::error::AuthError::ConfigError {
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
// Reason: StateStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
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
            crate::error::AuthError::ConfigError {
                message: e.to_string(),
            }
        })?;

        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        use redis::AsyncCommands;

        let key = Self::state_key(state);
        let mut conn = self.client.clone();

        // SECURITY: Use GETDEL (atomic get-and-delete, Redis ≥6.2) to prevent the
        // GET+DEL race condition where two concurrent requests could both read the
        // same state token before either deletes it, enabling replay attacks.
        let provider: Option<String> =
            conn.get_del(&key).await.map_err(|e| crate::error::AuthError::ConfigError {
                message: e.to_string(),
            })?;

        let provider = provider.ok_or(crate::error::AuthError::InvalidState)?;

        // Return current time as expiry (it was already validated by Redis TTL)
        let expiry_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok((provider, expiry_secs))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod lru_eviction_tests {
    use super::*;

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
    }

    // L-state-store-doc: at capacity the store must evict the oldest entry (LRU), not
    // reject new authorization flows with a 500.
    #[tokio::test]
    async fn store_evicts_oldest_at_capacity_instead_of_rejecting() {
        let store = InMemoryStateStore::with_max_states(2);
        let now = now_secs();
        store.store("s1".into(), "p".into(), now + 100).await.unwrap();
        store.store("s2".into(), "p".into(), now + 200).await.unwrap();
        // Third insert at capacity must SUCCEED by evicting the oldest (s1).
        store.store("s3".into(), "p".into(), now + 300).await.unwrap();

        assert_eq!(store.states.len(), 2, "store should stay at capacity, not grow");
        assert!(store.retrieve("s1").await.is_err(), "oldest (s1) should have been evicted");
        assert!(store.retrieve("s3").await.is_ok(), "newest (s3) should be present");
    }
}
