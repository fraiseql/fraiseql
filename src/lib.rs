//! fraiseql-wire: Streaming JSON query engine for Postgres 17
//!
//! This crate provides a minimal, async Rust query engine that streams JSON
//! data from Postgres with low latency and bounded memory usage.
//!
//! # Supported Query Shape
//!
//! ```sql
//! SELECT data
//! FROM v_{entity}
//! WHERE predicate
//! [ORDER BY expression]
//! ```

#![warn(missing_docs, rust_2018_idioms)]

pub mod auth;
pub mod client;
pub mod connection;
pub mod error;
pub mod json;
pub mod metrics;
pub mod operators;
pub mod protocol;
pub mod stream;
pub mod util;

// Re-export commonly used types
pub use client::FraiseClient;
pub use error::{Error, Result};
pub use operators::{Field, OrderByClause, SortOrder, Value, WhereOperator};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
