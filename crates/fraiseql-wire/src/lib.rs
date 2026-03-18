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
// Pedantic allows — workspace sets pedantic = deny. These are suppressed for this crate.
#![allow(clippy::missing_panics_doc)] // Reason: panics eliminated by design in this crate
#![allow(clippy::cast_precision_loss)] // Reason: intentional f64 conversions for metrics counters
#![allow(clippy::cast_possible_truncation)] // Reason: intentional usize/u64 casts for buffer sizes
#![allow(clippy::cast_sign_loss)] // Reason: duration/size values are always positive
#![allow(clippy::cast_possible_wrap)] // Reason: byte counts within positive range
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use for clarity
#![allow(clippy::format_push_string)] // Reason: incremental query string building
#![allow(clippy::needless_continue)] // Reason: explicit continues in wire protocol loops
#![allow(clippy::match_same_arms)] // Reason: explicit arms document each protocol variant
#![allow(clippy::manual_let_else)] // Reason: match with early return is clearer here
#![allow(clippy::iter_with_drain)] // Reason: drain pattern for ownership transfer in buffers
#![allow(clippy::no_effect_underscore_binding)] // Reason: placeholder bindings for protocol fields
#![allow(clippy::needless_pass_by_value)] // Reason: API consistency with async trait bounds
#![allow(clippy::implicit_hasher)] // Reason: HashMap type params explicit at call sites
#![allow(clippy::doc_link_with_quotes)] // Reason: quoted protocol names in docs are intentional
#![allow(clippy::unreadable_literal)] // Reason: large numeric literals in test assertions and protocol sizes
#![allow(clippy::doc_markdown)] // Reason: parameter names in docstrings without backticks
#![allow(clippy::map_unwrap_or)] // Reason: map/unwrap patterns in test examples
#![allow(clippy::explicit_iter_loop)] // Reason: explicit iteration in test setup
#![allow(clippy::range_plus_one)] // Reason: range expressions in examples

#[cfg(not(unix))]
compile_error!("fraiseql-wire only supports Unix-like operating systems (Linux, macOS).");

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
pub use error::{Result, WireError};
pub use operators::{Field, OrderByClause, SortOrder, Value, WhereOperator};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
