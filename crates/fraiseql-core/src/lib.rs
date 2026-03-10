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
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load compiled schema
//! let schema = CompiledSchema::from_file("schema.compiled.json")?;
//!
//! // Create executor (db_pool is a DatabaseAdapter implementation)
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
// Each entry documents the trade-off rather than being a silent suppression.
// Reason: ~300+ existing doc comments use backticks without code fencing; converting
//         all of them is a separate cleanup effort, not blocking current development.
#![allow(clippy::doc_markdown)]
// Reason: fraiseql-core has ~300+ public fallible functions; error-doc coverage is
//         a separate effort tracked in the backlog.
#![allow(clippy::missing_errors_doc)]
// Reason: explicit duplicate arms can clarify intent in complex match expressions.
#![allow(clippy::match_same_arms)]
// Reason: many intentional u64→u32 casts in cache shard index computation
//         where the value is always < 64.
#![allow(clippy::cast_possible_truncation)]
// Reason: intentional f64 conversions in latency histogram bucketing.
#![allow(clippy::cast_precision_loss)]
// Reason: intentional signed→unsigned conversions for offset/length calculations.
#![allow(clippy::cast_sign_loss)]
// Reason: schema compilation functions take type, context, config, security, and modifiers;
//         splitting into builder structs is planned but not done.
#![allow(clippy::too_many_arguments)]
// Reason: `push_str(&format!(...))` is sometimes clearer than `write!` in SQL builders.
#![allow(clippy::format_push_string)]
// Reason: wildcard imports used intentionally for enum variants in local match blocks.
#![allow(clippy::wildcard_imports)]
// Reason: fraiseql-core has ~300+ public functions; panic-doc coverage is
//         a separate effort tracked in the backlog (see U3 remediation plan).
#![allow(clippy::missing_panics_doc)]
// Reason: `from_str` / `from_value` are intentionally named differently from `FromStr`
//         to avoid confusion with the trait; they are schema-specific constructors.
#![allow(clippy::should_implement_trait)]
// Reason: some public API functions take owned values for ergonomics at call sites;
//         those that implement traits cannot change their signature.
#![allow(clippy::needless_pass_by_value)]
// Reason: `.map(...).unwrap_or(...)` is kept where a separate map_or would be harder to read
//         in multi-line chains; 31 call sites, cleaned up where straightforward.
#![allow(clippy::map_unwrap_or)]
// Reason: `HashMap::default()` is explicit about the concrete type at instantiation.
#![allow(clippy::default_trait_access)]
// Reason: explicit `if a > b { a - b } else { 0 }` is clearer than implicit saturating.
#![allow(clippy::implicit_saturating_sub)]
// Reason: wildcard enum imports improve readability in heavily match-driven modules.
#![allow(clippy::enum_glob_use)]
// Reason: test code legitimately allocates large test-data arrays on the stack;
//         the lint cannot attribute this to a specific source location (reports .clippy.toml:1).
#![allow(clippy::large_stack_arrays)]

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

// Re-exports for convenience
pub use config::FraiseQLConfig;
pub use error::{FraiseQLError, Result};
pub use schema::CompiledSchema;
pub use tenancy::TenantContext;

/// Version of the FraiseQL core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version
pub const MSRV: &str = "1.88";
