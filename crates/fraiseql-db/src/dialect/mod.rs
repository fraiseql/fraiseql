//! SQL dialect abstractions for WHERE clause generation.
//!
//! This module defines the [`SqlDialect`] trait and provides four dialect
//! implementations: [`PostgresDialect`], [`MySqlDialect`], [`SqliteDialect`],
//! and [`SqlServerDialect`].
//!
//! The generic [`GenericWhereGenerator`] is parameterised over any type that
//! implements `SqlDialect`, so dialect-specific primitives (identifier quoting,
//! JSON extraction, placeholder syntax, LIKE/ILIKE, …) can be swapped without
//! touching the visitor logic.
//!
//! [`GenericWhereGenerator`]: crate::where_generator::GenericWhereGenerator

// The trait lives in a sub-module so the `pub use` below re-exports it cleanly
// without polluting this module's item namespace with the internal `trait_def` name.
pub mod capability;
pub mod trait_def;

mod mysql;
mod postgres;
mod sqlite;
mod sqlserver;

pub use capability::{DialectCapabilityGuard, Feature};
pub use mysql::MySqlDialect;
pub use postgres::PostgresDialect;
pub use sqlite::SqliteDialect;
pub use sqlserver::SqlServerDialect;
pub use trait_def::{SqlDialect, UnsupportedOperator};
