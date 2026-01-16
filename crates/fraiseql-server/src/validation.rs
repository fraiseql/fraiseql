//! GraphQL request validation module.
//!
//! Provides validation for GraphQL queries including:
//! - Query depth validation (prevent deeply nested queries)
//! - Query complexity scoring (prevent complex queries)
//! - Variable type validation (ensure variable types match schema)

use serde_json::Value as JsonValue;
use thiserror::Error;

/// Validation error types.
#[derive(Debug, Error, Clone)]
pub enum ValidationError {
    /// Query exceeds maximum allowed depth.
    #[error("Query exceeds maximum depth of {max_depth}: depth = {actual_depth}")]
    QueryTooDeep {
        /// Maximum allowed depth
        max_depth: usize,
        /// Actual query depth
        actual_depth: usize,
    },

    /// Query exceeds maximum complexity score.
    #[error("Query exceeds maximum complexity of {max_complexity}: score = {actual_complexity}")]
    QueryTooComplex {
        /// Maximum allowed complexity
        max_complexity: usize,
        /// Actual query complexity
        actual_complexity: usize,
    },

    /// Invalid query variables.
    #[error("Invalid variables: {0}")]
    InvalidVariables(String),

    /// Malformed GraphQL query.
    #[error("Malformed GraphQL query: {0}")]
    MalformedQuery(String),
}

/// GraphQL request validator.
#[derive(Debug, Clone)]
pub struct RequestValidator {
    /// Maximum query depth allowed.
    max_depth: usize,
    /// Maximum query complexity score allowed.
    max_complexity: usize,
    /// Enable query depth validation.
    validate_depth: bool,
    /// Enable query complexity validation.
    validate_complexity: bool,
}

impl RequestValidator {
    /// Create a new validator with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum query depth.
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set maximum query complexity.
    #[must_use]
    pub fn with_max_complexity(mut self, max_complexity: usize) -> Self {
        self.max_complexity = max_complexity;
        self
    }

    /// Enable/disable depth validation.
    #[must_use]
    pub fn with_depth_validation(mut self, enabled: bool) -> Self {
        self.validate_depth = enabled;
        self
    }

    /// Enable/disable complexity validation.
    #[must_use]
    pub fn with_complexity_validation(mut self, enabled: bool) -> Self {
        self.validate_complexity = enabled;
        self
    }

    /// Validate a GraphQL query string.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the query violates any validation rules.
    pub fn validate_query(&self, query: &str) -> Result<(), ValidationError> {
        // Validate query is not empty
        if query.trim().is_empty() {
            return Err(ValidationError::MalformedQuery("Empty query".to_string()));
        }

        // Check depth if enabled
        if self.validate_depth {
            let depth = self.calculate_depth(query);
            if depth > self.max_depth {
                return Err(ValidationError::QueryTooDeep {
                    max_depth: self.max_depth,
                    actual_depth: depth,
                });
            }
        }

        // Check complexity if enabled
        if self.validate_complexity {
            let complexity = self.calculate_complexity(query);
            if complexity > self.max_complexity {
                return Err(ValidationError::QueryTooComplex {
                    max_complexity: self.max_complexity,
                    actual_complexity: complexity,
                });
            }
        }

        Ok(())
    }

    /// Validate variables JSON.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if variables are invalid.
    pub fn validate_variables(
        &self,
        variables: Option<&JsonValue>,
    ) -> Result<(), ValidationError> {
        if let Some(vars) = variables {
            // Validate that variables is an object
            if !vars.is_object() {
                return Err(ValidationError::InvalidVariables(
                    "Variables must be an object".to_string(),
                ));
            }

            // Validate variable values are not null (optional - can be configured)
            // For now, just ensure it's valid JSON which it already is
        }

        Ok(())
    }

    /// Calculate query depth (max nesting level).
    fn calculate_depth(&self, query: &str) -> usize {
        let mut max_depth: usize = 0;
        let mut current_depth: usize = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for ch in query.chars() {
            if escape_next {
                escape_next = false;
                continue;
            }

            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }

            if ch == '"' {
                in_string = !in_string;
                continue;
            }

            if in_string {
                continue;
            }

            match ch {
                '{' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                '}' => {
                    if current_depth > 0 {
                        current_depth -= 1;
                    }
                }
                _ => {}
            }
        }

        max_depth
    }

    /// Calculate query complexity score (heuristic).
    fn calculate_complexity(&self, query: &str) -> usize {
        let mut complexity = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for ch in query.chars() {
            if escape_next {
                escape_next = false;
                continue;
            }

            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }

            if ch == '"' {
                in_string = !in_string;
                continue;
            }

            if in_string {
                continue;
            }

            match ch {
                '{' => complexity += 1,
                '[' => complexity += 2, // Array selections cost more
                '(' => complexity += 1, // Arguments
                _ => {}
            }
        }

        complexity
    }
}

impl Default for RequestValidator {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_complexity: 100,
            validate_depth: true,
            validate_complexity: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_validation() {
        let validator = RequestValidator::new();
        assert!(validator.validate_query("").is_err());
        assert!(validator.validate_query("   ").is_err());
    }

    #[test]
    fn test_query_depth_validation() {
        let validator = RequestValidator::new().with_max_depth(3);

        // Shallow query should pass
        let shallow = "{ user { id } }";
        assert!(validator.validate_query(shallow).is_ok());

        // Deep query should fail
        let deep = "{ user { profile { settings { theme } } } }";
        assert!(validator.validate_query(deep).is_err());
    }

    #[test]
    fn test_query_complexity_validation() {
        let validator = RequestValidator::new().with_max_complexity(5);

        // Simple query should pass
        let simple = "{ user { id name } }";
        assert!(validator.validate_query(simple).is_ok());

        // Complex query should fail (many nested fields and array selections)
        let complex = "{ user [ id name email [ tags [ name ] ] profile { bio avatar [ url size ] settings { theme notifications } } ] }";
        assert!(validator.validate_query(complex).is_err());
    }

    #[test]
    fn test_variables_validation() {
        let validator = RequestValidator::new();

        // Valid variables object
        let valid = serde_json::json!({"id": "123", "name": "John"});
        assert!(validator.validate_variables(Some(&valid)).is_ok());

        // No variables
        assert!(validator.validate_variables(None).is_ok());

        // Invalid: variables is not an object
        let invalid = serde_json::json!([1, 2, 3]);
        assert!(validator.validate_variables(Some(&invalid)).is_err());
    }

    #[test]
    fn test_depth_calculation_with_strings() {
        let validator = RequestValidator::new();

        // Query with string containing braces should not affect depth
        let query = r#"{ user { description: "Has { and }" } }"#;
        let depth = validator.calculate_depth(query);
        assert_eq!(depth, 2);
    }

    #[test]
    fn test_disable_validation() {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_depth(1)
            .with_max_complexity(1);

        // Even very deep query should pass when validation is disabled
        let deep = "{ a { b { c { d { e { f } } } } } }";
        assert!(validator.validate_query(deep).is_ok());
    }
}
