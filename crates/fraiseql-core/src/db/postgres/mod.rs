//! PostgreSQL database adapter.
//!
//! Provides connection pooling and query execution for PostgreSQL.

mod adapter;
mod where_generator;

pub use adapter::PostgresAdapter;
pub use where_generator::PostgresWhereGenerator;
