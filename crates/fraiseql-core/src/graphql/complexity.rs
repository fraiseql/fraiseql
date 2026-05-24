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

    /// Returns true when every configured validation knob is disabled and the
    /// expensive AST parse can be skipped entirely.
    ///
    /// A validator is "fully disabled" only when depth, complexity, AND alias
    /// checks are all off. The alias-amplification check is a distinct `DoS`
    /// vector — `max_aliases_per_query == 0` disables it, any other value
    /// keeps it active even when depth/complexity validation are turned off.
    #[must_use]
    pub const fn is_no_op(&self) -> bool {
        !self.validate_depth && !self.validate_complexity && self.max_aliases_per_query == 0
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

        if self.is_no_op() {
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
            // Reason: `is_object()` check on the line above guarantees this is an object;
            // `as_object()` cannot return None here.
            let obj = vars.as_object().expect("invariant: vars.is_object() checked above");
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
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: value is clamped to [1, 100] immediately after; truncation and sign loss
                // are safe
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
