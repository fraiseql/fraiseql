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

/// Maximum number of variables per request (`DoS` protection).
///
/// A single GraphQL request with thousands of variables can cause excessive memory
/// allocation during deserialization and variable injection. This constant caps
/// the number of top-level keys in the `variables` JSON object.
pub const MAX_VARIABLES_COUNT: usize = 1_000;

/// Configuration for query complexity limits.
#[derive(Debug, Clone)]
pub struct ComplexityConfig {
    /// Maximum query depth (nesting level) — default: 10
    pub max_depth:      usize,
    /// Maximum complexity score — default: 100
    pub max_complexity: usize,
    /// Maximum number of field aliases per query — default: 30
    pub max_aliases:    usize,
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

/// GraphQL query validation error types (depth, complexity, aliases).
#[derive(Debug, thiserror::Error, Clone)]
#[non_exhaustive]
pub enum ComplexityValidationError {
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
    pub const fn from_config(config: &ComplexityConfig) -> Self {
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
    /// Returns [`ComplexityValidationError::MalformedQuery`] if the query cannot be parsed.
    pub fn analyze(&self, query: &str) -> Result<QueryMetrics, ComplexityValidationError> {
        if query.trim().is_empty() {
            return Err(ComplexityValidationError::MalformedQuery("Empty query".to_string()));
        }
        let document = graphql_parser::parse_query::<String>(query)
            .map_err(|e| ComplexityValidationError::MalformedQuery(format!("{e}")))?;
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
    /// Returns [`ComplexityValidationError`] if the query violates any validation rules.
    pub fn validate_query(&self, query: &str) -> Result<(), ComplexityValidationError> {
        if query.trim().is_empty() {
            return Err(ComplexityValidationError::MalformedQuery("Empty query".to_string()));
        }

        // Skip AST parsing only when depth, complexity, AND alias checks are all disabled.
        // The alias amplification check is a distinct DoS vector: it must run even when
        // depth and complexity validation are both turned off.
        if !self.validate_depth && !self.validate_complexity && self.max_aliases_per_query == 0 {
            return Ok(());
        }

        let document = graphql_parser::parse_query::<String>(query)
            .map_err(|e| ComplexityValidationError::MalformedQuery(format!("{e}")))?;
        let fragments = collect_fragments(&document);

        if self.validate_depth {
            let depth = self.calculate_depth_ast(&document, &fragments);
            if depth > self.max_depth {
                return Err(ComplexityValidationError::QueryTooDeep {
                    max_depth:    self.max_depth,
                    actual_depth: depth,
                });
            }
        }

        if self.validate_complexity {
            let complexity = self.calculate_complexity_ast(&document, &fragments);
            if complexity > self.max_complexity {
                return Err(ComplexityValidationError::QueryTooComplex {
                    max_complexity:    self.max_complexity,
                    actual_complexity: complexity,
                });
            }
        }

        let alias_count = self.count_aliases_ast(&document);
        if alias_count > self.max_aliases_per_query {
            return Err(ComplexityValidationError::TooManyAliases {
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
    /// Returns [`ComplexityValidationError`] if variables are not a JSON object or exceed
    /// [`MAX_VARIABLES_COUNT`].
    ///
    /// # Panics
    ///
    /// Cannot panic in practice — the `expect` on `as_object()` is guarded
    /// by a preceding `is_object()` check that returns `Err` first.
    pub fn validate_variables(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> Result<(), ComplexityValidationError> {
        if let Some(vars) = variables {
            if !vars.is_object() {
                return Err(ComplexityValidationError::InvalidVariables(
                    "Variables must be an object".to_string(),
                ));
            }
            // Safety: we just verified `vars` is an object above.
            let obj = vars.as_object().expect("checked above");
            if obj.len() > MAX_VARIABLES_COUNT {
                return Err(ComplexityValidationError::InvalidVariables(format!(
                    "Too many variables: {} exceeds maximum of {}",
                    obj.len(),
                    MAX_VARIABLES_COUNT
                )));
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
                Definition::Fragment(f) => self.selection_set_depth(&f.selection_set, fragments, 0),
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
                    if let Some(frag) = fragments.iter().find(|f| f.name == spread.fragment_name) {
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
                    if let Some(frag) = fragments.iter().find(|f| f.name == spread.fragment_name) {
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
fn collect_fragments<'a>(
    document: &'a Document<'a, String>,
) -> Vec<&'a FragmentDefinition<'a, String>> {
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
fn extract_limit_multiplier(arguments: &[(String, graphql_parser::query::Value<String>)]) -> usize {
    for (name, value) in arguments {
        if matches!(name.as_str(), "first" | "limit" | "take" | "last") {
            if let graphql_parser::query::Value::Int(n) = value {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]  // Reason: value is bounded; truncation cannot occur in practice
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
        validator
            .validate_query("{ user { id } }")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_depth_validation_fail() {
        let validator = RequestValidator::default().with_max_depth(3);
        let deep = "{ user { profile { settings { theme } } } }";
        let result = validator.validate_query(deep);
        assert!(
            matches!(result, Err(ComplexityValidationError::QueryTooDeep { .. })),
            "expected QueryTooDeep, got: {result:?}"
        );
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
        validator
            .validate_query(query)
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    // ── Complexity ──

    #[test]
    fn test_complexity_validation_pass() {
        let validator = RequestValidator::default().with_max_complexity(20);
        validator
            .validate_query("query { user { id name email } }")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
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
        validator
            .validate_query(query)
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_alias_count_exceeds_limit() {
        let validator = RequestValidator::new().with_max_aliases(2);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(
            matches!(
                validator.validate_query(query),
                Err(ComplexityValidationError::TooManyAliases {
                    actual_aliases: 3,
                    ..
                })
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
        validator
            .validate_query(&format!("query {{ {fields_30} }}"))
            .unwrap_or_else(|e| panic!("expected Ok for 30 aliases: {e}"));

        let fields_31: String = (0..31).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "f{i}: user {{ id }} ");
            s
        });
        let result_31 = validator.validate_query(&format!("query {{ {fields_31} }}"));
        assert!(
            matches!(result_31, Err(ComplexityValidationError::TooManyAliases { .. })),
            "expected TooManyAliases for 31 aliases, got: {result_31:?}"
        );
    }

    // ── Parse errors ──

    #[test]
    fn test_empty_query_rejected() {
        let validator = RequestValidator::new();
        let r1 = validator.validate_query("");
        assert!(
            matches!(r1, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery for empty string, got: {r1:?}"
        );
        let r2 = validator.validate_query("   ");
        assert!(
            matches!(r2, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery for whitespace, got: {r2:?}"
        );
    }

    #[test]
    fn test_malformed_query_rejected() {
        let validator = RequestValidator::new();
        let result = validator.validate_query("{ invalid query {{}}");
        assert!(
            matches!(result, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery, got: {result:?}"
        );
    }

    // ── Variables ──

    #[test]
    fn test_valid_variables() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!({"id": "123"});
        validator
            .validate_variables(Some(&vars))
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_invalid_variables_not_object() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!([1, 2, 3]);
        let result = validator.validate_variables(Some(&vars));
        assert!(
            matches!(result, Err(ComplexityValidationError::InvalidVariables(_))),
            "expected InvalidVariables, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_variables_too_many() {
        let validator = RequestValidator::new();
        // Build an object with MAX_VARIABLES_COUNT + 1 keys — must be rejected.
        let vars: serde_json::Value = serde_json::Value::Object(
            (0..=MAX_VARIABLES_COUNT)
                .map(|i| (format!("v{i}"), serde_json::Value::Null))
                .collect(),
        );
        let result = validator.validate_variables(Some(&vars));
        assert!(
            matches!(result, Err(ComplexityValidationError::InvalidVariables(_))),
            "expected InvalidVariables for too-many-variables, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_variables_at_limit_is_ok() {
        let validator = RequestValidator::new();
        // Exactly MAX_VARIABLES_COUNT keys — must be accepted.
        let vars: serde_json::Value = serde_json::Value::Object(
            (0..MAX_VARIABLES_COUNT)
                .map(|i| (format!("v{i}"), serde_json::Value::Null))
                .collect(),
        );
        validator
            .validate_variables(Some(&vars))
            .unwrap_or_else(|e| panic!("expected Ok at limit, got: {e}"));
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
        validator
            .validate_query(deep)
            .unwrap_or_else(|e| panic!("expected Ok when depth/complexity disabled: {e}"));
    }

    // ── Boundary / mutation-test sentinels ──
    //
    // Each test below is written to catch a specific surviving or predicted mutant.
    // Do not remove or weaken these tests without running `cargo mutants` first.

    // Guards: complexity > max (not >=, not ==)
    #[test]
    fn test_complexity_at_limit_is_allowed() {
        // { a b c } has complexity 3. max=3 must PASS (> not >=).
        let validator = RequestValidator::new().with_max_complexity(3);
        validator
            .validate_query("query { a b c }")
            .unwrap_or_else(|e| panic!("complexity == max must be allowed: {e}"));
    }

    #[test]
    fn test_complexity_just_over_limit_is_rejected() {
        // { a b c d } has complexity 4 > max=3, must FAIL.
        let validator = RequestValidator::new().with_max_complexity(3);
        assert!(
            matches!(
                validator.validate_query("query { a b c d }"),
                Err(ComplexityValidationError::QueryTooComplex { .. })
            ),
            "complexity > max must be rejected"
        );
    }

    // Guards: depth > max (not >=, not ==)
    #[test]
    fn test_depth_at_limit_is_allowed() {
        // { a { b { c } } } has depth 3. max_depth=3 must PASS (> not >=).
        let validator = RequestValidator::default().with_max_depth(3);
        validator
            .validate_query("{ a { b { c } } }")
            .unwrap_or_else(|e| panic!("depth == max must be allowed: {e}"));
    }

    #[test]
    fn test_depth_just_over_limit_is_rejected() {
        // { a { b { c { d } } } } has depth 4 > max=3, must FAIL.
        let validator = RequestValidator::default().with_max_depth(3);
        assert!(
            matches!(
                validator.validate_query("{ a { b { c { d } } } }"),
                Err(ComplexityValidationError::QueryTooDeep { .. })
            ),
            "depth > max must be rejected"
        );
    }

    // Guards: early-return condition is `&&` not `||`, and requires all three
    #[test]
    fn test_skip_validation_requires_aliases_also_zero() {
        // Depth and complexity disabled but aliases still active: alias check must still run.
        // Guards the `||` → `&&` mutation at the early-return gate.
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(2);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(
            validator.validate_query(query).is_err(),
            "alias check must run even when depth/complexity validation is disabled"
        );
    }

    #[test]
    fn test_early_return_requires_depth_disabled() {
        // Guards `delete !` on `!self.validate_depth`: when depth is still on, the early-return
        // must not fire even if complexity is off and aliases == 0.
        let validator = RequestValidator::new()
            .with_depth_validation(true)
            .with_complexity_validation(false)
            .with_max_aliases(0)
            .with_max_depth(2);
        assert!(
            matches!(
                validator.validate_query("{ a { b { c } } }"),
                Err(ComplexityValidationError::QueryTooDeep { .. })
            ),
            "depth validation must still run when only complexity is disabled"
        );
    }

    #[test]
    fn test_early_return_requires_complexity_disabled() {
        // Guards `delete !` on `!self.validate_complexity`: when complexity is still on,
        // the early-return must not fire even if depth is off and aliases == 0.
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(true)
            .with_max_aliases(0)
            .with_max_complexity(2);
        assert!(
            matches!(
                validator.validate_query("query { users(first: 100) { id name } }"),
                Err(ComplexityValidationError::QueryTooComplex { .. })
            ),
            "complexity validation must still run when only depth is disabled"
        );
    }

    // Guards: recursion guard `> 32` in fragment depth / complexity helpers
    #[test]
    fn test_deep_fragment_recursion_guard() {
        // A chain of 34 fragment spreads exceeds the recursion guard (> 32).
        // The guard must return max_depth+1 (not max_depth or max_depth-1),
        // ensuring the query is rejected rather than silently allowed.
        let validator = RequestValidator::new().with_max_depth(5);
        let mut query = String::from("query { ...F0 }\n");
        for i in 0..34_usize {
            use std::fmt::Write;
            let _ = writeln!(query, "fragment F{i} on T {{ ...F{} }}", i + 1);
        }
        query.push_str("fragment F34 on T { id }\n");
        assert!(
            validator.validate_query(&query).is_err(),
            "deeply nested fragment chain must be rejected by recursion guard"
        );
    }

    // Guards: alias `+` not `-` in count_aliases_in_selection_set
    #[test]
    fn test_nested_aliases_counted_correctly() {
        // Aliases nested inside another field's selection set must be summed, not subtracted.
        // { a: user { id } b: user { c: name d: email } } has 4 aliases total.
        let validator = RequestValidator::new().with_max_aliases(3);
        assert!(
            matches!(
                validator.validate_query(
                    "query { a: user { id } b: user { c: name d: email } }"
                ),
                Err(ComplexityValidationError::TooManyAliases {
                    actual_aliases: 4,
                    ..
                })
            ),
            "nested aliases must be summed, not subtracted"
        );
    }

    // ── from_config ──

    #[test]
    fn test_from_config() {
        let config = ComplexityConfig {
            max_depth:      5,
            max_complexity: 20,
            max_aliases:    3,
        };
        let validator = RequestValidator::from_config(&config);
        // Depth-6 query should fail
        let result = validator.validate_query("{ a { b { c { d { e { f } } } } } }");
        assert!(
            matches!(result, Err(ComplexityValidationError::QueryTooDeep { .. })),
            "expected QueryTooDeep for depth-6 query with max 5, got: {result:?}"
        );
    }
}
