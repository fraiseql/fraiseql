//! Connection pool for fraiseql-wire clients.
//!
//! fraiseql-wire's `FraiseClient` consumes itself on query execution,
//! so we implement a simple connection factory pattern instead of traditional pooling.

use std::sync::Arc;
use crate::error::{FraiseQLError, Result};

/// Connection factory for fraiseql-wire clients.
///
/// Since `FraiseClient::query()` consumes the client, we store the connection string
/// and create new clients on demand rather than pooling connections.
#[derive(Debug, Clone)]
pub struct WireClientFactory {
    connection_string: Arc<String>,
}

impl WireClientFactory {
    /// Create a new client factory.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string (e.g., "postgres://localhost/mydb")
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::db::wire_pool::WireClientFactory;
    ///
    /// let factory = WireClientFactory::new("postgres://localhost/fraiseql");
    /// ```
    #[must_use]
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: Arc::new(connection_string.into()),
        }
    }

    /// Create a new fraiseql-wire client.
    ///
    /// This method creates a fresh connection each time it's called.
    /// The connection is closed when the client is dropped after query execution.
    ///
    /// # Returns
    ///
    /// A new `FraiseClient` ready for query execution.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if connection fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use fraiseql_core::db::wire_pool::WireClientFactory;
    ///
    /// let factory = WireClientFactory::new("postgres://localhost/fraiseql");
    /// let client = factory.create_client().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_client(&self) -> Result<fraiseql_wire::FraiseClient> {
        fraiseql_wire::FraiseClient::connect(&self.connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create fraiseql-wire client: {e}"),
            })
    }

    /// Get the connection string.
    #[must_use]
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        let factory = WireClientFactory::new("postgres://localhost/test");
        assert_eq!(factory.connection_string(), "postgres://localhost/test");
    }

    #[test]
    fn test_factory_clone() {
        let factory1 = WireClientFactory::new("postgres://localhost/test");
        let factory2 = factory1.clone();
        assert_eq!(factory1.connection_string(), factory2.connection_string());
    }
}
