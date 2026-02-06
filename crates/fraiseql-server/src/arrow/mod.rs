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
pub mod executor_wrapper;

#[cfg(feature = "arrow")]
use std::sync::Arc;

#[cfg(feature = "arrow")]
pub use database_adapter::FlightDatabaseAdapter;
#[cfg(feature = "arrow")]
pub use executor_wrapper::ExecutorQueryAdapter;
#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;

#[cfg(all(feature = "arrow", not(feature = "wire-backend")))]
use fraiseql_core::db::postgres::PostgresAdapter;

#[cfg(all(feature = "arrow", feature = "wire-backend"))]
use fraiseql_core::db::FraiseWireAdapter;

/// Create an Arrow Flight service with a real database adapter.
///
/// Supports both PostgreSQL and FraiseQL Wire adapters depending on feature flags:
/// - Default: PostgreSQL adapter for traditional database connections
/// - `wire-backend` feature: FraiseQL Wire adapter for streaming JSON queries with low memory overhead
///
/// # Arguments
///
/// * `adapter` - Database adapter from fraiseql-core (PostgreSQL or Wire depending on features)
///
/// # Returns
///
/// FraiseQLFlightService configured with the real database adapter
///
/// # Example
///
/// ```rust,ignore
/// // PostgreSQL (default)
/// let pg_adapter = PostgresAdapter::new(&db_url).await?;
/// let flight_service = create_flight_service(Arc::new(pg_adapter));
///
/// // FraiseQL Wire (with wire-backend feature)
/// # #[cfg(feature = "wire-backend")]
/// # {
/// let wire_adapter = FraiseWireAdapter::new(&db_url);
/// let flight_service = create_flight_service(Arc::new(wire_adapter));
/// # }
/// ```
#[cfg(all(feature = "arrow", not(feature = "wire-backend")))]
pub fn create_flight_service(adapter: Arc<PostgresAdapter>) -> FraiseQLFlightService {
    let flight_adapter = FlightDatabaseAdapter::from_arc(adapter);

    // Create Flight service with PostgreSQL adapter
    FraiseQLFlightService::new_with_db(Arc::new(flight_adapter))
}

#[cfg(all(feature = "arrow", feature = "wire-backend"))]
pub fn create_flight_service(adapter: Arc<FraiseWireAdapter>) -> FraiseQLFlightService {
    let flight_adapter = FlightDatabaseAdapter::from_arc(adapter);

    // Create Flight service with FraiseQL Wire adapter
    FraiseQLFlightService::new_with_db(Arc::new(flight_adapter))
}
