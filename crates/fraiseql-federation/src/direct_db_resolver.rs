//! Direct database entity resolution for federation.
//!
//! Resolves entities from remote FraiseQL database instances via direct database connections,
//! achieving <20ms latency by eliminating HTTP overhead.

use crate::connection_manager::ConnectionManager;

/// Resolves entities from remote databases via direct connections
pub struct DirectDatabaseResolver {
    /// Manages connections to remote databases
    connection_manager: ConnectionManager,
}

impl Default for DirectDatabaseResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectDatabaseResolver {
    /// Create a new direct database resolver
    #[must_use] 
    pub fn new() -> Self {
        Self {
            connection_manager: ConnectionManager::new(),
        }
    }

    /// Get the number of cached remote connections
    #[must_use] 
    pub fn connection_count(&self) -> usize {
        self.connection_manager.connection_count()
    }

    /// Close a specific remote connection
    pub fn close_connection(&self, connection_string: &str) {
        self.connection_manager.close_connection(connection_string);
    }

    /// Close all remote connections
    pub fn close_all(&self) {
        self.connection_manager.close_all();
    }
}

#[cfg(test)]
mod tests;
