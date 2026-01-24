//! FraiseQL Arrow Flight integration.
//!
//! This crate provides Apache Arrow Flight support for FraiseQL, enabling:
//! - High-performance columnar data transfer (50x faster than JSON)
//! - Zero-copy deserialization in clients (Python, R, Java)
//! - Direct integration with data warehouses (ClickHouse, Snowflake)
//!
//! # Architecture
//!
//! ```text
//! Client                    fraiseql-server              fraiseql-core
//!   │                             │                            │
//!   ├─── DoGet(ticket) ──────────>│                            │
//!   │                             ├─── Execute query ─────────>│
//!   │                             │<─── SQL rows ──────────────┤
//!   │                             ├─── Convert to Arrow ───────│
//!   │<─── Arrow RecordBatch ──────┤                            │
//!   │<─── Arrow RecordBatch ──────┤                            │
//!   │                             │                            │
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use fraiseql_arrow::flight_server::FraiseQLFlightService;
//! use tonic::transport::Server;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let service = FraiseQLFlightService::new();
//!     let addr = "0.0.0.0:50051".parse()?;
//!
//!     Server::builder()
//!         .add_service(service.into_server())
//!         .serve(addr)
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod convert;
pub mod db_convert;
pub mod error;
pub mod event_schema;
pub mod flight_server;
pub mod metadata;
pub mod schema;
pub mod schema_gen;
pub mod ticket;

pub use error::{ArrowFlightError, Result};
pub use flight_server::FraiseQLFlightService;
pub use metadata::SchemaRegistry;
pub use ticket::FlightTicket;
