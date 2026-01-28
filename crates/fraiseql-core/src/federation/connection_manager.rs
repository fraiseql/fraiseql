//! Connection management for direct database federation.
//!
//! Manages database connections to remote FraiseQL instances,
//! enabling direct database queries without HTTP overhead.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
};

/// Configuration for a remote database connection
#[derive(Debug, Clone)]
pub struct RemoteDatabaseConfig {
    /// Connection string (e.g., "postgresql://user:pass@host:5432/dbname")
    pub connection_string: String,
    /// Optional pool size (default: 5)
    pub pool_size:         Option<u32>,
    /// Optional connection timeout in seconds (default: 5)
    pub timeout_seconds:   Option<u32>,
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

    /// Set the connection pool size
    pub fn with_pool_size(mut self, size: u32) -> Self {
        self.pool_size = Some(size);
        self
    }

    /// Set the connection timeout
    pub fn with_timeout(mut self, seconds: u32) -> Self {
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
}

/// Manages connections to remote databases
pub struct ConnectionManager {
    /// Cached adapters keyed by connection string
    adapters: Arc<Mutex<HashMap<String, Arc<dyn DatabaseAdapter>>>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create a connection to a remote database
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
    /// Returns error if connection creation fails
    pub async fn get_or_create_connection(
        &self,
        config: RemoteDatabaseConfig,
    ) -> Result<Arc<dyn DatabaseAdapter>> {
        // Check cache first
        {
            let adapters = self.adapters.lock().map_err(|e| FraiseQLError::Internal {
                message: format!("Connection cache lock error: {}", e),
                source:  None,
            })?;

            if let Some(adapter) = adapters.get(&config.connection_string) {
                return Ok(Arc::clone(adapter));
            }
        }

        // Create new connection
        // Note: In production, this would create a real database adapter
        // For now, we document the interface
        Err(FraiseQLError::Internal {
            message:
                "Direct database connection creation requires database-specific implementation"
                    .to_string(),
            source:  None,
        })
    }

    /// Close a specific connection by connection string
    pub fn close_connection(&self, connection_string: &str) -> Result<()> {
        let mut adapters = self.adapters.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Connection cache lock error: {}", e),
            source:  None,
        })?;

        adapters.remove(connection_string);
        Ok(())
    }

    /// Close all cached connections
    pub fn close_all(&self) -> Result<()> {
        let mut adapters = self.adapters.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Connection cache lock error: {}", e),
            source:  None,
        })?;

        adapters.clear();
        Ok(())
    }

    /// Get number of cached connections
    pub fn connection_count(&self) -> Result<usize> {
        let adapters = self.adapters.lock().map_err(|e| FraiseQLError::Internal {
            message: format!("Connection cache lock error: {}", e),
            source:  None,
        })?;

        Ok(adapters.len())
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(manager.connection_count().unwrap(), 0);
    }

    #[test]
    fn test_close_all() {
        let manager = ConnectionManager::new();
        // Should not panic even with no connections
        assert!(manager.close_all().is_ok());
    }
}
