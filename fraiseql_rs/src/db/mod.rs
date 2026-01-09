//! Database connection and query execution layer for `PostgreSQL`.
//!
//! This module provides:
//! - Connection pooling with deadpool-postgres
//! - Query execution with streaming results
//! - Transaction management
//! - Connection lifecycle management
//! - Safe parameter binding (Phase 3.2)
//!
//! Architecture:
//! - `pool.rs`: Connection pool management
//! - `pool_production.rs`: Production pool with SSL/TLS (Phase 1)
//! - `runtime.rs`: Global Tokio runtime (Phase 1)
//! - `health.rs`: Health check utilities (Phase 1)
//! - `transaction.rs`: ACID transaction support
//! - `types.rs`: Type definitions and configurations
//! - `query_builder.rs`: SQL query building (Phase 2 - Python migration)
//! - `where_builder.rs`: WHERE clause construction
//! - `query.rs`: Query execution and result handling
//! - `parameter_binding.rs`: Safe parameter binding (Phase 3.2)

pub mod errors;
pub mod health;
pub mod metrics;
pub mod mutex_recovery;
pub mod parameter_binding; // Phase 3.2: Safe parameter binding
pub mod pool; // Pool abstraction traits
pub mod pool_config;
pub mod pool_production;
pub mod prototype; // Phase 0: Async bridge prototype
pub mod query;
pub mod query_builder;
pub mod runtime;
pub mod transaction;
pub mod types;
pub mod where_builder;

// Re-export main types
pub use mutex_recovery::recover_from_poisoned;
pub use pool::{DatabasePool, PoolBackend}; // Pool abstraction + Python binding
pub use pool_config::DatabaseConfig;
pub use pool_production::ProductionPool;
// Phase 0: Export prototype for testing
