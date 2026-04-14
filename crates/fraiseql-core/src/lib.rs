//! # FraiseQL Core
//!
//! Core execution engine for FraiseQL v2 - A compiled GraphQL execution engine.

//! ## Architecture
//!
//! FraiseQL v2 compiles GraphQL schemas into optimized SQL execution plans at build time,
//! eliminating runtime overhead and enabling deterministic, high-performance query execution.
//!
//! ### Key Components
//!
//! - **Schema**: Compiled schema representation (types, fields, SQL mappings)
//! - **Compiler**: Parsing, validation, and runtime sub-modules (aggregation, fact tables, window functions)
//! - **Runtime**: Compiled query executor
//! - **Database**: Connection pooling and transaction management
//! - **Cache**: Query result caching with coherency
//! - **Security**: Authentication, authorization, and audit
//! - **APQ**: Automatic Persisted Queries
//!
//! ## Compilation Flow
//!
//! ```text
//! Python/TypeScript Decorators
//!         ↓
//!    JSON Schema
//!         ↓
//!  SchemaConverter (fraiseql-cli)
//!    ↙    ↓    ↘
//! Parse Validate Convert
//!         ↓
//!  CompiledSchema.json
//!         ↓
//!      Runtime
//!    ↙    ↓    ↘
//! Match Execute Project
//!         ↓
//!   GraphQL Response
//! ```
//!
//! ## Example
//!
//! ```no_run
//! // Requires: a compiled schema file and a live PostgreSQL database.
//! // See: tests/integration/ for runnable examples.
//! use fraiseql_core::schema::CompiledSchema;
//! use fraiseql_core::runtime::Executor;
//! use fraiseql_core::db::postgres::PostgresAdapter;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let schema_json = r#"{"types":[],"queries":[]}"#;
//! // Load compiled schema
//! let schema = CompiledSchema::from_json(schema_json)?;
//!
//! // Create executor (db_pool is a DatabaseAdapter implementation)
//! let db_pool = Arc::new(PostgresAdapter::new("postgresql://localhost/mydb").await?);
//! let executor = Executor::new(schema, db_pool);
//!
//! // Execute query
//! let query = r#"query { users { id name } }"#;
//! let result = executor.execute(query, None).await?;
//!
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

// Core modules
pub mod config;
pub mod error;
pub mod http;
pub mod schema;

// Compilation layer
pub mod compiler;

// Execution layer
pub mod runtime;

// GraphQL parsing and query processing
pub mod graphql;

// Infrastructure
pub mod apq;
pub mod cache;
pub use fraiseql_db as db;
#[cfg(feature = "schema-lint")]
pub mod design;
#[cfg(feature = "federation")]
pub use fraiseql_federation as federation;
pub mod filters;
pub mod security;
pub mod tenancy;
pub mod types;
pub mod utils;
pub mod validation;

pub mod prelude;

// Re-exports for convenience
pub use config::FraiseQLConfig;
pub use error::{FraiseQLError, Result};
pub use schema::CompiledSchema;
pub use tenancy::TenantContext;

/// Version of the FraiseQL core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version
pub const MSRV: &str = "1.88";
