//! Database abstraction layer.
//!
//! This module provides database-agnostic access to multiple database backends:
//! - PostgreSQL (primary, full feature set)
//! - MySQL (secondary support)
//! - SQLite (local dev, testing)
//! - SQL Server (enterprise)
//!
//! # Architecture
//!
//! The database layer follows a trait-based design:
//!
//! - `DatabaseAdapter` - Core trait for query execution
//! - `WhereClauseGenerator` - Database-specific WHERE SQL generation
//! - `ConnectionPool` - Connection pooling abstraction
//!
//! # Example
//!
//! ```rust,no_run
//! use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator, postgres::PostgresAdapter};
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create PostgreSQL adapter
//! let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
//!
//! // Build WHERE clause
//! let where_clause = WhereClause::Field {
//!     path: vec!["email".to_string()],
//!     operator: WhereOperator::Icontains,
//!     value: json!("example.com"),
//! };
//!
//! // Execute query
//! let results = adapter
//!     .execute_where_query("v_user", Some(&where_clause), None, None)
//!     .await?;
//!
//! println!("Found {} users", results.len());
//! # Ok(())
//! # }
//! ```

pub mod traits;
pub mod types;
pub mod where_clause;

#[cfg(feature = "postgres")]
pub mod postgres;

// TODO: Phase 2 Extension - Add MySQL, SQLite, SQL Server adapters
// #[cfg(feature = "mysql")]
// pub mod mysql;
//
// #[cfg(feature = "sqlite")]
// pub mod sqlite;
//
// #[cfg(feature = "sqlserver")]
// pub mod sqlserver;

// Re-export commonly used types
pub use traits::DatabaseAdapter;
pub use types::{DatabaseType, JsonbValue, PoolMetrics};
pub use where_clause::{WhereClause, WhereOperator};

#[cfg(feature = "postgres")]
pub use postgres::PostgresAdapter;
