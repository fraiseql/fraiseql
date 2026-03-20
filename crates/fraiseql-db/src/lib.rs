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
// unnecessary_wraps, must_use_candidate, module_name_repetitions:
// allowed at workspace level (Cargo.toml [workspace.lints.clippy]).

// Reason: SQL identifiers in doc comments (e.g. `SELECT`, `WHERE`) are not Rust
//         items and should not be linked; forcing backtick wrapping on every SQL
//         keyword is noisier than the lint prevents.
#![allow(clippy::doc_markdown)]
// Reason: SQL builder methods return `&mut Self` for method chaining; callers
//         often call the final method for its side-effect and ignore the return
//         value. Forcing `#[must_use]` on every intermediate step adds annotation
//         noise without a safety benefit.
#![allow(clippy::return_self_not_must_use)]
// Reason: Many SQL-builder call sites use positional format strings
//         (`format!("{}", x)`) for clarity when the expression is complex.
//         Migrating every site to `format!("{x}")` is a large low-value churn.
#![allow(clippy::uninlined_format_args)]
// Reason: Several dialect helpers take `&self` for API uniformity (the trait
//         requires it) even when the specific impl does not read any fields.
//         Removing `self` would require a different trait signature.
#![allow(clippy::unused_self)]
// Reason: Multiple `match` arms return identical values in some dialect
//         dispatch blocks because the dialects share a default behaviour;
//         merging them with `|` would lose the explicit per-dialect annotation.
#![allow(clippy::match_same_arms)]
// Reason: DatabaseAdapter implementations take many configuration parameters
//         (view name, WHERE clause, projection, limit, offset, params…).
//         Breaking them into per-call builder structs would add wrapper
//         complexity without clarity gains on the hot path.
#![allow(clippy::too_many_arguments)]
// Reason: SQL string assembly uses `push_str(&format!(…))` in several places
//         where the formatted fragment is non-trivial. Replacing with
//         `write!(sql, …).unwrap()` would require adding `use std::fmt::Write`
//         to many modules. Tracked for a dedicated cleanup pass.
#![allow(clippy::format_push_string)]
// Reason: `.map(|x| x.method())` is retained where the closure makes the
//         intent clearer than a bare method reference (e.g., when `method`
//         is ambiguous without the type). Tracked for a cleanup pass.
#![allow(clippy::redundant_closure_for_method_calls)]
// Reason: `for x in v.iter()` is used for readability in several SQL
//         generation loops. Tracked for a cleanup pass to `for x in &v`.
#![allow(clippy::explicit_iter_loop)]
// Reason: `if condition { 1 } else { 0 }` conversions appear in SQL flag
//         generation where the explicit branch documents the semantics better
//         than a cast.
#![allow(clippy::bool_to_int_with_if)]
// Reason: `match` with a single non-trivial arm and an else block is used
//         in dialect dispatch where the structure makes fallthrough intent
//         explicit; collapsing to `if let` would lose that clarity.
#![allow(clippy::single_match_else)]
// Reason: `use super::*` in test modules (standard Rust test idiom) triggers
//         this lint. Production code does not use wildcard imports. Scoping
//         this allow to each test module is tracked but deferred.
#![allow(clippy::wildcard_imports)]
// Reason: Several configuration structs (e.g., `CollationConfig`,
//         `IntrospectorOptions`) carry multiple boolean flags that represent
//         orthogonal on/off capabilities. Grouping them would obscure the
//         per-flag semantics exposed in `fraiseql.toml`.
#![allow(clippy::struct_excessive_bools)]
// Reason: SQL generation code uses short names (`sql`, `col`, `tbl`, `idx`)
//         that are conventional in database code and in test fixtures. The
//         lint fires most heavily on test helpers where brevity aids readability.
#![allow(clippy::similar_names)]
// Reason: `if let Some(x) = … { … } else { … }` is retained where the else
//         branch has side effects (logging, error construction) that make
//         `.map_or_else` harder to read. Applied conservatively.
#![allow(clippy::option_if_let_else)]
// Reason: `if !condition { A } else { B }` (if-not-else) appears in guard
//         clauses where negated early-returns are the idiomatic Rust pattern.
//         Inverting to `if condition { B } else { A }` would hurt readability.
#![allow(clippy::if_not_else)]
// Reason: `format!("literal")` is used in a few places to produce an owned
//         `String` from a static string; replacing with `.to_owned()` or
//         `String::from()` is mechanical but low priority.
#![allow(clippy::useless_format)]
// Reason: `.unwrap_or(f())` is used in a few places where `f()` is a cheap
//         constructor (e.g., `String::new()`); `.unwrap_or_else(|| f())`
//         would be equivalent but the lint avoidance is marginal here.
#![allow(clippy::or_fun_call)]
// Reason: `async fn` on trait implementation methods must match the trait
//         signature even when the specific impl does not need to await. This
//         is a `#[async_trait]` requirement; all `async_trait` impls are
//         annotated with a tracking comment pointing to RFC 3425.
#![allow(clippy::unused_async)]
// Reason: Some adapter methods (e.g., `default()`, `add()`) share names with
//         standard traits but intentionally do not implement them because the
//         semantics differ (SQL-builder `add` ≠ `std::ops::Add`).
#![allow(clippy::should_implement_trait)]
// Reason: SQL generation functions accept `Vec<T>` or `String` by value
//         because ownership is intentional (callers build and pass the collection).
//         Changing to `&[T]` / `&str` throughout would require cascading
//         lifetime annotations in the trait signatures.
#![allow(clippy::needless_pass_by_value)]
// Reason: Manual saturating arithmetic (`if a > b { b } else { a - b }`) is
//         used in a few places where the saturating-sub equivalent was written
//         before `saturating_sub` was in the codebase. Tracked for cleanup.
#![allow(clippy::manual_saturating_arithmetic)]
// Reason: `match` arms that match a single enum variant and a wildcard `_`
//         appear in dialect dispatch where the wildcard carries an explicit
//         comment explaining the fallthrough intent.
#![allow(clippy::match_wildcard_for_single_variants)]
// Reason: `str::find('c')` vs `str::find("c")` — single-char patterns used
//         in SQL parsing code where the string form is more readable in context.
#![allow(clippy::single_char_pattern)]
// Reason: Doc-comment links wrapped in backtick-quotes (`` [`Foo`] `` vs
//         `[Foo]`) are used inconsistently across the crate. Standardising is
//         a low-priority formatting pass.
#![allow(clippy::doc_link_with_quotes)]
// Reason: Nested `if` blocks in SQL assembly are retained where the inner
//         condition depends on a value computed in the outer block, making
//         collapsing semantically misleading. Tracked for a cleanup pass.
#![allow(clippy::collapsible_if)]
// Reason: `.map(f).unwrap_or(d)` is used in some adapter methods where the
//         map value and default are both cheap; `.map_or(d, f)` is equivalent
//         but less readable in argument-heavy call sites.
#![allow(clippy::map_unwrap_or)]
// Reason: `match opt { Some(x) => Some(f(x)), None => None }` is retained
//         in a few places where the explicit arms document the transformation
//         intent more clearly than `.map(f)`. Tracked for cleanup.
#![allow(clippy::manual_map)]
// Reason: `Default::default()` is used instead of `T::default()` in some
//         struct-initialiser expressions to avoid repeating a long type name.
#![allow(clippy::default_trait_access)]
// Reason: Explicit `if a > b { a - b } else { 0 }` guards are retained over
//         `.saturating_sub()` in a few places where the guard condition also
//         gates unrelated logic; using saturating_sub would not remove the
//         branch anyway.
#![allow(clippy::implicit_saturating_sub)]
// Reason: `&Vec<T>` and `&String` function parameters are used in some
//         adapter trait implementations that were extracted from fraiseql-core
//         and share signatures with internal callers. Migrating to `&[T]` /
//         `&str` requires coordinated changes across crate boundaries.
#![allow(clippy::ptr_arg)]
// Reason: `use DatabaseType::*` glob imports appear in test modules and in
//         a few dialect-dispatch match blocks for readability. Production code
//         outside `#[cfg(test)]` blocks does not use enum glob imports.
#![allow(clippy::enum_glob_use)]
// Reason: `.unwrap_or_default()` is used intentionally in several places
//         where the `Default` implementation is the correct fallback and
//         the `Option` represents a truly optional configuration value.
#![allow(clippy::unwrap_or_default)]
// Reason: `|x| f(x)` closures are retained where the surrounding code
//         uses them for readability in chained iterator expressions.
//         Tracked for a cleanup pass to bare function references.
#![allow(clippy::redundant_closure)]
// Reason: Doc comments on some struct fields start with `//!` at the wrong
//         level; fixing requires reviewing every field comment in the crate.
//         Tracked for a documentation pass.
#![allow(clippy::suspicious_doc_comments)]
// Reason: Float equality comparisons (`==`) appear in test assertions where
//         exact round-trip equality is expected (e.g., serialising a known
//         constant and deserialising it). Production code does not compare floats.
#![allow(clippy::float_cmp)]
// Reason: Test data fixtures include large static arrays (e.g., SQL test
//         vectors, schema samples). These are only compiled in `#[cfg(test)]`
//         blocks and do not affect production stack usage.
#![allow(clippy::large_stack_arrays)]

// New modules (types extracted from fraiseql-core)
pub mod collation_config;
pub mod dialect;
pub mod filters;
pub mod introspector;
pub mod types;
pub mod where_generator;

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
pub use dialect::{
    MySqlDialect, PostgresDialect, SqlDialect, SqlServerDialect, SqliteDialect, UnsupportedOperator,
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
pub use projection_generator::{
    MySqlProjectionGenerator, PostgresProjectionGenerator, SqliteProjectionGenerator,
};
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteAdapter;
#[cfg(feature = "sqlserver")]
pub use sqlserver::SqlServerAdapter;
pub use traits::{
    ArcDatabaseAdapter, BoxDatabaseAdapter, CursorValue, DatabaseAdapter, DatabaseCapabilities,
    DirectMutationContext, DirectMutationOp, MutationCapable, MutationStrategy,
    RelayDatabaseAdapter, RelayPageResult,
};
pub use types::{
    DatabaseType, JsonbValue, PoolMetrics,
    sql_hints::{OrderByClause, OrderDirection, SqlProjectionHint},
};
#[cfg(feature = "grpc")]
pub use types::{ColumnSpec, ColumnValue};
pub use dialect::{
    MySqlDialect, PostgresDialect, SqlDialect, SqlServerDialect, SqliteDialect,
    UnsupportedOperator,
};
pub use where_clause::{HavingClause, WhereClause, WhereOperator};
pub use where_generator::GenericWhereGenerator;
pub use where_sql_generator::WhereSqlGenerator;
