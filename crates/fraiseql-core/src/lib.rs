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
//! - **Compiler**: GraphQL schema → SQL template compiler
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
// The workspace Cargo.toml already enforces `deny` for clippy::all and clippy::pedantic
// via [workspace.lints.clippy]. Repeating `#![warn(...)]` here would downgrade those
// workspace-level denials for this crate, which is wrong. Suppressions below use
// `#![allow(...)]` which are still needed for legitimate per-crate overrides.
//
// Per-crate overrides for workspace pedantic denials.
// Each entry documents the trade-off. Crate-level allows are used only where the
// issue spans 300+ sites and inlining would create more noise than it removes.
// Targeted single-site suppressions use inline #[allow] — see the noted files.
//
// Reason: ~300+ existing doc comments use backticks without code fencing; converting
//         all of them is a separate cleanup tracked in the v2.2.0 backlog.
#![allow(clippy::doc_markdown)]
// Reason: explicit duplicate match arms clarify intent in complex match expressions
//         throughout the compiler; collapsing them would harm readability.
#![allow(clippy::match_same_arms)]
// Reason: schema compilation functions take type, context, config, security, and
//         modifier arguments; refactoring to builder structs is planned (v2.2.0).
#![allow(clippy::too_many_arguments)]
// Reason: `push_str(&format!(...))` is used in ~12 SQL builder sites (window.rs,
//         aggregation/, explain.rs, schema.rs) where it is clearer than `write!`.
#![allow(clippy::format_push_string)]
// Reason: `from_str`/`from_value` are schema-specific constructors intentionally
//         named to avoid confusion with the `FromStr` standard trait.
#![allow(clippy::should_implement_trait)]
// Reason: several public API functions take owned values for ergonomics; trait
//         implementations cannot change their signature to use references.
#![allow(clippy::needless_pass_by_value)]
// Reason: struct field initialisation uses `Default::default()` for alignment in
//         long struct literals (compiler/codegen.rs, validation/custom_type_registry/).
#![allow(clippy::default_trait_access)]
// NOTE: clippy::wildcard_imports and clippy::enum_glob_use are suppressed inline
//       at their specific sites (vault.rs, subscription/manager.rs, aggregation/expressions.rs).

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
