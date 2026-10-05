//! PostgreSQL WHERE clause SQL generation.
//!
//! `PostgresWhereGenerator` is a type alias for
//! `GenericWhereGenerator<PostgresDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{dialect::PostgresDialect, where_generator::GenericWhereGenerator};

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
/// This `impl` block exposes the `new()` constructor the old concrete struct had.
impl PostgresWhereGenerator {
    /// Create a new PostgreSQL WHERE generator.
    #[must_use]
    pub const fn postgres_new() -> Self {
        Self::new(PostgresDialect)
    }
}

#[cfg(test)]
mod tests;
