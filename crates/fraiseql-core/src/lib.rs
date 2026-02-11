//! # FraiseQL Core
//!
//! Core execution engine for FraiseQL v2 - A compiled GraphQL execution engine.
//!
//! ## Architecture
//!
//! FraiseQL v2 compiles GraphQL schemas into optimized SQL execution plans at build time,
//! eliminating runtime overhead and enabling deterministic, high-performance query execution.
//!
//! ### Key Components
//!
//! - **Schema**: Compiled schema representation (reused from v1)
//! - **Compiler**: GraphQL schema → SQL template compiler (new for v2)
//! - **Runtime**: Compiled query executor (new for v2)
//! - **Database**: Connection pooling and transaction management (from v1)
//! - **Cache**: Query result caching with coherency (from v1)
//! - **Security**: Authentication, authorization, and audit (from v1)
//! - **APQ**: Automatic Persisted Queries (from v1)
//!
//! ## Compilation Flow
//!
//! ```text
//! Python/TypeScript Decorators
//!         ↓
//!    JSON Schema
//!         ↓
//!     Compiler
//!    ↙    ↓    ↘
//! Parse Validate Codegen
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
//! ```ignore
//! use fraiseql_core::schema::CompiledSchema;
//! use fraiseql_core::runtime::Executor;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load compiled schema
//! let schema = CompiledSchema::from_file("schema.compiled.json")?;
//!
//! // Create executor
//! let executor = Executor::new(schema, db_pool).await?;
//!
//! // Execute query
//! let query = r#"query { users { id name } }"#;
//! let result = executor.execute(query, None).await?;
//!
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

// Core modules
pub mod config;
pub mod error;
pub mod schema;

// Compilation layer
pub mod compiler;

// Execution layer
pub mod runtime;

// GraphQL parsing and query processing
pub mod graphql;

// Infrastructure
pub mod apq;
pub mod audit;
pub mod cache;
pub mod db;
pub mod design;
pub mod federation;
pub mod filters;
pub mod observability;
pub mod security;
pub mod tenancy;
pub mod utils;
pub mod validation;

// Arrow Flight integration (optional)
#[cfg(feature = "arrow")]
pub mod arrow_executor;

// Re-exports for convenience
pub use config::FraiseQLConfig;
pub use error::{FraiseQLError, Result};
pub use schema::CompiledSchema;
pub use tenancy::TenantContext;

/// Version of the FraiseQL core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version
pub const MSRV: &str = "1.75";
