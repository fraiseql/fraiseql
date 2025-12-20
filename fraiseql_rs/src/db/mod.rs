//! Database connection and query execution layer for PostgreSQL.
//!
//! This module provides:
//! - Connection pooling with deadpool-postgres
//! - Query execution with streaming results
//! - Transaction management
//! - Connection lifecycle management
//!
//! Architecture:
//! - `pool.rs`: Connection pool management
//! - `transaction.rs`: ACID transaction support
//! - `types.rs`: Type definitions and configurations
//! - `where_builder.rs`: WHERE clause construction
//! - `query.rs`: Query execution and result handling

pub mod pool;
pub mod query;
pub mod transaction;
pub mod types;
pub mod where_builder;

pub use pool::DatabasePool;
pub use query::QueryExecutor;
pub use transaction::Transaction;
pub use types::*;
pub use where_builder::{WhereBuilder, WhereCondition};