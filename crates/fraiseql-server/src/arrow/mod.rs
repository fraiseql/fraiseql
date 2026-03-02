//! Arrow Flight adapter layer for `fraiseql-server`.
//!
//! This module is a **thin adapter** (~270 lines) that bridges fraiseql-core's
//! database adapters to the [`fraiseql_arrow`] crate's trait interfaces and manages
//! the Flight gRPC server lifecycle (port 50051, graceful shutdown).
//!
//! # Architecture
//!
//! Arrow Flight support uses a library/consumer split:
//!
//! - [`fraiseql_arrow`] (the `fraiseql-arrow` crate) — full Arrow Flight gRPC
//!   implementation, database-agnostic via `DatabaseAdapter` and `QueryExecutor` traits
//! - This module (`fraiseql-server/src/arrow`) — thin adapter layer that bridges
//!   `fraiseql-core` adapters to the `fraiseql-arrow` traits
//!
//! The Flight gRPC server binds on port 50051 alongside the HTTP server (port 3000).
//! Enable with `--features arrow`.
//!
//! # Relationship to `fraiseql-arrow`
//!
//! This module does **not** re-implement the Arrow Flight protocol. All Flight logic
//! (authentication, streaming, caching, JSON↔Arrow conversion) lives in the
//! [`fraiseql_arrow`] library crate. This module provides:
//!
//! - [`FlightDatabaseAdapter`]: Wraps fraiseql-core adapters (Postgres, Wire) to
//!   implement `fraiseql_arrow::DatabaseAdapter`
//! - [`ExecutorQueryAdapter`]: Wraps `Executor<A>` to implement
//!   `fraiseql_arrow::QueryExecutor` (type erasure)
//! - [`create_flight_service`]: Factory that assembles a configured
//!   `FraiseQLFlightService` from core adapters
//!
//! # Usage
//!
//! This module is only available when the `arrow` feature is enabled.

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
#[cfg(all(feature = "arrow", feature = "wire-backend"))]
use fraiseql_core::db::FraiseWireAdapter;
#[cfg(all(feature = "arrow", not(feature = "wire-backend")))]
use fraiseql_core::db::postgres::PostgresAdapter;

/// Create an Arrow Flight service with a real database adapter.
///
/// Supports both PostgreSQL and FraiseQL Wire adapters depending on feature flags:
/// - Default: PostgreSQL adapter for traditional database connections
/// - `wire-backend` feature: FraiseQL Wire adapter for streaming JSON queries with low memory
///   overhead
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
