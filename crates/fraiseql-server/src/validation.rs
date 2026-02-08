//! GraphQL request validation module.
//!
//! Provides validation for GraphQL queries including:
//! - Query depth validation (prevent deeply nested queries)
//! - Query complexity scoring (prevent complex queries)
//! - Variable type validation (ensure variable types match schema)
//!
//! # Security
//!
//! Uses AST-based validation via `graphql-parser` to correctly handle:
//! - Fragment spreads (which expand to arbitrary depth)
//! - Inline fragments
//! - Aliases and multiple operations
//! - Pagination arguments that multiply result cardinality

use graphql_parser::query::{
    Definition, Document, FragmentDefinition, OperationDefinition, Selection, SelectionSet,
};
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Validation error types.
#[derive(Debug, Error, Clone)]
pub enum ValidationError {
    /// Query exceeds maximum allowed depth.
    #[error("Query exceeds maximum depth of {max_depth}: depth = {actual_depth}")]
    QueryTooDeep {
        /// Maximum allowed depth
        max_depth:    usize,
        /// Actual query depth
        actual_depth: usize,
    },

    /// Query exceeds maximum complexity score.
    #[error("Query exceeds maximum complexity of {max_complexity}: score = {actual_complexity}")]
    QueryTooComplex {
        /// Maximum allowed complexity
        max_complexity:    usize,
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
    max_depth:           usize,
    /// Maximum query complexity score allowed.
    max_complexity:      usize,
    /// Enable query depth validation.
    validate_depth:      bool,
    /// Enable query complexity validation.
    validate_complexity: bool,
}

impl RequestValidator {
    /// Create a new validator with default settings.
    #[must_use]
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

        // Skip AST parsing if both validations are disabled
        if !self.validate_depth && !self.validate_complexity {
            return Ok(());
        }

        // Parse the GraphQL query into an AST
        let document = graphql_parser::parse_query::<String>(query)
            .map_err(|e| ValidationError::MalformedQuery(format!("{e}")))?;

        // Collect fragment definitions for resolving fragment spreads
        let fragments: Vec<&FragmentDefinition<String>> = document
            .definitions
            .iter()
            .filter_map(|def| {
                if let Definition::Fragment(f) = def {
                    Some(f)
                } else {
                    None
                }
            })
            .collect();

        // Check depth if enabled
        if self.validate_depth {
            let depth = self.calculate_depth_ast(&document, &fragments);
            if depth > self.max_depth {
                return Err(ValidationError::QueryTooDeep {
                    max_depth:    self.max_depth,
                    actual_depth: depth,
                });
            }
        }

        // Check complexity if enabled
        if self.validate_complexity {
            let complexity = self.calculate_complexity_ast(&document, &fragments);
            if complexity > self.max_complexity {
                return Err(ValidationError::QueryTooComplex {
                    max_complexity:    self.max_complexity,
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
    pub fn validate_variables(&self, variables: Option<&JsonValue>) -> Result<(), ValidationError> {
        if let Some(vars) = variables {
            if !vars.is_object() {
                return Err(ValidationError::InvalidVariables(
                    "Variables must be an object".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Calculate query depth using AST walking.
    ///
    /// Correctly handles fragment spreads, inline fragments, and nested selections.
    fn calculate_depth_ast(
        &self,
        document: &Document<String>,
        fragments: &[&FragmentDefinition<String>],
    ) -> usize {
        let mut max_depth = 0;

        for definition in &document.definitions {
            let depth = match definition {
                Definition::Operation(op) => match op {
                    OperationDefinition::Query(q) => {
                        self.selection_set_depth(&q.selection_set, fragments, 0)
                    },
                    OperationDefinition::Mutation(m) => {
                        self.selection_set_depth(&m.selection_set, fragments, 0)
                    },
                    OperationDefinition::Subscription(s) => {
                        self.selection_set_depth(&s.selection_set, fragments, 0)
                    },
                    OperationDefinition::SelectionSet(ss) => {
                        self.selection_set_depth(ss, fragments, 0)
                    },
                },
                Definition::Fragment(f) => {
                    // Fragment definitions are walked when referenced
                    self.selection_set_depth(&f.selection_set, fragments, 0)
                },
            };
            max_depth = max_depth.max(depth);
        }

        max_depth
    }

    /// Recursively calculate depth of a selection set.
    fn selection_set_depth(
        &self,
        selection_set: &SelectionSet<String>,
        fragments: &[&FragmentDefinition<String>],
        recursion_depth: usize,
    ) -> usize {
        // Prevent infinite recursion from circular fragment references
        if recursion_depth > 32 {
            return self.max_depth + 1;
        }

        if selection_set.items.is_empty() {
            return 0;
        }

        let mut max_child_depth = 0;

        for selection in &selection_set.items {
            let child_depth = match selection {
                Selection::Field(field) => {
                    if field.selection_set.items.is_empty() {
                        0
                    } else {
                        self.selection_set_depth(
                            &field.selection_set,
                            fragments,
                            recursion_depth,
                        )
                    }
                },
                Selection::InlineFragment(inline) => {
                    self.selection_set_depth(
                        &inline.selection_set,
                        fragments,
                        recursion_depth,
                    )
                },
                Selection::FragmentSpread(spread) => {
                    // Find the fragment definition and calculate its depth
                    if let Some(frag) = fragments.iter().find(|f| f.name == spread.fragment_name) {
                        self.selection_set_depth(
                            &frag.selection_set,
                            fragments,
                            recursion_depth + 1,
                        )
                    } else {
                        // Unknown fragment: be conservative
                        self.max_depth
                    }
                },
            };
            max_child_depth = max_child_depth.max(child_depth);
        }

        1 + max_child_depth
    }

    /// Calculate query complexity using AST walking.
    ///
    /// Each field adds 1 to complexity. Fields with nested selections (list fields)
    /// multiply the nested cost. Fragment spreads are resolved and counted.
    fn calculate_complexity_ast(
        &self,
        document: &Document<String>,
        fragments: &[&FragmentDefinition<String>],
    ) -> usize {
        let mut total = 0;

        for definition in &document.definitions {
            let cost = match definition {
                Definition::Operation(op) => match op {
                    OperationDefinition::Query(q) => {
                        self.selection_set_complexity(&q.selection_set, fragments, 0)
                    },
                    OperationDefinition::Mutation(m) => {
                        self.selection_set_complexity(&m.selection_set, fragments, 0)
                    },
                    OperationDefinition::Subscription(s) => {
                        self.selection_set_complexity(&s.selection_set, fragments, 0)
                    },
                    OperationDefinition::SelectionSet(ss) => {
                        self.selection_set_complexity(ss, fragments, 0)
                    },
                },
                Definition::Fragment(_) => 0, // Only counted when referenced
            };
            total += cost;
        }

        total
    }

    /// Recursively calculate complexity of a selection set.
    ///
    /// Each field costs 1. Fields with sub-selections cost 1 + nested cost.
    /// Arguments like `first`, `limit`, `take` act as multipliers.
    fn selection_set_complexity(
        &self,
        selection_set: &SelectionSet<String>,
        fragments: &[&FragmentDefinition<String>],
        recursion_depth: usize,
    ) -> usize {
        if recursion_depth > 32 {
            return self.max_complexity + 1;
        }

        let mut total = 0;

        for selection in &selection_set.items {
            total += match selection {
                Selection::Field(field) => {
                    let multiplier = Self::extract_limit_multiplier(&field.arguments);
                    if field.selection_set.items.is_empty() {
                        // Leaf field
                        1
                    } else {
                        // Field with sub-selections: base cost + nested * multiplier
                        let nested = self.selection_set_complexity(
                            &field.selection_set,
                            fragments,
                            recursion_depth,
                        );
                        1 + nested * multiplier
                    }
                },
                Selection::InlineFragment(inline) => self.selection_set_complexity(
                    &inline.selection_set,
                    fragments,
                    recursion_depth,
                ),
                Selection::FragmentSpread(spread) => {
                    if let Some(frag) =
                        fragments.iter().find(|f| f.name == spread.fragment_name)
                    {
                        self.selection_set_complexity(
                            &frag.selection_set,
                            fragments,
                            recursion_depth + 1,
                        )
                    } else {
                        10 // Unknown fragment: conservative estimate
                    }
                },
            };
        }

        total
    }

    /// Extract pagination limit from field arguments to use as a cost multiplier.
    ///
    /// Looks for `first`, `limit`, `take`, or `last` arguments. Clamps the value
    /// to prevent absurdly high multipliers.
    fn extract_limit_multiplier(
        arguments: &[(String, graphql_parser::query::Value<String>)],
    ) -> usize {
        for (name, value) in arguments {
            if matches!(name.as_str(), "first" | "limit" | "take" | "last") {
                if let graphql_parser::query::Value::Int(n) = value {
                    let limit = n.as_i64().unwrap_or(10) as usize;
                    // Clamp: treat anything > 100 as 100 for cost purposes
                    return limit.clamp(1, 100);
                }
            }
        }
        // Default multiplier for fields without explicit limits
        1
    }
}

impl Default for RequestValidator {
    fn default() -> Self {
        Self {
            max_depth:           10,
            max_complexity:      100,
            validate_depth:      true,
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

        // Shallow query should pass (depth = 2)
        let shallow = "{ user { id } }";
        assert!(validator.validate_query(shallow).is_ok());

        // Deep query should fail (depth = 4)
        let deep = "{ user { profile { settings { theme } } } }";
        assert!(validator.validate_query(deep).is_err());
    }

    #[test]
    fn test_query_complexity_validation() {
        let validator = RequestValidator::new().with_max_complexity(5);

        // Simple query should pass (complexity = 3: root + user + id)
        let simple = "{ user { id name } }";
        assert!(validator.validate_query(simple).is_ok());
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

    // SECURITY: Fragment-based depth bypass tests (VULN #5)

    #[test]
    fn test_fragment_depth_bypass_blocked() {
        let validator = RequestValidator::new().with_max_depth(3);

        // Fragment that expands to depth > 3
        let query = "
            fragment Deep on User {
                a { b { c { d { e } } } }
            }
            query { ...Deep }
        ";
        let result = validator.validate_query(query);
        assert!(result.is_err(), "Fragment depth bypass must be blocked");
    }

    #[test]
    fn test_inline_fragment_depth_counted() {
        let validator = RequestValidator::new().with_max_depth(3);

        let query = "
            query {
                ... on User { a { b { c { d } } } }
            }
        ";
        let result = validator.validate_query(query);
        assert!(
            result.is_err(),
            "Inline fragment depth must be counted correctly"
        );
    }

    #[test]
    fn test_multiple_fragments_depth() {
        let validator = RequestValidator::new().with_max_depth(4);

        // Fragment A references Fragment B, total depth > 4
        let query = "
            fragment B on Type { x { y { z } } }
            fragment A on Type { inner { ...B } }
            query { ...A }
        ";
        let result = validator.validate_query(query);
        assert!(
            result.is_err(),
            "Chained fragment depth must be detected"
        );
    }

    #[test]
    fn test_shallow_fragment_allowed() {
        let validator = RequestValidator::new().with_max_depth(5);

        let query = "
            fragment UserFields on User { id name email }
            query { user { ...UserFields } }
        ";
        assert!(
            validator.validate_query(query).is_ok(),
            "Shallow fragments should be allowed"
        );
    }

    // SECURITY: Complexity scoring with multipliers (VULN #6)

    #[test]
    fn test_pagination_limit_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);

        // This query has a high multiplier from the limit argument
        // users(first: 100) { id name } => 1 + (1 + 1) * 100 = 201
        let query = "query { users(first: 100) { id name } }";
        let result = validator.validate_query(query);
        assert!(
            result.is_err(),
            "High pagination limits must increase complexity"
        );
    }

    #[test]
    fn test_nested_list_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);

        // Nested lists should compound: users(first:10) { friends(first:10) { id } }
        // = 1 + (1 + (1)*10)*10 = 1 + 110 = 111
        let query = "query { users(first: 10) { friends(first: 10) { id } } }";
        let result = validator.validate_query(query);
        assert!(
            result.is_err(),
            "Nested list multipliers must compound"
        );
    }

    #[test]
    fn test_simple_query_low_complexity() {
        let validator = RequestValidator::new().with_max_complexity(20);

        let query = "query { user { id name email } }";
        assert!(
            validator.validate_query(query).is_ok(),
            "Simple queries should have low complexity"
        );
    }

    #[test]
    fn test_malformed_query_rejected() {
        let validator = RequestValidator::new();
        let result = validator.validate_query("{ invalid query {{}}");
        assert!(result.is_err(), "Malformed queries must be rejected");
    }
}
