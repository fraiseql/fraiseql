//! JSON-extraction expressions keyed on [`DatabaseType`].
//!
//! Historical note: this module used to hold a `DialectCapabilityGuard` and a
//! per-dialect feature matrix. Three audit passes found the guard was never
//! called from any production path, and the G2 decision (#374) removed the
//! non-PostgreSQL dialects it existed to arbitrate — so it was deleted rather
//! than wired. What remains is the single source of truth for "how does a
//! `DatabaseType` extract a value from the `data` JSONB column".

use super::SqlDialect;
use crate::types::{DatabaseType, sql_hints::ScalarFieldType};

impl DatabaseType {
    /// Return a SQL expression that extracts a text value from the `data` JSONB column.
    ///
    /// The `key` must already be validated via `OrderByClause::validate_field_name`
    /// and converted to snake_case storage form via `OrderByClause::storage_key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_db::DatabaseType;
    ///
    /// assert_eq!(DatabaseType::PostgreSQL.json_field_expr("created_at"), "data->>'created_at'");
    /// ```
    #[must_use]
    pub fn json_field_expr(self, key: &str) -> String {
        match self {
            Self::PostgreSQL => format!("data->>'{key}'"),
        }
    }

    /// Return the [`SqlDialect`] that renders SQL for this database.
    ///
    /// The two dialect abstractions — this enum, used by the ORDER BY renderer,
    /// and the [`SqlDialect`] trait, used by the WHERE generator — resolve to
    /// the same implementation here, so neither can carry its own copy of a
    /// rendering rule.
    #[must_use]
    pub fn dialect(self) -> &'static dyn SqlDialect {
        match self {
            Self::PostgreSQL => &super::PostgresDialect,
        }
    }

    /// Return a SQL expression that extracts and casts a value from the `data` JSONB
    /// column for ORDER BY sorting.
    ///
    /// When `field_type` is [`ScalarFieldType::Text`] this is identical to
    /// [`json_field_expr`](Self::json_field_expr). For numeric, date, and boolean
    /// types the expression is wrapped in a dialect-specific cast so the database
    /// sorts by the typed value instead of the raw text (`"9" > "10"` is wrong for
    /// numbers).
    ///
    /// The cast itself comes from [`SqlDialect::cast_expr_as`] — the same call the
    /// WHERE generator makes — so an ORDER BY and a filter on one field agree.
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_db::{DatabaseType, ScalarFieldType};
    ///
    /// assert_eq!(
    ///     DatabaseType::PostgreSQL.typed_json_field_expr("amount", ScalarFieldType::Numeric),
    ///     "(data->>'amount')::numeric"
    /// );
    /// ```
    #[must_use]
    pub fn typed_json_field_expr(self, key: &str, field_type: ScalarFieldType) -> String {
        let base = self.json_field_expr(key);
        self.dialect().cast_expr_as(&base, field_type).into_owned()
    }
}

#[cfg(test)]
mod tests;
