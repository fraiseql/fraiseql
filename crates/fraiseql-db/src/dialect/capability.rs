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

use crate::types::DatabaseType;

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
            }
            (Self::MySQL, Feature::StddevVariance) => {
                Some("MySQL does not provide STDDEV/VARIANCE; compute them in application code.")
            }
            (Self::SQLite, Feature::Mutations) => {
                Some("SQLite mutations are not supported. Use PostgreSQL or MySQL for mutation support.")
            }
            (Self::SQLite, Feature::WindowFunctions) => {
                Some("SQLite 3.25+ supports basic window functions; upgrade your SQLite version or use PostgreSQL.")
            }
            (Self::SQLite, Feature::Subscriptions) => {
                Some("Subscriptions require a database with LISTEN/NOTIFY. Use PostgreSQL.")
            }
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

        let suggestion = dialect
            .suggestion_for(feature)
            .map(|s| format!(" {s}"))
            .unwrap_or_default();

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
    pub fn check_all(
        dialect: DatabaseType,
        features: &[Feature],
    ) -> Result<(), FraiseQLError> {
        let failures: Vec<String> = features
            .iter()
            .copied()
            .filter(|&f| !dialect.supports(f))
            .map(|f| {
                let suggestion = dialect
                    .suggestion_for(f)
                    .map(|s| format!(" {s}"))
                    .unwrap_or_default();
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
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    // --- DatabaseType::supports ---

    #[test]
    fn test_postgres_supports_all_features() {
        for feature in all_features() {
            assert!(
                DatabaseType::PostgreSQL.supports(feature),
                "PostgreSQL should support {feature:?}"
            );
        }
    }

    #[test]
    fn test_mysql_does_not_support_jsonb() {
        assert!(!DatabaseType::MySQL.supports(Feature::JsonbPathOps));
    }

    #[test]
    fn test_mysql_supports_mutations() {
        assert!(DatabaseType::MySQL.supports(Feature::Mutations));
    }

    #[test]
    fn test_mysql_supports_window_functions() {
        assert!(DatabaseType::MySQL.supports(Feature::WindowFunctions));
    }

    #[test]
    fn test_mysql_does_not_support_stddev() {
        assert!(!DatabaseType::MySQL.supports(Feature::StddevVariance));
    }

    #[test]
    fn test_sqlite_supports_cte() {
        assert!(DatabaseType::SQLite.supports(Feature::CommonTableExpressions));
    }

    #[test]
    fn test_sqlite_does_not_support_mutations() {
        assert!(!DatabaseType::SQLite.supports(Feature::Mutations));
    }

    #[test]
    fn test_sqlite_does_not_support_subscriptions() {
        assert!(!DatabaseType::SQLite.supports(Feature::Subscriptions));
    }

    #[test]
    fn test_sqlite_does_not_support_window_functions() {
        assert!(!DatabaseType::SQLite.supports(Feature::WindowFunctions));
    }

    #[test]
    fn test_sqlserver_does_not_support_jsonb() {
        assert!(!DatabaseType::SQLServer.supports(Feature::JsonbPathOps));
    }

    #[test]
    fn test_sqlserver_supports_mutations() {
        assert!(DatabaseType::SQLServer.supports(Feature::Mutations));
    }

    // --- DialectCapabilityGuard::check ---

    #[test]
    fn test_guard_ok_when_supported() {
        assert!(
            DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::Mutations).is_ok()
        );
    }

    #[test]
    fn test_guard_err_when_unsupported() {
        let result = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps);
        assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })));
    }

    #[test]
    fn test_guard_error_mentions_feature_and_dialect() {
        let err = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("JSONB"), "message should mention feature: {msg}");
        assert!(msg.contains("mysql"), "message should mention dialect: {msg}");
    }

    #[test]
    fn test_guard_error_includes_suggestion() {
        let err = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("json_extract"),
            "message should include suggestion: {msg}"
        );
    }

    #[test]
    fn test_guard_check_all_returns_all_failures() {
        let result = DialectCapabilityGuard::check_all(
            DatabaseType::SQLite,
            &[
                Feature::Mutations,
                Feature::WindowFunctions,
                Feature::CommonTableExpressions, // supported
            ],
        );
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Mutations"), "should mention mutations: {msg}");
        assert!(msg.contains("Window"), "should mention window functions: {msg}");
        // CTE is supported — must NOT appear in the error
        assert!(!msg.contains("Common Table"), "should not mention CTEs: {msg}");
    }

    #[test]
    fn test_guard_check_all_ok_when_all_supported() {
        assert!(
            DialectCapabilityGuard::check_all(
                DatabaseType::PostgreSQL,
                &[Feature::JsonbPathOps, Feature::Subscriptions, Feature::Mutations],
            )
            .is_ok()
        );
    }

    #[test]
    fn test_guard_error_links_to_compatibility_docs() {
        let err = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("docs/database-compatibility.md"),
            "unsupported feature error must link to compatibility docs: {msg}"
        );
    }

    #[test]
    fn test_guard_check_all_error_links_to_compatibility_docs() {
        let err = DialectCapabilityGuard::check_all(
            DatabaseType::SQLite,
            &[Feature::Mutations, Feature::WindowFunctions],
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("docs/database-compatibility.md"),
            "check_all error must link to compatibility docs: {msg}"
        );
    }

    // Helper: iterate all Feature variants
    fn all_features() -> impl Iterator<Item = Feature> {
        [
            Feature::JsonbPathOps,
            Feature::Subscriptions,
            Feature::Mutations,
            Feature::WindowFunctions,
            Feature::CommonTableExpressions,
            Feature::FullTextSearch,
            Feature::AdvisoryLocks,
            Feature::StddevVariance,
            Feature::Upsert,
            Feature::ArrayTypes,
            Feature::BackwardPagination,
        ]
        .into_iter()
    }
}
