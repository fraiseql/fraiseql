//! FraiseQL Arrow Flight integration.
//!
//! This crate provides Apache Arrow Flight support for FraiseQL, enabling:
//! - High-performance columnar data transfer
//! - Zero-copy deserialization in clients (Python, R, Java)
//! - Direct integration with data warehouses (ClickHouse, Snowflake)
//!
//! Arrow columnar format provides better throughput and memory efficiency compared to
//! row-oriented JSON. See `benches/arrow_vs_json_serialization.rs` for performance measurements.
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

pub mod cache;
pub mod convert;
pub mod db;
pub mod db_convert;
pub mod error;
pub mod event_schema;
pub mod event_storage;
pub mod exchange_protocol;
pub mod export;
pub mod flight_server;
pub mod metadata;
pub mod schema;
pub mod schema_gen;
pub mod subscription;
pub mod ticket;

#[cfg(feature = "clickhouse")]
pub mod clickhouse_sink;

pub use cache::QueryCache;
#[cfg(feature = "clickhouse")]
pub use clickhouse_sink::{ClickHouseSink, ClickHouseSinkConfig, EventRow};
pub use db::{DatabaseAdapter, DatabaseError, DatabaseResult};
pub use error::{ArrowFlightError, Result};
pub use event_storage::{EventStorage, HistoricalEvent};
pub use exchange_protocol::{ExchangeMessage, RequestType};
pub use export::{BatchStats, BulkExporter, ExportFormat};
pub use flight_server::{FraiseQLFlightService, QueryExecutor};
pub use metadata::SchemaRegistry;
pub use subscription::{EventSubscription, SubscriptionManager};
pub use ticket::FlightTicket;
