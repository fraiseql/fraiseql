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
// Reason: builder pattern — callers chain these methods and the return is not "must check".
#![allow(clippy::return_self_not_must_use)]
// Reason: style preference; `format!("{}", x)` and `format!("{x}")` are both clear.
#![allow(clippy::uninlined_format_args)]
// Reason: some trait adapter methods accept &mut self or &self for API uniformity.
#![allow(clippy::unused_self)]
// Reason: some functions return Option<T> / Result<T> to match a trait signature.
#![allow(clippy::unnecessary_wraps)]
// Reason: builder methods and fluent APIs are too noisy with must_use everywhere.
#![allow(clippy::must_use_candidate)]
// Reason: fraiseql-core has ~300+ public fallible functions; error-doc coverage is
//         a separate effort tracked in the backlog.
#![allow(clippy::missing_errors_doc)]
// Reason: common naming convention in Rust; e.g. `schema::SchemaField` is idiomatic.
#![allow(clippy::module_name_repetitions)]
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
// Reason: style preference; suppressed workspace-wide but needed per-crate too.
#![allow(clippy::redundant_closure_for_method_calls)]
// Reason: explicit `.iter()` is sometimes clearer in SQL generation loops.
#![allow(clippy::explicit_iter_loop)]
// Reason: `if condition { 1 } else { 0 }` is sometimes clearer than `usize::from(cond)`.
#![allow(clippy::bool_to_int_with_if)]
// Reason: single-arm match with else can be clearer than if-let for complex branches.
#![allow(clippy::single_match_else)]
// Reason: wildcard imports used intentionally for enum variants in local match blocks.
#![allow(clippy::wildcard_imports)]
// Reason: config structs have many independent boolean flags that map directly to TOML.
//         Converting to bitflags would break serde deserialization.
#![allow(clippy::struct_excessive_bools)]
// Reason: fraiseql-core has ~300+ public functions; panic-doc coverage is
//         a separate effort tracked in the backlog.
#![allow(clippy::missing_panics_doc)]
// Reason: short variable names are idiomatic in SQL generation (lhs, rhs, sq, op).
#![allow(clippy::similar_names)]
// Reason: explicit match can be clearer than map_or_else for multi-line branches.
#![allow(clippy::option_if_let_else)]
// Reason: negative-first guard clauses are sometimes more readable.
#![allow(clippy::if_not_else)]
// Reason: `format!("{}", x)` is occasionally needed for type coercion via Display.
#![allow(clippy::useless_format)]
// Reason: `.unwrap_or(default_fn())` is sometimes clearer than `.unwrap_or_else(|| ...)`.
#![allow(clippy::or_fun_call)]
// Reason: some trait adapter methods are async for future extensibility.
#![allow(clippy::unused_async)]
// Reason: `from_str` / `from_value` are intentionally named differently from `FromStr`.
#![allow(clippy::should_implement_trait)]
// Reason: some public API functions take owned values for ergonomics at call sites.
#![allow(clippy::needless_pass_by_value)]
// Reason: explicit `x.checked_sub(y).unwrap_or(0)` is clearer than saturating_sub in
//         some contexts.
#![allow(clippy::manual_saturating_arithmetic)]
// Reason: wildcard arms after exhaustive matches can be clearer than listing all arms.
#![allow(clippy::match_wildcard_for_single_variants)]
// Reason: single-char string patterns are micro-optimizations not worth the noise.
#![allow(clippy::single_char_pattern)]
// Reason: doc links with quotes are a style choice in this codebase.
#![allow(clippy::doc_link_with_quotes)]
// Reason: nested if statements are sometimes clearer than `&&` conditions for readability.
#![allow(clippy::collapsible_if)]
// Reason: `.map(...).unwrap_or(...)` can be clearer than `.map_or(...)`.
#![allow(clippy::map_unwrap_or)]
// Reason: explicit match can be clearer than `.map(|x| ...)` for some transformations.
#![allow(clippy::manual_map)]
// Reason: `HashMap::default()` is explicit about the concrete type at instantiation.
#![allow(clippy::default_trait_access)]
// Reason: explicit `if a > b { a - b } else { 0 }` is clearer than implicit saturating.
#![allow(clippy::implicit_saturating_sub)]
// Reason: `&Vec<T>` is sometimes used for clarity at call sites that own the Vec.
#![allow(clippy::ptr_arg)]
// Reason: wildcard enum imports improve readability in heavily match-driven modules.
#![allow(clippy::enum_glob_use)]
// Reason: `.unwrap_or_default()` is correct; false positives with `or_insert_with`.
#![allow(clippy::unwrap_or_default)]
// Reason: closure is sometimes clearer than a method reference for non-obvious callables.
#![allow(clippy::redundant_closure)]
// Reason: mixing `///` and `//!` within the same file is intentional for module vs item docs.
#![allow(clippy::suspicious_doc_comments)]
// Reason: exact float comparison is intentional in test assertions for deterministic values.
#![allow(clippy::float_cmp)]
// Reason: test fixtures sometimes require larger arrays that exceed the default stack limit.
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
pub mod design;
pub mod federation;
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
