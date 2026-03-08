//! Generic WHERE clause generator.
//!
//! The central export is [`GenericWhereGenerator`], which is parameterised
//! over a [`SqlDialect`] and handles all shared operator logic.
//!
//! The per-dialect type aliases live in each dialect's own `where_generator.rs`
//! for backward compatibility:
//!
//! | Type alias | Dialect |
//! |---|---|
//! | `PostgresWhereGenerator` | [`PostgresDialect`] |
//! | `MySqlWhereGenerator` | [`MySqlDialect`] |
//! | `SqliteWhereGenerator` | [`SqliteDialect`] |
//! | `SqlServerWhereGenerator` | [`SqlServerDialect`] |
//!
//! [`SqlDialect`]: crate::dialect::SqlDialect
//! [`PostgresDialect`]: crate::dialect::PostgresDialect
//! [`MySqlDialect`]: crate::dialect::MySqlDialect
//! [`SqliteDialect`]: crate::dialect::SqliteDialect
//! [`SqlServerDialect`]: crate::dialect::SqlServerDialect

pub(super) mod counter;
pub mod generic;

pub use generic::GenericWhereGenerator;
