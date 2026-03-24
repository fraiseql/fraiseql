//! SQLite database adapter.
//!
//! Provides connection pooling and query execution for SQLite.
//! Ideal for local development and testing.

mod adapter;
mod helpers;
mod introspector;
mod where_generator;

pub use adapter::SqliteAdapter;
pub use introspector::SqliteIntrospector;
pub use where_generator::SqliteWhereGenerator;
