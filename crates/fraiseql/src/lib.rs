//! # FraiseQL v2 - Compiled GraphQL Execution Engine
//!
//! FraiseQL compiles GraphQL schemas into optimized SQL at build time,
//! eliminating runtime overhead for deterministic, high-performance query execution.
//!
//! ## Quick Start
//!
//! ```text
//! use fraiseql::prelude::*;
//! use std::sync::Arc;
//!
//! // 1. Load compiled schema
//! let schema = CompiledSchema::from_file("schema.compiled.json")?;
//!
//! // 2. Create database adapter
//! let adapter = Arc::new(
//!     fraiseql::db::PostgresAdapter::new("postgresql://localhost/mydb").await?
//! );
//!
//! // 3. Create and run server (requires 'server' feature)
//! #[cfg(feature = "server")]
//! {
//!     use fraiseql::server::{Server, ServerConfig};
//!     let config = ServerConfig::from_file("fraiseql.toml")?;
//!     let server = Server::new(config, schema, adapter, None).await?;
//!     server.serve().await?;
//! }
//! ```
//!
//! ## Architecture
//!
//! ```text
//! Authoring               Compilation              Runtime
//! (Python/TS)            (Rust)                   (Rust)
//!     ↓                      ↓                        ↓
//! schema.json    +    fraiseql.toml      →    schema.compiled.json    →    GraphQL Server
//! (types)                 (config)           (types + config + SQL)        (Axum HTTP server)
//! ```
//!
//! ## Feature Flags
//!
//! - **Database backends:**
//!   - `postgres` (default) - PostgreSQL support
//!   - `mysql` - MySQL support
//!   - `sqlite` - SQLite support
//!   - `sqlserver` - SQL Server support
//!
//! - **Optional components:**
//!   - `server` - HTTP server with auth, webhooks, file uploads
//!   - `observers` - Reactive business logic system
//!   - `arrow` - Apache Arrow Flight integration
//!   - `wire` - Streaming JSON query engine
//!   - `cli` - Compiler CLI tools
//!
//! - **Bundles:**
//!   - `full` - All features enabled
//!   - `minimal` - Core only, no database backends

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Re-export core types
#[cfg(feature = "arrow")]
pub use fraiseql_arrow as arrow;
#[cfg(feature = "cli")]
pub use fraiseql_cli as cli;
pub use fraiseql_core::{CompiledSchema, FraiseQLConfig, FraiseQLError, Result, TenantContext};
// Re-export core modules for namespaced access
pub use fraiseql_core::{
    apq, cache, compiler, db, federation, graphql, runtime, schema, security, tenancy, validation,
};
// Re-export error types
pub use fraiseql_error::{AuthError, ConfigError, FileError, RuntimeError, WebhookError};
#[cfg(feature = "observers")]
pub use fraiseql_observers as observers;
// Conditional re-exports (feature-gated)
#[cfg(feature = "server")]
pub use fraiseql_server as server;
#[cfg(feature = "wire")]
pub use fraiseql_wire as wire;

// Prelude module for convenient imports
pub mod prelude {
    //! Prelude module for convenient imports.
    //!
    //! Import with: `use fraiseql::prelude::*;`

    // Core types
    // Database access
    // GraphQL parsing
    // Runtime executor
    pub use fraiseql_core::runtime::Executor;
    // Tenancy support
    pub use fraiseql_core::tenancy::TenantContext as Tenant;
    pub use fraiseql_core::{
        db,
        graphql::{ParsedQuery, parse_query},
        runtime::ExecutorAdapter,
    };

    // Optional: Observer system
    #[cfg(feature = "observers")]
    pub use crate::observers::{EntityEvent, EventKind, ObserverExecutor};
    // Optional: Server components
    #[cfg(feature = "server")]
    pub use crate::server::{Server, ServerConfig};
    pub use crate::{CompiledSchema, FraiseQLConfig, FraiseQLError, Result, TenantContext};
}

/// FraiseQL version string
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
