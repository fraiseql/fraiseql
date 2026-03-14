//! Connection management for direct database federation.
//!
//! Manages database connections to remote FraiseQL instances,
//! enabling direct database queries without HTTP overhead.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
    /// Connection string (e.g., "postgresql://user:pass@host:5432/dbname").
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
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Set the connection pool size
    pub const fn with_pool_size(mut self, size: u32) -> Self {
        self.pool_size = Some(size);
        self
    }

    /// Set the connection timeout
    pub const fn with_timeout(mut self, seconds: u32) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    /// Get pool size (default 5)
    pub fn get_pool_size(&self) -> u32 {
        self.pool_size.unwrap_or(5)
    }

    /// Get timeout in seconds (default 5)
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
            if size < MIN_POOL_SIZE || size > MAX_POOL_SIZE {
                return Err(fraiseql_error::FraiseQLError::Validation {
                    message: format!(
                        "pool_size {size} is out of range [{MIN_POOL_SIZE}, {MAX_POOL_SIZE}]"
                    ),
                    path:    None,
                });
            }
        }
        if let Some(secs) = self.timeout_seconds {
            if secs < MIN_TIMEOUT_SECS || secs > MAX_TIMEOUT_SECS {
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

/// Manages connections to remote databases
pub struct ConnectionManager {
    /// Cached adapters keyed by connection string
    adapters: Arc<Mutex<HashMap<String, ArcDatabaseAdapter>>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
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
        // Check cache first
        {
            let adapters = self.adapters.lock().unwrap_or_else(|e| e.into_inner());

            if let Some(adapter) = adapters.get(config.connection_string()) {
                return Ok(Arc::clone(adapter));
            }
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
        let mut adapters = self.adapters.lock().unwrap_or_else(|e| e.into_inner());
        adapters.remove(connection_string);
    }

    /// Close all cached connections.
    pub fn close_all(&self) {
        let mut adapters = self.adapters.lock().unwrap_or_else(|e| e.into_inner());
        adapters.clear();
    }

    /// Get number of cached connections.
    pub fn connection_count(&self) -> usize {
        let adapters = self.adapters.lock().unwrap_or_else(|e| e.into_inner());
        adapters.len()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_remote_database_config_defaults() {
        let config = RemoteDatabaseConfig::new("postgresql://localhost/db");
        assert_eq!(config.get_pool_size(), 5);
        assert_eq!(config.get_timeout_seconds(), 5);
    }

    #[test]
    fn test_remote_database_config_custom() {
        let config = RemoteDatabaseConfig::new("postgresql://localhost/db")
            .with_pool_size(10)
            .with_timeout(30);

        assert_eq!(config.get_pool_size(), 10);
        assert_eq!(config.get_timeout_seconds(), 30);
    }

    #[test]
    fn test_connection_manager_creation() {
        let _manager = ConnectionManager::new();
        // Should not panic
    }

    #[test]
    fn test_connection_manager_default() {
        let _manager = ConnectionManager::default();
        // Should not panic
    }

    #[test]
    fn test_connection_count_empty() {
        let manager = ConnectionManager::new();
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn test_close_all() {
        let manager = ConnectionManager::new();
        // Should not panic even with no connections
        manager.close_all();
    }

    #[test]
    fn test_config_connection_string_not_in_debug() {
        let config = RemoteDatabaseConfig::new("postgresql://user:secret@host/db");
        let debug_output = format!("{config:?}");
        assert!(!debug_output.contains("secret"), "connection string must not appear in Debug");
        assert!(debug_output.contains("<redacted>"));
    }

    #[test]
    fn test_config_connection_string_accessor() {
        let config = RemoteDatabaseConfig::new("postgresql://host/db");
        assert_eq!(config.connection_string(), "postgresql://host/db");
    }

    // ── Bounds validation tests ────────────────────────────────────────────────

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_accepts_valid_defaults() {
        let config = RemoteDatabaseConfig::new("postgresql://host/db");
        assert!(config.validate().is_ok(), "no explicit values — defaults are always valid");
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_accepts_pool_size_at_limits() {
        let lo = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MIN_POOL_SIZE);
        assert!(lo.validate().is_ok());

        let hi = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MAX_POOL_SIZE);
        assert!(hi.validate().is_ok());
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_rejects_pool_size_zero() {
        let config = RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(0);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("pool_size"), "error must mention pool_size: {err}");
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_rejects_pool_size_too_large() {
        let config =
            RemoteDatabaseConfig::new("postgresql://host/db").with_pool_size(MAX_POOL_SIZE + 1);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("pool_size"), "error must mention pool_size: {err}");
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_rejects_timeout_zero() {
        let config = RemoteDatabaseConfig::new("postgresql://host/db").with_timeout(0);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("timeout_seconds"), "error must mention timeout: {err}");
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_validate_rejects_timeout_too_large() {
        let config =
            RemoteDatabaseConfig::new("postgresql://host/db").with_timeout(MAX_TIMEOUT_SECS + 1);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("timeout_seconds"), "error must mention timeout: {err}");
    }
}
