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
//! - **Compiler**: Parsing, validation, and runtime sub-modules (aggregation, fact tables, window
//!   functions)
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
//! let schema = CompiledSchema::from_json(schema_json, false)?;
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

/// The `chrono` this crate's public API is built against (#1198).
pub use chrono;
/// The `deadpool_postgres` this crate's public API is built against (#1198).
pub use deadpool_postgres;
/// Core's own handle on the database layer.
///
/// Private on purpose. The *public* `db` module below is a curated subset, and core
/// must not be forced to widen that subset merely to reach something for itself —
/// which is what a single shared re-export would do, and is how the blanket export
/// came to expose the whole data plane.
pub(crate) use fraiseql_db as backend;

/// The database layer this crate's public API is built against.
///
/// **Enumerated, not a blanket `pub use fraiseql_db as db;`** (S3 of the boundary
/// work). The blanket form made every item in `fraiseql-db` nameable from any crate
/// that depends on `fraiseql-core` — including the data-plane surface the write
/// chokepoint sits above. Narrowing it is compiler-verified: an item that is not
/// listed is a compile error at the point of use, not a lint that has to be written
/// and then kept in step.
///
/// Adding a line here widens the surface that transports can reach around the
/// engine with. Add one only for a type a transport genuinely needs to *name* —
/// configuration, a trait bound, a value in a signature — never to reach a method.
pub mod db {
    pub use fraiseql_db::{
        // The admin-SQL route's own request/response types (an explicit admin API,
        // not a request-path surface).
        AdminSqlOutcome,
        AdminSqlRequest,
        dialect::{self, PostgresDialect},
        // The runtime's own identifier quoting, so the boot-time `sql_source` probe
        // (#487) and the runtime cannot drift on how a name is rendered.
        identifier::quote_postgres_identifier,
        // Introspection, as the CLI's schema tooling consumes it.
        introspector::{self, RelationInfo, RelationKind},
        // JSONB path escaping and projection SQL, which the CLI's generators and the
        // projection parity tests build against.
        path_escape,
        projection_generator::{self, PostgresProjectionGenerator},
        // The traits a transport bounds on, and the types that appear in those bounds.
        traits::{
            self, ArcDatabaseAdapter, CursorValue, DatabaseAdapter, RelayDatabaseAdapter,
            SupportsMutations,
        },
        types::{
            self, DatabaseType, JsonbValue, PoolMetrics, QueryStatEntry,
            sql_hints::{OrderByClause, RelevanceOrder, ScalarFieldType},
        },
        // One naming helper, not the whole `utils` module: a pure string function the
        // REST search handler needs, with no reach into the data plane.
        utils::to_snake_case,
        // WHERE construction, which the server builds for REST and gRPC filters.
        where_clause::{self, WhereClause, WhereOperator},
        where_generator,
        where_sql_generator::{self, WhereSqlGenerator},
    };
    /// The wire backend, behind the same feature that gates it in `fraiseql-db`.
    #[cfg(feature = "wire-backend")]
    pub use fraiseql_db::{FraiseWireAdapter, wire_pool};
    /// The PostgreSQL backend, behind the same feature that gates it in `fraiseql-db`.
    ///
    /// `PostgresAdapter` belongs here, not in the unconditional list above. The
    /// blanket `pub use fraiseql_db as db;` carried each item's own `#[cfg]` for
    /// free; enumerating the names drops that, so every line above is an assertion
    /// that the item is ungated in `fraiseql-db` — and nothing but a feature-OFF
    /// build checks it. `make lint-feature-matrix` is that build.
    #[cfg(feature = "postgres")]
    pub use fraiseql_db::{PostgresAdapter, PostgresIntrospector, postgres};
}

/// The `graphql_parser` this crate's public API is built against (#1198).
pub use graphql_parser;
/// The `indexmap` this crate's public API is built against (#1198).
pub use indexmap;
/// The `jsonwebtoken` this crate's public API is built against (#1198).
pub use jsonwebtoken;
/// The `regex` this crate's public API is built against (#1198).
pub use regex;
/// The `reqwest` this crate's public API is built against (#1198).
pub use reqwest;
/// The `serde_json` this crate's public API is built against (#1198).
pub use serde_json;
/// The `tokio_postgres` this crate's public API is built against (#1198).
pub use tokio_postgres;
/// The `tracing` this crate's public API is built against (#1198).
pub use tracing;
#[cfg(feature = "schema-lint")]
pub mod design;
#[cfg(feature = "federation")]
pub use fraiseql_federation as federation;
pub mod security;
pub mod tenancy;
pub mod types;
pub mod utils;
pub mod validation;

pub mod prelude;

// Re-exports for convenience
pub use error::{FraiseQLError, Result};
pub use schema::CompiledSchema;
pub use tenancy::TenantContext;
/// The `uuid` this crate's public API is built against (#1198).
pub use uuid;
/// The `zeroize` this crate's public API is built against (#1198).
pub use zeroize;

/// Version of the FraiseQL core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version
pub const MSRV: &str = "1.88";
