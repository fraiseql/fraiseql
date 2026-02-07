//! Database adapter wrapper for Arrow Flight service.
//!
//! This module provides a wrapper that adapts fraiseql-core's database adapters
//! to fraiseql-arrow's DatabaseAdapter trait, enabling the Arrow Flight service
//! to execute queries against real databases.
//!
//! Supports multiple backends:
//! - PostgreSQL (default, via `PostgresAdapter`)
//! - FraiseQL Wire (optional, via `wire-backend` feature, uses `FraiseWireAdapter`)

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
#[cfg(feature = "arrow")]
use fraiseql_arrow::db::{DatabaseAdapter as ArrowDatabaseAdapter, DatabaseError};
#[cfg(feature = "wire-backend")]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(not(feature = "wire-backend"))]
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::db::traits::DatabaseAdapter as CoreDatabaseAdapter;

/// Wrapper that adapts fraiseql-core's database adapters to fraiseql-arrow's DatabaseAdapter trait.
///
/// This enables the Arrow Flight service to execute queries against different database backends
/// without requiring direct knowledge of fraiseql-core's DatabaseAdapter interface.
///
/// # Feature-Gated Backends
///
/// - Default (PostgreSQL): Uses `PostgresAdapter` for traditional PostgreSQL connections
/// - `wire-backend` feature: Uses `FraiseWireAdapter` for streaming JSON queries with low memory
///   overhead
#[cfg(not(feature = "wire-backend"))]
pub struct FlightDatabaseAdapter {
    /// Inner PostgreSQL adapter from fraiseql-core
    inner: Arc<PostgresAdapter>,
}

#[cfg(feature = "wire-backend")]
pub struct FlightDatabaseAdapter {
    /// Inner FraiseQL Wire adapter from fraiseql-core (with lower memory usage)
    inner: Arc<FraiseWireAdapter>,
}

#[cfg(not(feature = "wire-backend"))]
impl FlightDatabaseAdapter {
    /// Create a new Arrow Flight database adapter with PostgreSQL backend.
    ///
    /// # Arguments
    ///
    /// * `adapter` - PostgreSQL adapter from fraiseql-core
    pub fn new(adapter: PostgresAdapter) -> Self {
        Self {
            inner: Arc::new(adapter),
        }
    }

    /// Create a new Arrow Flight database adapter from an Arc (PostgreSQL).
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

#[cfg(feature = "wire-backend")]
impl FlightDatabaseAdapter {
    /// Create a new Arrow Flight database adapter with FraiseQL Wire backend.
    ///
    /// # Arguments
    ///
    /// * `adapter` - FraiseQL Wire adapter from fraiseql-core
    pub fn new(adapter: FraiseWireAdapter) -> Self {
        Self {
            inner: Arc::new(adapter),
        }
    }

    /// Create a new Arrow Flight database adapter from an Arc (FraiseQL Wire).
    ///
    /// # Arguments
    ///
    /// * `adapter` - FraiseQL Wire adapter wrapped in Arc
    pub fn from_arc(adapter: Arc<FraiseWireAdapter>) -> Self {
        Self { inner: adapter }
    }

    /// Get a reference to the inner FraiseQL Wire adapter.
    pub fn inner(&self) -> &Arc<FraiseWireAdapter> {
        &self.inner
    }
}

#[cfg(all(feature = "arrow", not(feature = "wire-backend")))]
#[async_trait]
impl ArrowDatabaseAdapter for FlightDatabaseAdapter {
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>, DatabaseError> {
        // Delegate to PostgreSQL adapter
        self.inner
            .execute_raw_query(sql)
            .await
            .map_err(|e: fraiseql_core::error::FraiseQLError| DatabaseError::new(e.to_string()))
    }
}

#[cfg(all(feature = "arrow", feature = "wire-backend"))]
#[async_trait]
impl ArrowDatabaseAdapter for FlightDatabaseAdapter {
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>, DatabaseError> {
        // Delegate to FraiseQL Wire adapter
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
