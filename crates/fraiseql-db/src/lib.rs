//! # fraiseql-db
//!
//! Database abstraction layer for FraiseQL v2.
//!
//! This crate provides database-agnostic access to multiple database backends:
//! - PostgreSQL (primary, full feature set)
//! - MySQL (secondary support)
//! - SQLite (local dev, testing)
//! - SQL Server (enterprise)
//!
//! It also provides the shared DB-level types used by the compilation and
//! execution layers: collation configuration, SQL hint types, and extended
//! filter operators.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Reason: SQL identifiers in doc comments (e.g. `SELECT`, `WHERE`) are not Rust
//         items and should not be linked; forcing backtick wrapping on every SQL
//         keyword is noisier than the lint prevents.
#![allow(clippy::doc_markdown)]
// Reason: Multiple `match` arms return identical values in dialect dispatch
//         blocks because dialects share a default behaviour; merging with `|`
//         would lose the explicit per-dialect annotation.
#![allow(clippy::match_same_arms)]
// Reason: `DatabaseAdapter` implementations take many configuration parameters
//         (view name, WHERE clause, projection, limit, offset, params…).
//         Breaking them into per-call builder structs would add wrapper
//         complexity without clarity gains on the hot path.
#![allow(clippy::too_many_arguments)]
// Reason: `match` arms that match a single enum variant and a wildcard `_`
//         appear in dialect dispatch where the wildcard carries an explicit
//         comment explaining the fallthrough intent (forward-compat guard on
//         `DatabaseType`).
#![allow(clippy::match_wildcard_for_single_variants)]

/// A type alias for `Result<T, fraiseql_error::FraiseQLError>`, used throughout this crate.
pub type Result<T> = std::result::Result<T, fraiseql_error::FraiseQLError>;

// New modules (types extracted from fraiseql-core)
pub mod collation_config;
pub mod dialect;
pub mod filters;
pub mod introspector;
pub mod types;
pub mod where_generator;

// Shared utilities
pub(crate) mod utils;

// DB adapter modules (from the old db/ directory)
pub mod collation;
pub mod identifier;
pub mod order_by;
pub mod path_escape;
pub mod projection_generator;
pub mod traits;
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
pub use collation_config::{
    CollationConfig, DatabaseCollationOverrides, InvalidLocaleStrategy, MySqlCollationConfig,
    PostgresCollationConfig, SqlServerCollationConfig, SqliteCollationConfig,
};
pub use dialect::{
    DialectCapabilityGuard, Feature, MySqlDialect, PostgresDialect, SqlDialect, SqlServerDialect,
    SqliteDialect, UnsupportedOperator,
};
#[cfg(feature = "wire-backend")]
pub use fraiseql_wire_adapter::FraiseWireAdapter;
pub use identifier::{
    quote_mysql_identifier, quote_postgres_identifier, quote_sqlite_identifier,
    quote_sqlserver_identifier,
};
pub use introspector::{DatabaseIntrospector, RelationInfo, RelationKind};
#[cfg(feature = "mysql")]
pub use mysql::{MySqlAdapter, MySqlIntrospector};
#[cfg(feature = "postgres")]
pub use postgres::{PostgresAdapter, PostgresIntrospector};
pub use projection_generator::{
    FieldKind, MySqlProjectionGenerator, PostgresProjectionGenerator, ProjectionField,
    SqliteProjectionGenerator,
};
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteAdapter, SqliteIntrospector};
#[cfg(feature = "sqlserver")]
pub use sqlserver::{SqlServerAdapter, SqlServerIntrospector};
pub use traits::{
    ArcDatabaseAdapter, BoxDatabaseAdapter, CursorValue, DatabaseAdapter, DatabaseCapabilities,
    DirectMutationContext, DirectMutationOp, MutationStrategy, RelayDatabaseAdapter,
    RelayPageResult, SupportsMutations,
};
pub use types::{
    DatabaseType, JsonbValue, PoolMetrics,
    sql_hints::{OrderByClause, OrderByFieldType, OrderDirection, SqlProjectionHint},
};
pub use where_clause::{HavingClause, WhereClause, WhereOperator};
pub use where_generator::GenericWhereGenerator;
pub use where_sql_generator::WhereSqlGenerator;
