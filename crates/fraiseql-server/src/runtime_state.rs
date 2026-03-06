//! Application state with dependency injection traits.
//!
//! This module provides the shared application state structure with injectable
//! components for testing and modularity.

use std::{sync::Arc, time::SystemTime};

use fraiseql_error::RuntimeError;
#[cfg(feature = "database")]
use sqlx::PgPool;

use crate::lifecycle::shutdown::ShutdownCoordinator;

/// Shared application state with injectable components
pub struct AppState {
    /// Configuration
    pub config: Arc<crate::config::RuntimeConfig>,

    /// Database connection pool (optional - requires "database" feature)
    #[cfg(feature = "database")]
    pub db: PgPool,

    /// Read replica pools (for load balancing)
    #[cfg(feature = "database")]
    pub replicas: Vec<PgPool>,

    /// Cache client (optional, injectable)
    pub cache: Option<Arc<dyn CacheClient>>,

    /// Rate limiter state
    pub rate_limiter: Option<Arc<dyn RateLimiter>>,

    /// Webhook idempotency store (injectable)
    pub idempotency: Option<Arc<dyn IdempotencyStore>>,

    /// Shutdown coordinator
    pub shutdown: Arc<ShutdownCoordinator>,
}

impl AppState {
    /// Create new application state from configuration (without database).
    ///
    /// Only available when the `database` feature is disabled.
    /// When `database` is enabled, use `AppState::new_with_database` instead.
    #[cfg(not(feature = "database"))]
    pub fn new(config: crate::config::RuntimeConfig, shutdown: Arc<ShutdownCoordinator>) -> Self {
        Self {
            config: Arc::new(config),
            cache: None,
            rate_limiter: None,
            idempotency: None,
            shutdown,
        }
    }

    /// Create state with database connection (requires "database" feature)
    #[cfg(feature = "database")]
    pub async fn new_with_database(
        config: crate::config::RuntimeConfig,
        shutdown: Arc<ShutdownCoordinator>,
    ) -> Result<Self, RuntimeError> {
        // Connect to database
        let db_url =
            std::env::var(&config.database.url_env).map_err(|_| RuntimeError::Internal {
                message: format!("Missing environment variable: {}", config.database.url_env),
                source:  None,
            })?;
        let db = PgPool::connect(&db_url).await.map_err(|e| RuntimeError::Database(e))?;

        // Connect to replicas
        let mut replicas = Vec::new();
        for replica in &config.database.replicas {
            let url = std::env::var(&replica.url_env).map_err(|_| RuntimeError::Internal {
                message: format!("Missing environment variable: {}", replica.url_env),
                source:  None,
            })?;
            replicas.push(PgPool::connect(&url).await.map_err(|e| RuntimeError::Database(e))?);
        }

        Ok(Self {
            config: Arc::new(config),
            db,
            replicas,
            cache: None,
            rate_limiter: None,
            idempotency: None,
            shutdown,
        })
    }

    /// Get a database connection for reads (load-balanced across replicas)
    #[cfg(feature = "database")]
    pub fn read_connection(&self) -> &PgPool {
        if self.replicas.is_empty() {
            &self.db
        } else {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let idx = COUNTER.fetch_add(1, Ordering::Relaxed) % self.replicas.len();
            &self.replicas[idx]
        }
    }

    /// Get primary database connection (for writes)
    #[cfg(feature = "database")]
    pub fn write_connection(&self) -> &PgPool {
        &self.db
    }
}

/// Trait for cache operations (injectable for testing)
#[async_trait::async_trait]
pub trait CacheClient: Send + Sync {
    /// Retrieve a cached value by key.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RuntimeError>;
    /// Store a value in the cache with an optional TTL.
    async fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: Option<std::time::Duration>,
    ) -> Result<(), RuntimeError>;
    /// Remove a value from the cache.
    async fn delete(&self, key: &str) -> Result<(), RuntimeError>;
    /// Check cache connectivity.
    async fn ping(&self) -> Result<(), RuntimeError>;
}

/// Trait for rate limiting (injectable for testing)
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check whether a request identified by `key` is within the given `limit`
    /// over the sliding `window`, and record the attempt.
    async fn check(
        &self,
        key: &str,
        limit: u32,
        window: std::time::Duration,
    ) -> Result<RateLimitResult, RuntimeError>;
}

/// Result of a rate-limit check.
pub struct RateLimitResult {
    /// Whether the request was allowed (i.e. within the limit).
    pub allowed:   bool,
    /// Number of remaining requests in the current window.
    pub remaining: u32,
    /// Time at which the current rate-limit window resets.
    pub reset_at:  SystemTime,
}

/// Trait for idempotency checking (injectable for testing)
#[async_trait::async_trait]
pub trait IdempotencyStore: Send + Sync {
    /// Atomically check whether `key` has been seen before and record it if not.
    /// Returns `true` if this is the first time the key is seen.
    async fn check_and_store(
        &self,
        key: &str,
        ttl: std::time::Duration,
    ) -> Result<bool, RuntimeError>;
    /// Retrieve a previously stored idempotency result for `key`.
    async fn get_result(&self, key: &str) -> Result<Option<serde_json::Value>, RuntimeError>;
    /// Persist the idempotency result for `key` so future requests can replay it.
    async fn store_result(&self, key: &str, result: &serde_json::Value)
    -> Result<(), RuntimeError>;
}
