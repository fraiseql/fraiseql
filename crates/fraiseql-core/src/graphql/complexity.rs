// GraphQL query complexity analysis — AST-based to prevent DoS attacks.
//
// Uses `graphql-parser` to walk the document tree, correctly handling
// operation names, arguments, fragment spreads, and aliases (which a
// character-scan approach cannot distinguish from field names).

use graphql_parser::query::{
    Definition, Document, FragmentDefinition, OperationDefinition, Selection, SelectionSet,
};

/// Default maximum number of aliases per query (alias amplification protection).
///
/// This constant is the single source of truth used by [`ComplexityConfig`],
/// [`RequestValidator`], the server HTTP handler, and the CLI `explain` command.
pub const DEFAULT_MAX_ALIASES: usize = 30;

/// Configuration for query complexity limits.
#[derive(Debug, Clone)]
pub struct ComplexityConfig {
    /// Maximum query depth (nesting level) — default: 10
    pub max_depth: usize,
    /// Maximum complexity score — default: 100
    pub max_complexity: usize,
    /// Maximum number of field aliases per query — default: 30
    pub max_aliases: usize,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            max_depth:      10,
            max_complexity: 100,
            max_aliases:    DEFAULT_MAX_ALIASES,
        }
    }
}

/// Metrics returned by the AST-based analyzer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryMetrics {
    /// Maximum selection-set nesting depth.
    pub depth:       usize,
    /// Total complexity score (accounts for pagination multipliers).
    pub complexity:  usize,
    /// Number of aliased fields in the document.
    pub alias_count: usize,
}

/// Validation error types.
#[derive(Debug, thiserror::Error, Clone)]
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

    /// Query contains too many aliases (alias amplification attack).
    #[error("Query exceeds maximum alias count of {max_aliases}: count = {actual_aliases}")]
    TooManyAliases {
        /// Maximum allowed alias count
        max_aliases:    usize,
        /// Actual alias count
        actual_aliases: usize,
    },

    /// Invalid query variables.
    #[error("Invalid variables: {0}")]
    InvalidVariables(String),

    /// Malformed GraphQL query.
    #[error("Malformed GraphQL query: {0}")]
    MalformedQuery(String),
}

/// AST-based GraphQL request validator.
///
/// Uses `graphql-parser` to walk the full document tree. Correctly handles
/// operation names, arguments, fragment spreads, inline fragments, and aliases —
/// none of which a character-scan can distinguish from field names.
#[derive(Debug, Clone)]
pub struct RequestValidator {
    /// Maximum query depth allowed.
    max_depth:             usize,
    /// Maximum query complexity score allowed.
    max_complexity:        usize,
    /// Maximum number of field aliases per query (alias amplification protection).
    max_aliases_per_query: usize,
    /// Enable query depth validation.
    validate_depth:        bool,
    /// Enable query complexity validation.
    validate_complexity:   bool,
}

impl RequestValidator {
    /// Create a new validator with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a `ComplexityConfig`.
    #[must_use]
    pub const fn from_config(config: ComplexityConfig) -> Self {
        Self {
            max_depth:             config.max_depth,
            max_complexity:        config.max_complexity,
            max_aliases_per_query: config.max_aliases,
            validate_depth:        true,
            validate_complexity:   true,
        }
    }

    /// Set maximum query depth.
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set maximum query complexity.
    #[must_use]
    pub const fn with_max_complexity(mut self, max_complexity: usize) -> Self {
        self.max_complexity = max_complexity;
        self
    }

    /// Enable/disable depth validation.
    #[must_use]
    pub const fn with_depth_validation(mut self, enabled: bool) -> Self {
        self.validate_depth = enabled;
        self
    }

    /// Enable/disable complexity validation.
    #[must_use]
    pub const fn with_complexity_validation(mut self, enabled: bool) -> Self {
        self.validate_complexity = enabled;
        self
    }

    /// Set maximum number of aliases per query.
    #[must_use]
    pub const fn with_max_aliases(mut self, max_aliases: usize) -> Self {
        self.max_aliases_per_query = max_aliases;
        self
    }

    /// Compute query metrics without enforcing any limits.
    ///
    /// Returns [`QueryMetrics`] for the query.
    /// Used by CLI tooling (`explain`, `cost` commands) where raw metrics
    /// are needed without a hard rejection.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::MalformedQuery`] if the query cannot be parsed.
    pub fn analyze(&self, query: &str) -> Result<QueryMetrics, ValidationError> {
        if query.trim().is_empty() {
            return Err(ValidationError::MalformedQuery("Empty query".to_string()));
        }
        let document = graphql_parser::parse_query::<String>(query)
            .map_err(|e| ValidationError::MalformedQuery(format!("{e}")))?;
        let fragments = collect_fragments(&document);
        Ok(QueryMetrics {
            depth:       self.calculate_depth_ast(&document, &fragments),
            complexity:  self.calculate_complexity_ast(&document, &fragments),
            alias_count: self.count_aliases_ast(&document),
        })
    }

    /// Validate a GraphQL query string, enforcing configured limits.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if the query violates any validation rules.
    pub fn validate_query(&self, query: &str) -> Result<(), ValidationError> {
        if query.trim().is_empty() {
            return Err(ValidationError::MalformedQuery("Empty query".to_string()));
        }

        // Skip AST parsing only when depth, complexity, AND alias checks are all disabled.
        // The alias amplification check is a distinct DoS vector: it must run even when
        // depth and complexity validation are both turned off.
        if !self.validate_depth && !self.validate_complexity && self.max_aliases_per_query == 0 {
            return Ok(());
        }

        let document = graphql_parser::parse_query::<String>(query)
            .map_err(|e| ValidationError::MalformedQuery(format!("{e}")))?;
        let fragments = collect_fragments(&document);

        if self.validate_depth {
            let depth = self.calculate_depth_ast(&document, &fragments);
            if depth > self.max_depth {
                return Err(ValidationError::QueryTooDeep {
                    max_depth:    self.max_depth,
                    actual_depth: depth,
                });
            }
        }

        if self.validate_complexity {
            let complexity = self.calculate_complexity_ast(&document, &fragments);
            if complexity > self.max_complexity {
                return Err(ValidationError::QueryTooComplex {
                    max_complexity:    self.max_complexity,
                    actual_complexity: complexity,
                });
            }
        }

        let alias_count = self.count_aliases_ast(&document);
        if alias_count > self.max_aliases_per_query {
            return Err(ValidationError::TooManyAliases {
                max_aliases:    self.max_aliases_per_query,
                actual_aliases: alias_count,
            });
        }

        Ok(())
    }

    /// Validate variables JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if variables are not a JSON object.
    pub fn validate_variables(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> Result<(), ValidationError> {
        if let Some(vars) = variables {
            if !vars.is_object() {
                return Err(ValidationError::InvalidVariables(
                    "Variables must be an object".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn calculate_depth_ast(
        &self,
        document: &Document<String>,
        fragments: &[&FragmentDefinition<String>],
    ) -> usize {
        document
            .definitions
            .iter()
            .map(|def| match def {
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
                    self.selection_set_depth(&f.selection_set, fragments, 0)
                },
            })
            .max()
            .unwrap_or(0)
    }

    fn selection_set_depth(
        &self,
        selection_set: &SelectionSet<String>,
        fragments: &[&FragmentDefinition<String>],
        recursion_depth: usize,
    ) -> usize {
        if recursion_depth > 32 {
            return self.max_depth + 1;
        }
        if selection_set.items.is_empty() {
            return 0;
        }
        let max_child = selection_set
            .items
            .iter()
            .map(|sel| match sel {
                Selection::Field(field) => {
                    if field.selection_set.items.is_empty() {
                        0
                    } else {
                        self.selection_set_depth(&field.selection_set, fragments, recursion_depth)
                    }
                },
                Selection::InlineFragment(inline) => {
                    self.selection_set_depth(&inline.selection_set, fragments, recursion_depth)
                },
                Selection::FragmentSpread(spread) => {
                    if let Some(frag) =
                        fragments.iter().find(|f| f.name == spread.fragment_name)
                    {
                        self.selection_set_depth(
                            &frag.selection_set,
                            fragments,
                            recursion_depth + 1,
                        )
                    } else {
                        self.max_depth
                    }
                },
            })
            .max()
            .unwrap_or(0);
        1 + max_child
    }

    fn calculate_complexity_ast(
        &self,
        document: &Document<String>,
        fragments: &[&FragmentDefinition<String>],
    ) -> usize {
        document
            .definitions
            .iter()
            .map(|def| match def {
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
                Definition::Fragment(_) => 0,
            })
            .sum()
    }

    fn selection_set_complexity(
        &self,
        selection_set: &SelectionSet<String>,
        fragments: &[&FragmentDefinition<String>],
        recursion_depth: usize,
    ) -> usize {
        if recursion_depth > 32 {
            return self.max_complexity + 1;
        }
        selection_set
            .items
            .iter()
            .map(|sel| match sel {
                Selection::Field(field) => {
                    let multiplier = extract_limit_multiplier(&field.arguments);
                    if field.selection_set.items.is_empty() {
                        1
                    } else {
                        let nested = self.selection_set_complexity(
                            &field.selection_set,
                            fragments,
                            recursion_depth,
                        );
                        1 + nested * multiplier
                    }
                },
                Selection::InlineFragment(inline) => {
                    self.selection_set_complexity(&inline.selection_set, fragments, recursion_depth)
                },
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
                        10
                    }
                },
            })
            .sum()
    }

    fn count_aliases_ast(&self, document: &Document<String>) -> usize {
        document
            .definitions
            .iter()
            .map(|def| match def {
                Definition::Operation(op) => {
                    let ss = match op {
                        OperationDefinition::Query(q) => &q.selection_set,
                        OperationDefinition::Mutation(m) => &m.selection_set,
                        OperationDefinition::Subscription(s) => &s.selection_set,
                        OperationDefinition::SelectionSet(ss) => ss,
                    };
                    count_aliases_in_selection_set(ss)
                },
                Definition::Fragment(f) => count_aliases_in_selection_set(&f.selection_set),
            })
            .sum()
    }
}

impl Default for RequestValidator {
    fn default() -> Self {
        Self {
            max_depth:             10,
            max_complexity:        100,
            max_aliases_per_query: DEFAULT_MAX_ALIASES,
            validate_depth:        true,
            validate_complexity:   true,
        }
    }
}

/// Collect all fragment definitions from a parsed document.
fn collect_fragments<'a>(document: &'a Document<'a, String>) -> Vec<&'a FragmentDefinition<'a, String>> {
    document
        .definitions
        .iter()
        .filter_map(|def| {
            if let Definition::Fragment(f) = def {
                Some(f)
            } else {
                None
            }
        })
        .collect()
}

/// Extract pagination limit from field arguments to use as a cost multiplier.
fn extract_limit_multiplier(
    arguments: &[(String, graphql_parser::query::Value<String>)],
) -> usize {
    for (name, value) in arguments {
        if matches!(name.as_str(), "first" | "limit" | "take" | "last") {
            if let graphql_parser::query::Value::Int(n) = value {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: value is clamped to [1, 100] immediately after; truncation
                // and sign loss are intentional and safe here.
                let limit = n.as_i64().unwrap_or(10) as usize;
                return limit.clamp(1, 100);
            }
        }
    }
    1
}

/// Recursively count aliases in a selection set.
fn count_aliases_in_selection_set(selection_set: &SelectionSet<String>) -> usize {
    selection_set
        .items
        .iter()
        .map(|sel| match sel {
            Selection::Field(field) => {
                let self_alias = usize::from(field.alias.is_some());
                self_alias + count_aliases_in_selection_set(&field.selection_set)
            },
            Selection::InlineFragment(inline) => {
                count_aliases_in_selection_set(&inline.selection_set)
            },
            Selection::FragmentSpread(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Regression tests: operation names and arguments must NOT be counted ──

    #[test]
    fn test_operation_name_not_counted_as_field() {
        let validator = RequestValidator::default();
        let metrics = validator
            .analyze("query getUserPosts { users { id name } }")
            .expect("valid query");
        // "getUserPosts" is the operation name — must not count as a field.
        // Fields: users→(id, name) = complexity 3
        assert!(
            metrics.complexity <= 10,
            "operation name must not inflate complexity; got {metrics:?}"
        );
    }

    #[test]
    fn test_arguments_not_counted_as_fields() {
        let validator = RequestValidator::default();
        let metrics = validator
            .analyze("{ users(limit: 10, offset: 0) { id } }")
            .expect("valid query");
        // "limit" and "offset" are arguments, NOT fields.
        assert!(
            metrics.complexity < 50,
            "arguments must not be counted as fields; got {metrics:?}"
        );
    }

    // ── Depth ──

    #[test]
    fn test_simple_query_depth() {
        let validator = RequestValidator::default();
        let metrics = validator.analyze("{ users { id name } }").expect("valid");
        assert_eq!(metrics.depth, 2);
    }

    #[test]
    fn test_deeply_nested_query_depth() {
        let validator = RequestValidator::default();
        let query = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        let metrics = validator.analyze(query).expect("valid");
        assert!(metrics.depth >= 8, "expected depth ≥ 8, got {}", metrics.depth);
    }

    #[test]
    fn test_depth_validation_pass() {
        let validator = RequestValidator::default().with_max_depth(5);
        assert!(validator.validate_query("{ user { id } }").is_ok());
    }

    #[test]
    fn test_depth_validation_fail() {
        let validator = RequestValidator::default().with_max_depth(3);
        let deep = "{ user { profile { settings { theme } } } }";
        assert!(validator.validate_query(deep).is_err());
    }

    // ── Fragment depth bypass ──

    #[test]
    fn test_fragment_depth_bypass_blocked() {
        let validator = RequestValidator::new().with_max_depth(3);
        let query = "
            fragment Deep on User { a { b { c { d { e } } } } }
            query { ...Deep }
        ";
        assert!(
            validator.validate_query(query).is_err(),
            "fragment depth bypass must be blocked"
        );
    }

    #[test]
    fn test_shallow_fragment_allowed() {
        let validator = RequestValidator::new().with_max_depth(5);
        let query = "
            fragment UserFields on User { id name email }
            query { user { ...UserFields } }
        ";
        assert!(validator.validate_query(query).is_ok());
    }

    // ── Complexity ──

    #[test]
    fn test_complexity_validation_pass() {
        let validator = RequestValidator::default().with_max_complexity(20);
        assert!(validator.validate_query("query { user { id name email } }").is_ok());
    }

    #[test]
    fn test_pagination_limit_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);
        let query = "query { users(first: 100) { id name } }";
        assert!(
            validator.validate_query(query).is_err(),
            "high pagination limits must increase complexity"
        );
    }

    #[test]
    fn test_nested_list_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);
        let query = "query { users(first: 10) { friends(first: 10) { id } } }";
        assert!(
            validator.validate_query(query).is_err(),
            "nested list multipliers must compound"
        );
    }

    // ── Aliases ──

    #[test]
    fn test_alias_count_within_limit() {
        let validator = RequestValidator::new().with_max_aliases(5);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(validator.validate_query(query).is_ok());
    }

    #[test]
    fn test_alias_count_exceeds_limit() {
        let validator = RequestValidator::new().with_max_aliases(2);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(
            matches!(
                validator.validate_query(query),
                Err(ValidationError::TooManyAliases { actual_aliases: 3, .. })
            ),
            "should report alias count"
        );
    }

    #[test]
    fn test_default_alias_limit_is_30() {
        let validator = RequestValidator::new();
        let fields_30: String = (0..30).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "f{i}: user {{ id }} ");
            s
        });
        assert!(validator.validate_query(&format!("query {{ {fields_30} }}")).is_ok());

        let fields_31: String = (0..31).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "f{i}: user {{ id }} ");
            s
        });
        assert!(validator.validate_query(&format!("query {{ {fields_31} }}")).is_err());
    }

    // ── Parse errors ──

    #[test]
    fn test_empty_query_rejected() {
        let validator = RequestValidator::new();
        assert!(validator.validate_query("").is_err());
        assert!(validator.validate_query("   ").is_err());
    }

    #[test]
    fn test_malformed_query_rejected() {
        let validator = RequestValidator::new();
        assert!(validator.validate_query("{ invalid query {{}}").is_err());
    }

    // ── Variables ──

    #[test]
    fn test_valid_variables() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!({"id": "123"});
        assert!(validator.validate_variables(Some(&vars)).is_ok());
    }

    #[test]
    fn test_invalid_variables_not_object() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!([1, 2, 3]);
        assert!(validator.validate_variables(Some(&vars)).is_err());
    }

    // ── Disabled validation ──

    #[test]
    fn test_disable_depth_and_complexity_validation() {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_depth(1)
            .with_max_complexity(1);
        let deep = "{ a { b { c { d { e { f } } } } } }";
        assert!(validator.validate_query(deep).is_ok());
    }

    // ── from_config ──

    #[test]
    fn test_from_config() {
        let config = ComplexityConfig {
            max_depth:      5,
            max_complexity: 20,
            max_aliases:    3,
        };
        let validator = RequestValidator::from_config(config);
        // Depth-6 query should fail
        assert!(validator
            .validate_query("{ a { b { c { d { e { f } } } } } }")
            .is_err());
    }
}
