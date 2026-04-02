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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use std::{collections::HashSet, sync::Arc};

    use serde_json::json;

    use super::*;
    use crate::where_clause::{WhereClause, WhereOperator};

    #[test]
    fn test_simple_equality() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' = $1");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value: json!("alice"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'name' ILIKE '%' || $1 || '%'");
        assert_eq!(params, vec![json!("alice")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value: json!("active"),
            },
            WhereClause::Field {
                path: vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value: json!(18),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("AND"), "Expected AND: {sql}");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_indexed_columns() {
        let mut cols = HashSet::new();
        cols.insert("items__product__category__code".to_string());
        let gen = PostgresWhereGenerator::new(PostgresDialect).with_indexed_columns(Arc::new(cols));

        let clause = WhereClause::Field {
            path: vec![
                "items".to_string(),
                "product".to_string(),
                "category".to_string(),
                "code".to_string(),
            ],
            operator: WhereOperator::Eq,
            value: json!("BOOK"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(
            sql.contains("\"items__product__category__code\""),
            "Expected indexed col, got: {sql}"
        );
        assert_eq!(params, vec![json!("BOOK")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value: json!("Paris"),
        };

        let (sql, _) = gen.generate(&clause).unwrap();
        // Nested path: data->'address'->>'city'
        assert!(sql.contains("data->"), "Expected JSONB path: {sql}");
        assert!(sql.contains("address"), "Expected 'address' segment: {sql}");
        assert!(sql.contains("city"), "Expected 'city' segment: {sql}");
    }

    #[test]
    fn test_is_null() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(true),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'deleted_at' IS NULL");
        assert!(params.is_empty());
    }

    #[test]
    fn test_param_offset() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("a@b.com"),
        };

        let (sql, _) = gen.generate_with_param_offset(&clause, 2).unwrap();
        assert!(sql.contains("$3"), "Expected $3, got: {sql}");
    }

    #[test]
    fn test_in_operator() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::In,
            value: json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'status' IN ($1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_vector_cosine_distance() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["embedding".to_string()],
            operator: WhereOperator::CosineDistance,
            value: json!([0.1, 0.2, 0.3]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("<=>"), "Expected <=>: {sql}");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_ltree_ancestor_of() {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec!["category_path".to_string()],
            operator: WhereOperator::AncestorOf,
            value: json!("europe.france"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("@>"), "Expected @>: {sql}");
        assert!(sql.contains("ltree"), "Expected ::ltree: {sql}");
        assert_eq!(params.len(), 1);
    }
}
