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

#![forbid(unsafe_code)]
// Missing docs allowed for internal items - public API is fully documented
#![allow(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Allow common pedantic lints that are too noisy for this codebase
#![allow(clippy::doc_markdown)] // Would require 150+ doc changes for backticks
#![allow(clippy::return_self_not_must_use)] // Builder pattern doesn't always need #[must_use]
#![allow(clippy::uninlined_format_args)] // Style preference, not a bug
#![allow(clippy::unused_self)] // Often needed for trait consistency
#![allow(clippy::unnecessary_wraps)] // Sometimes needed for API consistency
#![allow(clippy::must_use_candidate)] // Too noisy for builder methods
#![allow(clippy::missing_errors_doc)] // Would require extensive doc additions
#![allow(clippy::module_name_repetitions)] // Common in Rust APIs
#![allow(clippy::match_same_arms)] // Sometimes clearer to be explicit
#![allow(clippy::cast_possible_truncation)] // Many intentional u64->u32 casts
#![allow(clippy::cast_precision_loss)] // Intentional f64 conversions
#![allow(clippy::cast_sign_loss)] // Intentional signed->unsigned conversions
#![allow(clippy::too_many_arguments)] // Some complex functions need many args
#![allow(clippy::format_push_string)] // Sometimes clearer than write!
#![allow(clippy::redundant_closure_for_method_calls)] // Sometimes clearer
#![allow(clippy::explicit_iter_loop)] // Explicit .iter() can be clearer
#![allow(clippy::bool_to_int_with_if)] // Sometimes clearer than conversion
#![allow(clippy::single_match_else)] // Sometimes clearer than if-else
#![allow(clippy::wildcard_imports)] // Used intentionally for enum variants
#![allow(clippy::struct_excessive_bools)] // AutoParams struct uses bools for flags
#![allow(clippy::missing_panics_doc)] // Would require extensive doc additions
#![allow(clippy::similar_names)] // Variable naming style
#![allow(clippy::option_if_let_else)] // Sometimes clearer
#![allow(clippy::if_not_else)] // Sometimes clearer
#![allow(clippy::useless_format)] // Sometimes needed for consistency
#![allow(clippy::or_fun_call)] // Sometimes clearer with function call
#![allow(clippy::unused_async)] // Placeholder for future async work
#![allow(clippy::should_implement_trait)] // from_str intentionally different
#![allow(clippy::needless_pass_by_value)] // Sometimes clearer API
#![allow(clippy::manual_saturating_arithmetic)] // Explicit can be clearer
#![allow(clippy::match_wildcard_for_single_variants)] // Sometimes clearer
#![allow(clippy::single_char_pattern)] // Very minor optimization
#![allow(clippy::doc_link_with_quotes)] // Documentation style choice
#![allow(clippy::collapsible_if)] // Sometimes clearer when separate
#![allow(clippy::map_unwrap_or)] // Sometimes clearer
#![allow(clippy::manual_map)] // Sometimes clearer
#![allow(clippy::default_trait_access)] // Map::default() vs Default::default()
#![allow(clippy::implicit_saturating_sub)] // Explicit subtraction can be clearer
#![allow(clippy::ptr_arg)] // Sometimes &Vec is clearer than &[T]
#![allow(clippy::enum_glob_use)] // Wildcard enum imports for readability
#![allow(clippy::unwrap_or_default)] // or_insert_with(Vec::new) style preference
#![allow(clippy::redundant_closure)] // Sometimes clearer
#![allow(clippy::suspicious_doc_comments)] // /// vs //! style is intentional
#![allow(clippy::float_cmp)] // Test assertions with exact float comparison are intentional

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
pub mod db;
pub mod design;
pub mod federation;
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
