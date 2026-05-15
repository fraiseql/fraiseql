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

/// Context for ID-based ltree operators (`descendantOfId`, `ancestorOfId`).
///
/// Carries hierarchy metadata from the compiled schema into the SQL generation
/// layer. This is infrastructure context (table name, path column, FK column)
/// that belongs at the call site, not embedded in `WhereOperator` enum variants.
///
/// Existing operators carry inline primitive data in their enum variant and
/// dispatch to `SqlDialect` trait methods. Hierarchy config is infrastructure
/// metadata — it belongs at the call site, keeping `WhereOperator` variants clean.
#[derive(Debug, Clone)]
pub struct HierarchyContext {
    /// Database table containing the ltree column (e.g., `"tb_category"`).
    pub table: String,

    /// Name of the ltree column in the table (e.g., `"category_path"`).
    pub path_column: String,

    /// FK column for cross-table hierarchies (e.g., `"fk_location"`).
    /// `None` for self-referencing hierarchies where the filtered entity's own
    /// table contains the ltree column.
    pub fk_column: Option<String>,
}
