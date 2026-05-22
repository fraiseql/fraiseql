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
    /// Returns [`SecurityError::QueryTooLarge`] if the query exceeds `max_size_bytes`.
    /// Returns [`SecurityError::MalformedQuery`] if GraphQL syntax is invalid.
    /// Returns [`SecurityError::QueryTooDeep`] if nesting depth exceeds `max_depth`.
    /// Returns [`SecurityError::QueryTooComplex`] if complexity exceeds `max_complexity`.
    /// Returns [`SecurityError::TooManyAliases`] if alias count exceeds `max_aliases`.
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
        let rv = RequestValidator::from_config(&ComplexityConfig {
            max_depth:      self.config.max_depth,
            max_complexity: self.config.max_complexity,
            max_aliases:    self.config.max_aliases,
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
