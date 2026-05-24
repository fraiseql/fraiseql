//! Connection management for direct database federation.
//!
//! Manages database connections to remote FraiseQL instances,
//! enabling direct database queries without HTTP overhead.

use std::sync::Arc;

use dashmap::DashMap;
use fraiseql_db::ArcDatabaseAdapter;
#[cfg(feature = "unstable")]
use fraiseql_error::{FraiseQLError, Result};

/// Minimum acceptable pool size (at least 1 connection is required for any work).
#[cfg(feature = "unstable")]
const MIN_POOL_SIZE: u32 = 1;
/// Maximum acceptable pool size — prevents accidental pool exhaustion on the target server.
#[cfg(feature = "unstable")]
const MAX_POOL_SIZE: u32 = 100;
/// Minimum acceptable connection timeout in seconds.
#[cfg(feature = "unstable")]
const MIN_TIMEOUT_SECS: u32 = 1;
/// Maximum acceptable connection timeout in seconds (2 minutes).
#[cfg(feature = "unstable")]
const MAX_TIMEOUT_SECS: u32 = 120;

/// Configuration for a remote database connection
#[derive(Clone)]
pub struct RemoteDatabaseConfig {
    /// Connection string (e.g., `<postgresql://user:pass@host:5432/dbname>`).
    ///
    /// Not included in `Debug` output to prevent credential leakage in logs.
    connection_string:   String,
    /// Optional pool size (default: 5)
    pub pool_size:       Option<u32>,
    /// Optional connection timeout in seconds (default: 5)
    pub timeout_seconds: Option<u32>,
}

impl std::fmt::Debug for RemoteDatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteDatabaseConfig")
            .field("connection_string", &"<redacted>")
            .field("pool_size", &self.pool_size)
            .field("timeout_seconds", &self.timeout_seconds)
            .finish()
    }
}

impl RemoteDatabaseConfig {
    /// Create a new remote database configuration
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            pool_size:         None,
            timeout_seconds:   None,
        }
    }

    /// Return the connection string.
    ///
    /// Kept private to prevent accidental exposure in `Debug` output or
    /// serialization; call this method only when the string is needed for
    /// an actual connection.
    #[must_use]
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Set the connection pool size
    #[must_use]
    pub const fn with_pool_size(mut self, size: u32) -> Self {
        self.pool_size = Some(size);
        self
    }

    /// Set the connection timeout
    #[must_use]
    pub const fn with_timeout(mut self, seconds: u32) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    /// Get pool size (default 5)
    #[must_use]
    pub fn get_pool_size(&self) -> u32 {
        self.pool_size.unwrap_or(5)
    }

    /// Get timeout in seconds (default 5)
    #[must_use]
    pub fn get_timeout_seconds(&self) -> u32 {
        self.timeout_seconds.unwrap_or(5)
    }

    /// Validate the configuration values.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if `pool_size` or `timeout_seconds`
    /// are outside their permitted ranges.
    #[cfg(feature = "unstable")]
    pub fn validate(&self) -> fraiseql_error::Result<()> {
        if let Some(size) = self.pool_size {
            if !(MIN_POOL_SIZE..=MAX_POOL_SIZE).contains(&size) {
                return Err(fraiseql_error::FraiseQLError::Validation {
                    message: format!(
                        "pool_size {size} is out of range [{MIN_POOL_SIZE}, {MAX_POOL_SIZE}]"
                    ),
                    path:    None,
                });
            }
        }
        if let Some(secs) = self.timeout_seconds {
            if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&secs) {
                return Err(fraiseql_error::FraiseQLError::Validation {
                    message: format!(
                        "timeout_seconds {secs} is out of range [{MIN_TIMEOUT_SECS}, {MAX_TIMEOUT_SECS}]"
                    ),
                    path:    None,
                });
            }
        }
        Ok(())
    }
}

/// Manages connections to remote databases.
///
/// Adapters are cached in a [`DashMap`] keyed by connection string.  Reads
/// (the common case after warm-up) take only a per-shard lock and never
/// contend with other readers on different shards; writes likewise lock a
/// single shard.  `DashMap` has no poisoning, so the cache survives panics
/// in unrelated code.
pub struct ConnectionManager {
    /// Cached adapters keyed by connection string
    adapters: Arc<DashMap<String, ArcDatabaseAdapter>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a connection to a remote database.
    ///
    /// Checks an in-memory cache first; on a miss it attempts to create a new
    /// database adapter for the given `config`.
    ///
    /// # Arguments
    ///
    /// * `config` - Remote database configuration with connection string
    ///
    /// # Returns
    ///
    /// A database adapter for the remote connection
    ///
    /// # Errors
    ///
    /// Returns error if connection creation fails.
    ///
    /// # Availability
    ///
    /// This method requires the `unstable` Cargo feature. The direct-database
    /// connection strategy is not yet production-ready; enable the feature only
    /// in development or testing environments.
    #[cfg(feature = "unstable")]
    pub async fn get_or_create_connection(
        &self,
        config: RemoteDatabaseConfig,
    ) -> Result<ArcDatabaseAdapter> {
        // Check cache first.  Holding a `dashmap::Ref` only locks one shard.
        if let Some(adapter) = self.adapters.get(config.connection_string()) {
            return Ok(Arc::clone(adapter.value()));
        }

        // Creating a real database adapter requires a database-specific
        // implementation that is not yet available in this crate.
        Err(FraiseQLError::Internal {
            message:
                "Direct database connection creation requires a database-specific implementation. \
                 This is an unstable API — contributions welcome."
                    .to_string(),
            source:  None,
        })
    }

    /// Close a specific connection by connection string.
    pub fn close_connection(&self, connection_string: &str) {
        self.adapters.remove(connection_string);
    }

    /// Close all cached connections.
    pub fn close_all(&self) {
        self.adapters.clear();
    }

    /// Get number of cached connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.adapters.len()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
