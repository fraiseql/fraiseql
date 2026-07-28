//! PostgreSQL database adapter.
//!
//! Provides connection pooling and query execution for PostgreSQL.

mod adapter;
mod introspector;
mod tls;
mod where_generator;

pub use adapter::{PoolPrewarmConfig, PostgresAdapter, SearchPath};
pub use introspector::PostgresIntrospector;
pub use tls::{PostgresConnector, PostgresSslMode, PostgresTlsConfig};
pub use where_generator::{IndexedColumnsCache, PostgresWhereGenerator};
