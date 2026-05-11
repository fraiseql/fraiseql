//! PostgreSQL WHERE clause SQL generation.
//!
//! `PostgresWhereGenerator` is a type alias for
//! `GenericWhereGenerator<PostgresDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{dialect::PostgresDialect, where_generator::GenericWhereGenerator};

/// Cache of indexed columns for views.
///
/// This cache stores column names that follow the FraiseQL indexed column naming
/// conventions:
/// - Human-readable: `items__product__category__code` (double-underscore path)
/// - Entity ID format: `f{entity_id}__{field_name}` (e.g., `f200100__code`)
///
/// When a WHERE clause references a nested path that has a corresponding indexed
/// column, the generator uses the indexed column directly instead of JSONB
/// extraction, enabling the database to use indexes for the query.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::postgres::IndexedColumnsCache;
/// use std::collections::{HashMap, HashSet};
///
/// let mut cache = IndexedColumnsCache::new();
///
/// // Register indexed columns for a view
/// let mut columns = HashSet::new();
/// columns.insert("items__product__category__code".to_string());
/// cache.insert("v_order_items".to_string(), columns);
/// ```
pub type IndexedColumnsCache = HashMap<String, HashSet<String>>;

/// PostgreSQL WHERE clause generator.
///
/// Type alias for `GenericWhereGenerator<PostgresDialect>`.
/// Refer to [`GenericWhereGenerator`] for full documentation.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::postgres::PostgresWhereGenerator;
/// use fraiseql_db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = PostgresWhereGenerator::postgres_new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "data->>'email' ILIKE '%' || $1 || '%'"
/// ```
pub type PostgresWhereGenerator = GenericWhereGenerator<PostgresDialect>;

/// Constructor compatibility shim for `PostgresWhereGenerator`.
///
/// These `impl` blocks expose the same `new()` / `with_indexed_columns()`
/// constructors that the old concrete struct had.
impl PostgresWhereGenerator {
    /// Create a new PostgreSQL WHERE generator.
    #[must_use]
    pub const fn postgres_new() -> Self {
        Self::new(PostgresDialect)
    }

    /// Create a new PostgreSQL WHERE generator with indexed columns for a view.
    ///
    /// When indexed columns are provided, the generator uses them instead of
    /// JSONB extraction for nested paths that have corresponding indexed columns.
    ///
    /// # Arguments
    ///
    /// * `indexed_columns` - Set of indexed column names for the current view
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_db::postgres::PostgresWhereGenerator;
    /// use std::collections::HashSet;
    /// use std::sync::Arc;
    ///
    /// let mut columns = HashSet::new();
    /// columns.insert("items__product__category__code".to_string());
    /// let generator = PostgresWhereGenerator::postgres_with_indexed_columns(Arc::new(columns));
    /// ```
    #[must_use]
    pub fn postgres_with_indexed_columns(indexed_columns: Arc<HashSet<String>>) -> Self {
        Self::new(PostgresDialect).with_indexed_columns(indexed_columns)
    }
}

#[cfg(test)]
mod tests;
