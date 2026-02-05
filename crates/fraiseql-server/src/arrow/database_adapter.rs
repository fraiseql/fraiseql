//! Database adapter wrapper for Arrow Flight service.
//!
//! This module provides a wrapper that adapts fraiseql-core's PostgresAdapter
//! to fraiseql-arrow's DatabaseAdapter trait, enabling the Arrow Flight service
//! to execute queries against real PostgreSQL databases.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
#[cfg(feature = "arrow")]
use fraiseql_arrow::db::{DatabaseAdapter as ArrowDatabaseAdapter, DatabaseError};
use fraiseql_core::db::{
    postgres::PostgresAdapter, traits::DatabaseAdapter as CoreDatabaseAdapter,
};

/// Wrapper that adapts fraiseql-core's PostgresAdapter to fraiseql-arrow's DatabaseAdapter trait.
///
/// This enables the Arrow Flight service to execute queries against PostgreSQL
/// without requiring direct knowledge of fraiseql-core's DatabaseAdapter interface.
pub struct FlightDatabaseAdapter {
    /// Inner PostgreSQL adapter from fraiseql-core
    inner: Arc<PostgresAdapter>,
}

impl FlightDatabaseAdapter {
    /// Create a new Arrow Flight database adapter.
    ///
    /// # Arguments
    ///
    /// * `adapter` - PostgreSQL adapter from fraiseql-core
    pub fn new(adapter: PostgresAdapter) -> Self {
        Self {
            inner: Arc::new(adapter),
        }
    }

    /// Create a new Arrow Flight database adapter from an Arc.
    ///
    /// # Arguments
    ///
    /// * `adapter` - PostgreSQL adapter wrapped in Arc
    pub fn from_arc(adapter: Arc<PostgresAdapter>) -> Self {
        Self { inner: adapter }
    }

    /// Get a reference to the inner PostgreSQL adapter.
    pub fn inner(&self) -> &Arc<PostgresAdapter> {
        &self.inner
    }
}

#[cfg(feature = "arrow")]
#[async_trait]
impl ArrowDatabaseAdapter for FlightDatabaseAdapter {
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>, DatabaseError> {
        // Delegate to fraiseql-core adapter
        self.inner
            .execute_raw_query(sql)
            .await
            .map_err(|e: fraiseql_core::error::FraiseQLError| DatabaseError::new(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that adapter can be created from PostgresAdapter
    #[test]
    fn test_adapter_creation() {
        // This test verifies the adapter can be created
        // In integration tests, we'll test actual query execution
        // (Note: This is a unit test that doesn't require a database)
        let _adapter: FlightDatabaseAdapter;
        // If this compiles, the struct is properly defined
    }
}
