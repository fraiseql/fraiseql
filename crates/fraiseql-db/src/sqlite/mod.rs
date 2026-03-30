//! SQLite database adapter.
//!
//! Provides connection pooling and query execution for SQLite.
//! Ideal for local development and testing.

mod adapter;
mod helpers;
pub mod introspector;
mod where_generator;

#[cfg(test)]
mod adapter_tests;

pub use adapter::SqliteAdapter;
pub use introspector::*;
pub use where_generator::SqliteWhereGenerator;
