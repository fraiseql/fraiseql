//! Query Validator
//!
//! This module provides query validation for GraphQL queries.
//! It validates:
//! - Query size (maximum bytes, O(1) check — no parsing required)
//! - Query depth (maximum nesting levels) — via AST analysis
//! - Query complexity (weighted scoring of fields) — via AST analysis
//! - Alias count (alias amplification protection) — via AST analysis
//!
//! # Architecture
//!
//! The Query Validator acts as the third layer in the security middleware:
//! ```text
//! GraphQL Query String
//!     ↓
//! QueryValidator::validate()
//!     ├─ Check 1: Validate query size (O(1) byte count)
//!     ├─ Check 2: AST-based analysis (depth, complexity, alias count)
//!     │           via `RequestValidator` from `graphql::complexity`
//!     ├─ Check 3: Check query depth
//!     ├─ Check 4: Check query complexity
//!     └─ Check 5: Check alias count (alias amplification protection)
//!     ↓
//! Result<QueryMetrics> (validation passed or error)
//! ```
//!
//! # Examples
//!
//! ```rust
//! use fraiseql_core::security::{QueryValidator, QueryValidatorConfig};
//!
//! // Create validator with standard limits
//! let config = QueryValidatorConfig {
//!     max_depth: 10,
//!     max_complexity: 1000,
//!     max_size_bytes: 100_000,
//!     max_aliases: 30,
//! };
//! let validator = QueryValidator::from_config(config);
//!
//! // Validate a query
//! let query = "{ user { posts { comments { author { name } } } } }";
//! let metrics = validator.validate(query).unwrap();
//! println!("Query depth: {}", metrics.depth);
//! println!("Query complexity: {}", metrics.complexity);
//! println!("Query aliases: {}", metrics.alias_count);
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    graphql::complexity::{ComplexityConfig, QueryMetrics, RequestValidator},
    security::errors::{Result, SecurityError},
};

/// Query validation configuration
///
/// Defines limits for query depth, complexity, size, and alias count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryValidatorConfig {
    /// Maximum nesting depth for queries
    pub max_depth: usize,

    /// Maximum complexity score for queries
    pub max_complexity: usize,

    /// Maximum query size in bytes
    pub max_size_bytes: usize,

    /// Maximum number of field aliases per query (alias amplification protection).
    pub max_aliases: usize,
}

impl QueryValidatorConfig {
    /// Create a permissive query validation configuration
    ///
    /// - Max depth: 20 levels
    /// - Max complexity: 5000
    /// - Max size: 1 MB
    /// - Max aliases: 100
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            max_depth:      20,
            max_complexity: 5000,
            max_size_bytes: 1_000_000, // 1 MB
            max_aliases:    100,
        }
    }

    /// Create a standard query validation configuration
    ///
    /// - Max depth: 10 levels
    /// - Max complexity: 1000
    /// - Max size: 256 KB
    /// - Max aliases: 30
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            max_depth:      10,
            max_complexity: 1000,
            max_size_bytes: 256_000, // 256 KB
            max_aliases:    30,
        }
    }

    /// Create a strict query validation configuration
    ///
    /// - Max depth: 5 levels
    /// - Max complexity: 500
    /// - Max size: 64 KB (regulated environments)
    /// - Max aliases: 10
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            max_depth:      5,
            max_complexity: 500,
            max_size_bytes: 64_000, // 64 KB
            max_aliases:    10,
        }
    }
}

/// Query Validator
///
/// Validates incoming GraphQL queries against security policies.
/// Acts as the third layer in the security middleware pipeline.
///
/// Delegates AST-based analysis to [`RequestValidator`] from
/// `graphql::complexity` — the single source of truth for depth,
/// complexity, and alias-amplification logic.
#[derive(Debug, Clone)]
pub struct QueryValidator {
    config: QueryValidatorConfig,
}

impl QueryValidator {
    /// Create a new query validator from configuration
    #[must_use]
    pub const fn from_config(config: QueryValidatorConfig) -> Self {
        Self { config }
    }

    /// Create validator with permissive settings
    #[must_use]
    pub const fn permissive() -> Self {
        Self::from_config(QueryValidatorConfig::permissive())
    }

    /// Create validator with standard settings
    #[must_use]
    pub const fn standard() -> Self {
        Self::from_config(QueryValidatorConfig::standard())
    }

    /// Create validator with strict settings
    #[must_use]
    pub const fn strict() -> Self {
        Self::from_config(QueryValidatorConfig::strict())
    }

    /// Validate a GraphQL query, enforcing all configured limits.
    ///
    /// Performs checks in order:
    /// 1. Query size (O(1) — no parsing)
    /// 2. AST parse (rejects malformed GraphQL)
    /// 3. Query depth
    /// 4. Query complexity
    /// 5. Alias count (alias amplification protection)
    ///
    /// Returns [`QueryMetrics`] if all checks pass, or the first
    /// [`SecurityError`] encountered.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::QueryTooLarge`] if the query exceeds `max_size_bytes`,
    /// [`SecurityError::MalformedQuery`] if the query cannot be parsed,
    /// [`SecurityError::QueryTooDeep`] if nesting exceeds `max_depth`,
    /// [`SecurityError::QueryTooComplex`] if the complexity score exceeds `max_complexity`, or
    /// [`SecurityError::TooManyAliases`] if the alias count exceeds `max_aliases`.
    pub fn validate(&self, query: &str) -> Result<QueryMetrics> {
        // Check 1: Validate query size (O(1) — pre-parse)
        let size_bytes = query.len();
        if size_bytes > self.config.max_size_bytes {
            return Err(SecurityError::QueryTooLarge {
                size:     size_bytes,
                max_size: self.config.max_size_bytes,
            });
        }

        // Checks 2–5: AST-based analysis via RequestValidator
        let rv = RequestValidator::from_config(ComplexityConfig {
            max_depth: self.config.max_depth,
            max_complexity: self.config.max_complexity,
            max_aliases: self.config.max_aliases,
            ..ComplexityConfig::default()
        });

        let metrics =
            rv.analyze(query).map_err(|e| SecurityError::MalformedQuery(e.to_string()))?;

        // Check 3: Query depth
        if metrics.depth > self.config.max_depth {
            return Err(SecurityError::QueryTooDeep {
                depth:     metrics.depth,
                max_depth: self.config.max_depth,
            });
        }

        // Check 4: Query complexity
        if metrics.complexity > self.config.max_complexity {
            return Err(SecurityError::QueryTooComplex {
                complexity:     metrics.complexity,
                max_complexity: self.config.max_complexity,
            });
        }

        // Check 5: Alias count
        if metrics.alias_count > self.config.max_aliases {
            return Err(SecurityError::TooManyAliases {
                alias_count: metrics.alias_count,
                max_aliases: self.config.max_aliases,
            });
        }

        Ok(metrics)
    }

    /// Get the underlying configuration
    #[must_use]
    pub const fn config(&self) -> &QueryValidatorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    // ============================================================================
    // Check 1: Query Size Validation Tests
    // ============================================================================

    fn large_query(size: usize) -> String {
        "{ ".to_string() + &"field ".repeat(size) + "}"
    }

    #[test]
    fn test_query_size_within_limit() {
        let validator = QueryValidator::standard();
        let result = validator.validate("{ user { id name } }");
        assert!(result.is_ok());
    }

    #[test]
    fn test_query_size_exceeds_limit() {
        let validator = QueryValidator::standard();
        let q = large_query(100_000);
        let result = validator.validate(&q);
        assert!(matches!(result, Err(SecurityError::QueryTooLarge { .. })));
    }

    // ============================================================================
    // Check 2: Malformed query
    // ============================================================================

    #[test]
    fn test_malformed_query_returns_error() {
        let validator = QueryValidator::standard();
        let result = validator.validate("this is not graphql {{{}}}");
        assert!(
            matches!(result, Err(SecurityError::MalformedQuery(_))),
            "malformed query must return MalformedQuery error, got {result:?}"
        );
    }

    // ============================================================================
    // Check 3: Query Depth Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_query_depth() {
        let validator = QueryValidator::standard();
        let result = validator.validate("{ user { id name } }");
        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert!(metrics.depth <= validator.config().max_depth);
    }

    #[test]
    fn test_query_depth_exceeds_limit() {
        let validator = QueryValidator::strict(); // max_depth = 5
        // depth = 7 (a→b→c→d→e→f→g)
        let deep = "{ a { b { c { d { e { f { g } } } } } } }";
        let result = validator.validate(deep);
        assert!(
            matches!(result, Err(SecurityError::QueryTooDeep { .. })),
            "depth-7 query must be rejected with strict (max=5), got {result:?}"
        );
    }

    #[test]
    fn test_very_deep_query_rejected() {
        let validator = QueryValidator::strict(); // max_depth = 5
        // depth = 8 (a→b→c→d→e→f→g→h)
        let deep = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        let result = validator.validate(deep);
        assert!(
            matches!(result, Err(SecurityError::QueryTooDeep { .. })),
            "depth-8 query must be rejected, got {result:?}"
        );
    }

    // ============================================================================
    // Check 4: Query Complexity Validation Tests
    // ============================================================================

    #[test]
    fn test_valid_query_complexity() {
        let validator = QueryValidator::standard();
        let result = validator.validate("{ user { id name } }");
        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert!(metrics.complexity <= validator.config().max_complexity);
    }

    #[test]
    fn test_complexity_calculated() {
        let validator = QueryValidator::standard();
        let metrics = validator.validate("{ user { id } }").unwrap();
        assert!(metrics.complexity > 0);
    }

    // ============================================================================
    // Check 5: Alias amplification protection
    // ============================================================================

    #[test]
    fn test_alias_amplification_rejected() {
        let validator = QueryValidator::standard(); // max_aliases = 30
        let aliases: String =
            (0..31).map(|i| ["a", &i.to_string(), ": user { id } "].concat()).collect();
        let query = format!("{{ {aliases} }}");
        let result = validator.validate(&query);
        assert!(
            matches!(
                result,
                Err(SecurityError::TooManyAliases {
                    alias_count: 31,
                    max_aliases: 30,
                })
            ),
            "31-alias query must be rejected with TooManyAliases, got {result:?}"
        );
    }

    #[test]
    fn test_alias_within_limit_allowed() {
        let validator = QueryValidator::standard(); // max_aliases = 30
        let aliases: String =
            (0..5).map(|i| ["a", &i.to_string(), ": user { id } "].concat()).collect();
        let query = format!("{{ {aliases} }}");
        let result = validator.validate(&query);
        assert!(result.is_ok(), "5 aliases should be allowed, got {result:?}");
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_permissive_config() {
        let config = QueryValidatorConfig::permissive();
        assert_eq!(config.max_depth, 20);
        assert_eq!(config.max_complexity, 5000);
        assert_eq!(config.max_size_bytes, 1_000_000);
        assert_eq!(config.max_aliases, 100);
    }

    #[test]
    fn test_standard_config() {
        let config = QueryValidatorConfig::standard();
        assert_eq!(config.max_depth, 10);
        assert_eq!(config.max_complexity, 1000);
        assert_eq!(config.max_size_bytes, 256_000);
        assert_eq!(config.max_aliases, 30);
    }

    #[test]
    fn test_strict_config() {
        let config = QueryValidatorConfig::strict();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.max_complexity, 500);
        assert_eq!(config.max_size_bytes, 64_000);
        assert_eq!(config.max_aliases, 10);
    }

    #[test]
    fn test_validator_helpers() {
        let permissive = QueryValidator::permissive();
        assert_eq!(permissive.config().max_depth, 20);

        let standard = QueryValidator::standard();
        assert_eq!(standard.config().max_depth, 10);

        let strict = QueryValidator::strict();
        assert_eq!(strict.config().max_depth, 5);
    }

    // ============================================================================
    // Metrics Tests
    // ============================================================================

    #[test]
    fn test_metrics_returned_on_valid_query() {
        let validator = QueryValidator::standard();
        let query = "{ user { id name } }";
        let metrics = validator.validate(query).unwrap();
        assert!(metrics.depth >= 2); // user → (id, name)
        assert!(metrics.complexity > 0);
        assert_eq!(metrics.alias_count, 0);
    }

    #[test]
    fn test_alias_count_in_metrics() {
        let validator = QueryValidator::standard();
        let query = "{ a: user { id } b: user { id } }";
        let metrics = validator.validate(query).unwrap();
        assert_eq!(metrics.alias_count, 2);
    }
}
