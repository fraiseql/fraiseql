//! Database connection and query execution layer for `PostgreSQL`.
//!
//! This module provides:
//! - Connection pooling with deadpool-postgres
//! - Query execution with streaming results
//! - Transaction management
//! - Connection lifecycle management
//!
//! Architecture:
//! - `pool.rs`: Connection pool management
//! - `pool_production.rs`: Production pool with SSL/TLS (Phase 1)
//! - `runtime.rs`: Global Tokio runtime (Phase 1)
//! - `health.rs`: Health check utilities (Phase 1)
//! - `transaction.rs`: ACID transaction support
//! - `types.rs`: Type definitions and configurations
//! - `where_builder.rs`: WHERE clause construction
//! - `query.rs`: Query execution and result handling

pub mod errors;
pub mod health;
pub mod metrics;
pub mod mutex_recovery;
pub mod pool;
pub mod pool_config;
pub mod pool_production;
pub mod prototype; // Phase 0: Async bridge prototype
pub mod query;
pub mod runtime;
pub mod transaction;
pub mod types;
pub mod where_builder;

// Re-export main types
pub use errors::{DatabaseError, DatabaseResult};
pub use health::{HealthCheckResult, PoolHealthStats};
pub use metrics::{MetricsSnapshot, PoolMetrics};
pub use mutex_recovery::recover_from_poisoned;
pub use pool::DatabasePool;
pub use pool_config::{DatabaseConfig, SslMode};
pub use pool_production::{PoolStats, ProductionPool};
pub use prototype::PrototypePool; // Phase 0: Export prototype for testing
pub use query::QueryExecutor;
pub use runtime::{ffi_runtime, init_runtime, runtime, RuntimeConfig, RuntimeStats};
pub use transaction::Transaction;
pub use types::*;
pub use where_builder::{WhereBuilder, WhereCondition};
