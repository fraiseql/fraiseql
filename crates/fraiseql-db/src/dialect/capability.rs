//! Dialect capability matrix and fail-fast guard.
//!
//! [`DialectCapabilityGuard`] is called at query-planning time to verify that
//! the requested feature is supported by the connected database dialect. If not,
//! it returns `FraiseQLError::Unsupported` with a human-readable message and an
//! optional migration suggestion — before SQL generation begins.
//!
//! This prevents cryptic driver errors ("syntax error near 'RETURNING'") and
//! replaces them with actionable developer guidance.
//!
//! # Usage
//!
//! ```ignore
//! DialectCapabilityGuard::check(DatabaseType::SQLite, Feature::Mutations)?;
//! // → Err(FraiseQLError::Unsupported { message: "Mutations (INSERT/UPDATE/DELETE
//! //     via mutation_response) are not supported on SQLite. Use PostgreSQL or
//! //     MySQL for mutation support." })
//! ```

use fraiseql_error::FraiseQLError;

use super::SqlDialect;
use crate::types::{DatabaseType, sql_hints::ScalarFieldType};

// ============================================================================
// Feature enum
// ============================================================================

/// A database feature that may not be supported on all dialects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Feature {
    /// JSONB path expressions (`metadata->>'key'`, `@>`, `?`, etc.)
    JsonbPathOps,
    /// GraphQL subscriptions (real-time push over WebSocket/SSE)
    Subscriptions,
    /// Mutations (INSERT/UPDATE/DELETE via `mutation_response`)
    Mutations,
    /// Window functions (`RANK()`, `ROW_NUMBER()`, `LAG()`, etc.)
    WindowFunctions,
    /// Common Table Expressions (`WITH` clause)
    CommonTableExpressions,
    /// Full-text search (`MATCH`, `@@`, `CONTAINS`)
    FullTextSearch,
    /// Advisory locks (`pg_advisory_lock`, `GET_LOCK`)
    AdvisoryLocks,
    /// Standard deviation / variance aggregates (`STDDEV`, `VARIANCE`)
    StddevVariance,
    /// Upsert (`ON CONFLICT DO UPDATE`, `INSERT ... ON DUPLICATE KEY UPDATE`, `MERGE`)
    Upsert,
    /// Array column types (`text[]`, `integer[]`)
    ArrayTypes,
    /// Backward keyset pagination (requires stable sort with reversed direction)
    BackwardPagination,
}

impl Feature {
    /// Human-readable display name for error messages.
    const fn display_name(self) -> &'static str {
        match self {
            Self::JsonbPathOps => "JSONB path expressions",
            Self::Subscriptions => "Subscriptions (real-time push)",
            Self::Mutations => "Mutations (INSERT/UPDATE/DELETE via mutation_response)",
            Self::WindowFunctions => "Window functions (RANK, ROW_NUMBER, LAG, etc.)",
            Self::CommonTableExpressions => "Common Table Expressions (WITH clause)",
            Self::FullTextSearch => "Full-text search",
            Self::AdvisoryLocks => "Advisory locks",
            Self::StddevVariance => "STDDEV/VARIANCE aggregates",
            Self::Upsert => "Upsert (ON CONFLICT / INSERT OR REPLACE)",
            Self::ArrayTypes => "Array column types",
            Self::BackwardPagination => "Backward keyset pagination",
        }
    }
}

// ============================================================================
// Capability matrix
// ============================================================================

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
    /// assert_eq!(DatabaseType::MySQL.json_field_expr("name"), "JSON_UNQUOTE(JSON_EXTRACT(data, '$.name'))");
    /// ```
    #[must_use]
    pub fn json_field_expr(self, key: &str) -> String {
        match self {
            Self::PostgreSQL => format!("data->>'{key}'"),
            Self::MySQL => format!("JSON_UNQUOTE(JSON_EXTRACT(data, '$.{key}'))"),
            Self::SQLite => format!("json_extract(data, '$.{key}')"),
            Self::SQLServer => format!("JSON_VALUE(data, '$.{key}')"),
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
            Self::MySQL => &super::MySqlDialect,
            Self::SQLite => &super::SqliteDialect,
            Self::SQLServer => &super::SqlServerDialect,
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
    /// assert_eq!(
    ///     DatabaseType::MySQL.typed_json_field_expr("amount", ScalarFieldType::Numeric),
    ///     "CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.amount')) AS DECIMAL(38,12))"
    /// );
    /// ```
    #[must_use]
    pub fn typed_json_field_expr(self, key: &str, field_type: ScalarFieldType) -> String {
        let base = self.json_field_expr(key);
        self.dialect().cast_expr_as(&base, field_type).into_owned()
    }

    /// Check whether this dialect supports `feature`.
    ///
    /// All checks are `const`-friendly and zero-cost at runtime.
    #[must_use]
    pub const fn supports(self, feature: Feature) -> bool {
        match (self, feature) {
            // PostgreSQL: fully featured
            (Self::PostgreSQL, _) => true,

            // MySQL 8+: no JSONB path ops, subscriptions, advisory locks,
            // STDDEV, array types. Everything else is supported.
            (
                Self::MySQL,
                Feature::JsonbPathOps
                | Feature::Subscriptions
                | Feature::AdvisoryLocks
                | Feature::StddevVariance
                | Feature::ArrayTypes,
            ) => false,
            (Self::MySQL, _) => true,

            // SQL Server: no JSONB path ops, subscriptions, advisory locks,
            // array types. Everything else is supported.
            (
                Self::SQLServer,
                Feature::JsonbPathOps
                | Feature::Subscriptions
                | Feature::AdvisoryLocks
                | Feature::ArrayTypes,
            ) => false,
            (Self::SQLServer, _) => true,

            // SQLite: very limited — only CTEs and Upsert are supported
            (Self::SQLite, Feature::CommonTableExpressions | Feature::Upsert) => true,
            (Self::SQLite, _) => false,
        }
    }

    /// Return a human-readable migration suggestion for an unsupported feature.
    ///
    /// `None` means no specific guidance is available beyond the error message.
    #[must_use]
    pub const fn suggestion_for(self, feature: Feature) -> Option<&'static str> {
        match (self, feature) {
            (Self::MySQL, Feature::JsonbPathOps) => {
                Some("Use `json_extract(column, '$.key')` syntax instead of JSONB path operators.")
            },
            (Self::MySQL, Feature::StddevVariance) => {
                Some("MySQL does not provide STDDEV/VARIANCE; compute them in application code.")
            },
            (Self::SQLite, Feature::Mutations) => Some(
                "SQLite mutations are not supported. Use PostgreSQL or MySQL for mutation support.",
            ),
            (Self::SQLite, Feature::WindowFunctions) => Some(
                "SQLite 3.25+ supports basic window functions; upgrade your SQLite version or use PostgreSQL.",
            ),
            (Self::SQLite, Feature::Subscriptions) => {
                Some("Subscriptions require a database with LISTEN/NOTIFY. Use PostgreSQL.")
            },
            _ => None,
        }
    }
}

// ============================================================================
// Guard
// ============================================================================

/// Fail-fast guard that checks database dialect capabilities before SQL generation.
///
/// Call [`DialectCapabilityGuard::check`] during query planning to produce
/// a `FraiseQLError::Unsupported` with actionable guidance instead of a
/// cryptic driver error.
pub struct DialectCapabilityGuard;

impl DialectCapabilityGuard {
    /// Check that `dialect` supports `feature`.
    ///
    /// Returns `Ok(())` if the feature is supported, or
    /// `Err(FraiseQLError::Unsupported)` with a human-readable message.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Unsupported`] when the feature is not available
    /// on the specified dialect.
    pub fn check(dialect: DatabaseType, feature: Feature) -> Result<(), FraiseQLError> {
        if dialect.supports(feature) {
            return Ok(());
        }

        let suggestion =
            dialect.suggestion_for(feature).map(|s| format!(" {s}")).unwrap_or_default();

        Err(FraiseQLError::Unsupported {
            message: format!(
                "{} is not supported on {}.{suggestion} \
                 See docs/database-compatibility.md for the full feature matrix.",
                feature.display_name(),
                dialect.as_str(),
            ),
        })
    }

    /// Check multiple features at once and return **all** unsupported ones.
    ///
    /// Unlike [`check`], this collects all failures before returning, giving
    /// the developer a complete picture in a single error message.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Unsupported`] listing all unsupported features
    /// if any are unsupported.
    ///
    /// [`check`]: Self::check
    pub fn check_all(dialect: DatabaseType, features: &[Feature]) -> Result<(), FraiseQLError> {
        let failures: Vec<String> = features
            .iter()
            .copied()
            .filter(|&f| !dialect.supports(f))
            .map(|f| {
                let suggestion =
                    dialect.suggestion_for(f).map(|s| format!(" {s}")).unwrap_or_default();
                format!("- {}{suggestion}", f.display_name())
            })
            .collect();

        if failures.is_empty() {
            return Ok(());
        }

        Err(FraiseQLError::Unsupported {
            message: format!(
                "The following features are not supported on {}:\n{}\n\
                 See docs/database-compatibility.md for the full feature matrix.",
                dialect.as_str(),
                failures.join("\n"),
            ),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
