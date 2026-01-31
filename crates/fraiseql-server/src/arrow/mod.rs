//! Arrow Flight server integration for high-performance columnar data transfer.
//!
//! This module provides Arrow Flight server integration that enables clients
//! to fetch data via the Apache Arrow Flight protocol for efficient,
//! high-performance data retrieval.
//!
//! # Features
//!
//! - Arrow Flight gRPC service
//! - Support for ta_* materialized tables
//! - Schema registry with pre-compiled Arrow schemas
//! - Zero-copy row-to-Arrow conversion
//!
//! # Usage
//!
//! This module is only available when the "arrow" feature is enabled.

#[cfg(feature = "arrow")]
pub mod database_adapter;

#[cfg(feature = "arrow")]
use std::sync::Arc;

#[cfg(feature = "arrow")]
pub use database_adapter::FlightDatabaseAdapter;
#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
#[cfg(feature = "arrow")]
use fraiseql_core::db::postgres::PostgresAdapter;

/// Create an Arrow Flight service with a real PostgreSQL database adapter.
///
/// # Arguments
///
/// * `adapter` - PostgreSQL adapter from fraiseql-core
///
/// # Returns
///
/// FraiseQLFlightService configured with the real database adapter
///
/// # Example
///
/// ```rust,ignore
/// let pg_adapter = PostgresAdapter::new(&db_url).await?;
/// let flight_service = create_flight_service(Arc::new(pg_adapter));
/// ```
#[cfg(feature = "arrow")]
pub fn create_flight_service(adapter: Arc<PostgresAdapter>) -> FraiseQLFlightService {
    let flight_adapter = FlightDatabaseAdapter::from_arc(adapter);

    // Create Flight service with real database adapter
    FraiseQLFlightService::new_with_db(Arc::new(flight_adapter))
}
