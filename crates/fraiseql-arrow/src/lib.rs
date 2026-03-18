//! FraiseQL Arrow Flight integration.
//!
//! This crate is the **library implementation** of FraiseQL's Arrow Flight support.
//! It provides [`FraiseQLFlightService`], the gRPC service that handles all Flight
//! protocol operations (handshake, `DoGet`, `DoPut`, schema introspection, etc.).
//!
//! # Relationship to `fraiseql-server`
//!
//! `fraiseql-arrow` and `fraiseql-server::arrow` are **not duplicates** — they form a
//! library/consumer pair:
//!
//! - **`fraiseql-arrow`** (this crate): Implements the Arrow Flight protocol on top of abstract
//!   `DatabaseAdapter` and `QueryExecutor` traits. Contains all Flight logic: authentication,
//!   caching, streaming, JSON↔Arrow conversion, schema registry.
//!
//! - **`fraiseql-server::arrow`**: Thin adapter layer (~270 lines) that wraps fraiseql-core
//!   database adapters to this crate's trait interfaces and manages the Flight gRPC server
//!   lifecycle (port 50051, graceful shutdown).
//!
//! # Features
//!
//! - High-performance columnar data transfer
//! - Zero-copy deserialization in clients (Python, R, Java)
//! - Direct integration with data warehouses (`ClickHouse`, Snowflake)
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
// Pedantic allows — workspace sets pedantic = deny. Suppressed for Arrow Flight crate.
#![allow(clippy::cast_possible_truncation)] // Reason: intentional casts for Arrow column indices
#![allow(clippy::format_push_string)] // Reason: incremental schema/query string building
#![allow(clippy::implicit_hasher)] // Reason: HashMap params explicit at call sites
#![allow(clippy::match_same_arms)] // Reason: explicit arms document each Arrow type
#![allow(clippy::needless_pass_by_value)] // Reason: API consistency with tonic trait bounds
#![allow(clippy::doc_link_with_quotes)] // Reason: quoted type names intentional in docs
#![allow(clippy::ref_option)] // Reason: Option refs match tonic/Arrow API
#![allow(clippy::derive_partial_eq_without_eq)] // Reason: some types contain f64 fields
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use
#![allow(clippy::needless_raw_string_hashes)] // Reason: raw strings in test fixtures preserved

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
pub use db::{ArrowDatabaseAdapter, DatabaseError, DatabaseResult};
pub use error::{ArrowFlightError, Result};
pub use event_storage::{ArrowEventStorage, HistoricalEvent};
pub use exchange_protocol::{ExchangeMessage, RequestType};
pub use export::{BatchStats, BulkExporter, ExportFormat};
pub use flight_server::{FraiseQLFlightService, QueryExecutor};
pub use metadata::SchemaRegistry;
pub use subscription::{EventSubscription, SubscriptionManager};
pub use ticket::FlightTicket;
