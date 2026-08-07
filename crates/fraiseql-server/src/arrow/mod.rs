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
//! - [`fraiseql_arrow`] (the `fraiseql-arrow` crate) — full Arrow Flight gRPC implementation,
//!   database-agnostic via `ArrowDatabaseAdapter` and `QueryExecutor` traits
//! - This module (`fraiseql-server/src/arrow`) — thin adapter layer that bridges `fraiseql-core`
//!   adapters to the `fraiseql-arrow` traits
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
//! - [`FlightDatabaseAdapter`]: Wraps fraiseql-core adapters (Postgres, Wire) to implement
//!   `fraiseql_arrow::ArrowDatabaseAdapter`
//! - [`ExecutorQueryAdapter`]: Wraps `Executor<A>` to implement `fraiseql_arrow::QueryExecutor`
//!   (type erasure)
//! - [`create_flight_service`]: Factory that assembles a configured `FraiseQLFlightService` from
//!   core adapters
//!
//! # Usage
//!
//! This module is only available when the `arrow` feature is enabled.

#[cfg(feature = "arrow")]
pub mod database_adapter;
#[cfg(feature = "arrow")]
pub mod executor_wrapper;

#[cfg(test)]
mod tests;

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
/// **No `QueryExecutor` is attached — deliberately.** The Flight GraphQL paths
/// therefore refuse every ad-hoc query fail-closed ("no executor configured").
/// Wiring one in must go through the tenant-dispatch/policy seam (tenant
/// resolution, suspension, quotas, trusted documents), not a bare
/// `set_executor`, or Flight becomes the one transport that skips them.
///
/// Supports both PostgreSQL and FraiseQL Wire adapters depending on feature flags:
/// - Default: PostgreSQL adapter for traditional database connections
/// - `wire-backend` feature: FraiseQL Wire adapter for streaming JSON queries with low memory
///   overhead
///
/// `upload_tables` is the operator's `flight_upload_tables` allow-list (#953). An
/// empty slice leaves `Upload` **disabled**, which is the default and the only safe
/// one: the target table is named by the client and the write skips the mutation
/// pipeline. This is the config seam — without it `with_upload_tables` would be a
/// library-only setter with no caller, and no operator could reach the feature.
///
/// # Arguments
///
/// * `adapter` - Database adapter from fraiseql-core (PostgreSQL or Wire depending on features)
/// * `upload_tables` - Tables an authenticated client may `Upload` into; empty disables Upload
///
/// # Returns
///
/// `FraiseQLFlightService` configured with the real database adapter
///
/// # Example
///
/// ```text
/// // PostgreSQL (default)
/// let pg_adapter = PostgresAdapter::new(&db_url).await?;
/// let flight_service = create_flight_service(Arc::new(pg_adapter), &config.flight_upload_tables);
///
/// // FraiseQL Wire (with the `wire-backend` feature)
/// let wire_adapter = FraiseWireAdapter::new(&db_url);
/// let flight_service = create_flight_service(Arc::new(wire_adapter), &[]);
/// ```
#[cfg(all(feature = "arrow", not(feature = "wire-backend")))]
#[must_use]
pub fn create_flight_service(
    adapter: Arc<PostgresAdapter>,
    upload_tables: &[String],
) -> FraiseQLFlightService {
    let flight_adapter = FlightDatabaseAdapter::from_arc(adapter);

    // Create Flight service with PostgreSQL adapter
    let service = FraiseQLFlightService::new_with_db(Arc::new(flight_adapter));
    apply_upload_allow_list(service, upload_tables)
}

/// Apply the operator's Upload allow-list, leaving `Upload` disabled when empty.
///
/// An empty list must stay `None` on the service, not `Some(∅)`: both refuse every
/// table, but only `None` reports "Upload is disabled" rather than "not permitted
/// for this table", and that is the message that tells an operator they configured
/// nothing.
#[cfg(feature = "arrow")]
fn apply_upload_allow_list(
    service: FraiseQLFlightService,
    upload_tables: &[String],
) -> FraiseQLFlightService {
    if upload_tables.is_empty() {
        service
    } else {
        service.with_upload_tables(upload_tables.iter().cloned())
    }
}

/// Create an Arrow Flight service backed by the fraiseql-wire streaming adapter.
///
/// Requires both the `arrow` and `wire-backend` features.
#[cfg(all(feature = "arrow", feature = "wire-backend"))]
#[must_use]
pub fn create_flight_service(
    adapter: Arc<FraiseWireAdapter>,
    upload_tables: &[String],
) -> FraiseQLFlightService {
    let flight_adapter = FlightDatabaseAdapter::from_arc(adapter);

    // Create Flight service with FraiseQL Wire adapter. Note that the wire adapter
    // does not implement `execute_gated_upload`, so an allow-listed Upload is still
    // refused for want of an atomic write path — allow-listing it here cannot open
    // the surface (#953).
    let service = FraiseQLFlightService::new_with_db(Arc::new(flight_adapter));
    apply_upload_allow_list(service, upload_tables)
}
