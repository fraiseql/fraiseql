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
// Pedantic allows — workspace sets `pedantic = deny`. These are grouped and
// justified per Q3 / F053 (see `IMPROVEMENTS.md`). Two categories:
//
//   1. Wire-protocol cast suppressions — binary decoders that are statically
//      bounded by the protocol contract (counts, lengths, offsets fit in the
//      target type by construction).
//   2. Crate-wide style preferences — binary-protocol code prizes locality and
//      explicitness over the rust-idiomatic alternatives clippy prefers.
//
// Test-bleed lints (`unreadable_literal`, `explicit_iter_loop`) have been
// moved to per-module `#![allow]` inside `mod tests` so the suppression
// scope matches the locus of the fires.
//
// === Wire-protocol cast suppressions (binary decoders, statically bounded) ===
#![allow(clippy::cast_precision_loss)] // Reason: intentional f64 conversions for metrics counters
#![allow(clippy::cast_possible_truncation)] // Reason: intentional usize/u64 casts for buffer sizes
#![allow(clippy::cast_sign_loss)] // Reason: duration/size values are always positive
#![allow(clippy::cast_possible_wrap)] // Reason: byte counts within positive range
#![allow(clippy::format_push_string)] // Reason: incremental query string building
#![allow(clippy::needless_continue)] // Reason: explicit continues in wire protocol loops
#![allow(clippy::iter_with_drain)] // Reason: drain pattern for ownership transfer in buffers
#![allow(clippy::no_effect_underscore_binding)] // Reason: placeholder bindings for protocol fields
// === Crate-wide style preferences (binary-protocol locality and explicitness) ===
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use for clarity
#![allow(clippy::match_same_arms)] // Reason: explicit arms document each protocol variant
#![allow(clippy::manual_let_else)] // Reason: match with early return is clearer here
#![allow(clippy::needless_pass_by_value)] // Reason: API consistency with async trait bounds
#![allow(clippy::implicit_hasher)] // Reason: HashMap type params explicit at call sites
#![allow(clippy::doc_link_with_quotes)] // Reason: quoted protocol names in docs are intentional
#![allow(clippy::doc_markdown)] // Reason: parameter names in docstrings without backticks

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
