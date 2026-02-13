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

pub mod collation;
pub mod identifier;
pub mod path_escape;
pub mod projection_generator;
pub mod traits;
pub mod types;
pub mod where_clause;
pub mod where_sql_generator;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "sqlserver")]
pub mod sqlserver;

#[cfg(feature = "wire-backend")]
pub mod wire_pool;

#[cfg(feature = "wire-backend")]
pub mod fraiseql_wire_adapter;

// Re-export commonly used types
pub use collation::{CollationCapabilities, CollationMapper};
#[cfg(feature = "wire-backend")]
pub use fraiseql_wire_adapter::FraiseWireAdapter;
pub use identifier::{
    quote_mysql_identifier, quote_postgres_identifier, quote_sqlite_identifier,
    quote_sqlserver_identifier,
};
#[cfg(feature = "mysql")]
pub use mysql::MySqlAdapter;
#[cfg(feature = "postgres")]
pub use postgres::{PostgresAdapter, PostgresIntrospector};
pub use projection_generator::PostgresProjectionGenerator;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteAdapter;
#[cfg(feature = "sqlserver")]
pub use sqlserver::SqlServerAdapter;
pub use traits::{DatabaseAdapter, DatabaseCapabilities};
pub use types::{DatabaseType, JsonbValue, PoolMetrics};
pub use where_clause::{HavingClause, WhereClause, WhereOperator};
pub use where_sql_generator::WhereSqlGenerator;
