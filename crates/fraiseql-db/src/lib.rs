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
#![allow(clippy::doc_markdown)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unused_self)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::format_push_string)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::single_match_else)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::similar_names)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::useless_format)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::unused_async)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::manual_saturating_arithmetic)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::doc_link_with_quotes)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::manual_map)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::implicit_saturating_sub)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::suspicious_doc_comments)]
#![allow(clippy::float_cmp)]
#![allow(clippy::large_stack_arrays)]

// New modules (types extracted from fraiseql-core)
pub mod collation_config;
pub mod filters;
pub mod introspector;
pub mod types;

// DB adapter modules (from the old db/ directory)
pub mod collation;
pub mod identifier;
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
#[cfg(feature = "wire-backend")]
pub use fraiseql_wire_adapter::FraiseWireAdapter;
pub use identifier::{
    quote_mysql_identifier, quote_postgres_identifier, quote_sqlite_identifier,
    quote_sqlserver_identifier,
};
pub use introspector::DatabaseIntrospector;
#[cfg(feature = "mysql")]
pub use mysql::MySqlAdapter;
#[cfg(feature = "postgres")]
pub use postgres::{PostgresAdapter, PostgresIntrospector};
pub use projection_generator::PostgresProjectionGenerator;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteAdapter;
#[cfg(feature = "sqlserver")]
pub use sqlserver::SqlServerAdapter;
pub use traits::{
    CursorValue, DatabaseAdapter, DatabaseCapabilities, MutationCapable, RelayDatabaseAdapter,
    RelayPageResult,
};
pub use types::{
    DatabaseType, JsonbValue, PoolMetrics,
    sql_hints::{OrderByClause, OrderDirection, SqlProjectionHint},
};
pub use where_clause::{HavingClause, WhereClause, WhereOperator};
pub use where_sql_generator::WhereSqlGenerator;
