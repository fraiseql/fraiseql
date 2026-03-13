//! PostgreSQL database adapter.
//!
//! Provides connection pooling and query execution for PostgreSQL.

mod adapter;
mod introspector;
mod where_generator;

pub use adapter::PostgresAdapter;
pub use introspector::PostgresIntrospector;
pub use where_generator::{IndexedColumnsCache, PostgresWhereGenerator};
