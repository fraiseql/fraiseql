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
//! ```rust,no_run
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

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

// Core modules
pub mod error;
pub mod schema;
pub mod config;

// Compilation (Phase 4)
pub mod compiler;

// Execution (Phase 5)
pub mod runtime;

// Infrastructure
pub mod db;
pub mod cache;
pub mod security;
pub mod validation;
pub mod apq;
pub mod utils;

// Re-exports for convenience
pub use error::{FraiseQLError, Result};
pub use schema::CompiledSchema;
pub use config::FraiseQLConfig;

/// Version of the FraiseQL core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version
pub const MSRV: &str = "1.75";
