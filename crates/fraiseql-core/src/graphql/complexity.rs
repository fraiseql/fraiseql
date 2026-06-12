// GraphQL query complexity analysis — AST-based to prevent DoS attacks.
//
// Uses `graphql-parser` to walk the document tree, correctly handling
// operation names, arguments, fragment spreads, and aliases (which a
// character-scan approach cannot distinguish from field names).

use std::collections::{HashMap, HashSet};

use graphql_parser::query::{
    Definition, Document, FragmentDefinition, OperationDefinition, Selection, SelectionSet,
};

/// Parse a GraphQL query source into a [`graphql_parser`] document.
///
/// Exposed so the server's HTTP handler can parse the request exactly once
/// and reuse the AST for both complexity validation (via
/// [`RequestValidator::validate_query_doc`]) and the downstream execution path
/// — replacing the previous "validate then re-parse" pattern. See F001 in
/// `IMPROVEMENTS.md`.
///
/// # Errors
///
/// Returns [`ComplexityValidationError::MalformedQuery`] when the source fails
/// to parse as a GraphQL query document.
pub fn parse_graphql_document(
    query: &str,
) -> Result<Document<'_, String>, ComplexityValidationError> {
    if query.trim().is_empty() {
        return Err(ComplexityValidationError::MalformedQuery("Empty query".to_string()));
    }
    graphql_parser::parse_query::<String>(query)
        .map_err(|e| ComplexityValidationError::MalformedQuery(format!("{e}")))
}

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
        Ok(self.document_metrics(&document))
    }

    /// Compute depth, complexity, and alias count in a single memoizing pass.
    ///
    /// Resolving each fragment's contribution exactly once (and rejecting
    /// fragment cycles) keeps this linear in document size regardless of
    /// fragment-spread topology — without it, B-way branching across D chained
    /// spreads costs B^D walks and the validation step itself becomes the `DoS`
    /// (audit H4).
    fn document_metrics<'a>(&self, document: &'a Document<'a, String>) -> QueryMetrics {
        let fragments = collect_fragments(document);
        let mut analyzer = DocumentAnalyzer::new(&fragments, self.max_depth, self.max_complexity);
        analyzer.analyze_document(document)
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
        self.validate_query_doc(&document)
    }

    /// Validate an already-parsed GraphQL document, enforcing configured limits.
    ///
    /// This is the AST-only entry point used by the server's HTTP handler so the
    /// document is parsed exactly once per request (rather than once by the
    /// validator and once by the executor's matcher). See F001 in
    /// `IMPROVEMENTS.md`.
    ///
    /// # Errors
    ///
    /// Returns [`ComplexityValidationError`] if the document violates depth,
    /// complexity, or alias-amplification limits.
    pub fn validate_query_doc<'a>(
        &self,
        document: &'a Document<'a, String>,
    ) -> Result<(), ComplexityValidationError> {
        if self.is_no_op() {
            return Ok(());
        }

        let metrics = self.document_metrics(document);

        if self.validate_depth && metrics.depth > self.max_depth {
            return Err(ComplexityValidationError::QueryTooDeep {
                max_depth:    self.max_depth,
                actual_depth: metrics.depth,
            });
        }

        if self.validate_complexity && metrics.complexity > self.max_complexity {
            return Err(ComplexityValidationError::QueryTooComplex {
                max_complexity:    self.max_complexity,
                actual_complexity: metrics.complexity,
            });
        }

        if metrics.alias_count > self.max_aliases_per_query {
            return Err(ComplexityValidationError::TooManyAliases {
                max_aliases:    self.max_aliases_per_query,
                actual_aliases: metrics.alias_count,
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
}

/// Hard ceiling on fragment-spread chain length the analyzer resolves before
/// declaring the document over-limit. Mirrors the pre-memoization 32-hop
/// recursion guard: a document nesting fragment spreads beyond this is rejected
/// (real APIs never approach it), which also bounds the analyzer's own
/// recursion depth so a long fragment chain cannot overflow the stack. Fragment
/// cycles are caught earlier by the [`DocumentAnalyzer`] `visiting` set.
const FRAGMENT_SPREAD_DEPTH_LIMIT: usize = 32;

/// Depth, complexity, and alias contribution of one selection set (or one
/// fragment). Computed once per fragment and memoized by name so a fragment
/// spread costs a map lookup instead of a re-walk.
#[derive(Debug, Clone, Copy)]
struct SelectionMetrics {
    depth:      usize,
    complexity: usize,
    aliases:    usize,
}

/// Single-pass, memoizing analyzer shared by depth, complexity, and alias
/// counting.
///
/// Each fragment's metrics are resolved exactly once (on demand) and cached by
/// name; fragment cycles are detected via `visiting` and treated as over-limit
/// rather than recursed into. This is what bounds the validator's own cost —
/// the previous per-spread re-walk made the validation step itself a `DoS`:
/// B-way branching across D chained fragment spreads cost B^D walks (audit H4),
/// and aliases hidden inside spread fragments were never counted, bypassing the
/// alias-amplification limit (audit H4, alias counter).
struct DocumentAnalyzer<'a> {
    by_name:        HashMap<&'a str, &'a FragmentDefinition<'a, String>>,
    memo:           HashMap<&'a str, SelectionMetrics>,
    visiting:       HashSet<&'a str>,
    max_depth:      usize,
    max_complexity: usize,
}

impl<'a> DocumentAnalyzer<'a> {
    fn new(
        fragments: &[&'a FragmentDefinition<'a, String>],
        max_depth: usize,
        max_complexity: usize,
    ) -> Self {
        let mut by_name = HashMap::with_capacity(fragments.len());
        for frag in fragments {
            // First definition wins, matching the previous `.find()` lookup.
            by_name.entry(frag.name.as_str()).or_insert(*frag);
        }
        Self {
            by_name,
            memo: HashMap::new(),
            visiting: HashSet::new(),
            max_depth,
            max_complexity,
        }
    }

    /// Metrics that force rejection on both the depth and complexity gates,
    /// used for fragment cycles and over-long spread chains (never executable).
    const fn over_limit(&self) -> SelectionMetrics {
        SelectionMetrics {
            depth:      self.max_depth.saturating_add(1),
            complexity: self.max_complexity.saturating_add(1),
            aliases:    0,
        }
    }

    /// Reduce the whole document to a single [`QueryMetrics`].
    ///
    /// Depth is the max across every definition (operations *and* fragment
    /// definitions, preserving legacy behaviour); complexity sums operations
    /// only; the alias total sums operations only, where each fragment spread
    /// contributes the fragment's own alias count *per occurrence* — that is
    /// the alias-amplification fix (the old counter scored spreads as 0).
    fn analyze_document(&mut self, document: &'a Document<'a, String>) -> QueryMetrics {
        let mut depth = 0usize;
        let mut complexity = 0usize;
        let mut aliases = 0usize;
        for def in &document.definitions {
            match def {
                Definition::Operation(op) => {
                    let m = self.selection_metrics(
                        operation_selection_set(op),
                        FRAGMENT_SPREAD_DEPTH_LIMIT,
                    );
                    depth = depth.max(m.depth);
                    complexity = complexity.saturating_add(m.complexity);
                    aliases = aliases.saturating_add(m.aliases);
                },
                Definition::Fragment(f) => {
                    let m = self.resolve_fragment(f.name.as_str(), FRAGMENT_SPREAD_DEPTH_LIMIT);
                    depth = depth.max(m.depth);
                },
            }
        }
        QueryMetrics {
            depth,
            complexity,
            alias_count: aliases,
        }
    }

    /// Memoized metrics for a fragment by name.
    ///
    /// `budget` is the remaining fragment-spread hops; it decrements on each
    /// spread resolution and a cycle (`visiting`) or exhausted budget yields the
    /// over-limit sentinel without recursing. Cut/cycle results are deliberately
    /// not memoized — they are context-dependent, not the fragment's intrinsic
    /// metric.
    fn resolve_fragment(&mut self, name: &'a str, budget: usize) -> SelectionMetrics {
        if let Some(m) = self.memo.get(name) {
            return *m;
        }
        if self.visiting.contains(name) || budget == 0 {
            return self.over_limit();
        }
        let Some(frag) = self.by_name.get(name).copied() else {
            // Unknown fragment: preserve the legacy contribution — depth =
            // max_depth (so the enclosing selection set trips the depth gate),
            // complexity = 10, no aliases.
            let m = SelectionMetrics {
                depth:      self.max_depth,
                complexity: 10,
                aliases:    0,
            };
            self.memo.insert(name, m);
            return m;
        };
        self.visiting.insert(name);
        let m = self.selection_metrics(&frag.selection_set, budget - 1);
        self.visiting.remove(name);
        self.memo.insert(name, m);
        m
    }

    fn selection_metrics(
        &mut self,
        selection_set: &'a SelectionSet<'a, String>,
        budget: usize,
    ) -> SelectionMetrics {
        let mut max_child_depth = 0usize;
        let mut complexity = 0usize;
        let mut aliases = 0usize;
        for sel in &selection_set.items {
            let child = match sel {
                Selection::Field(field) => {
                    let self_alias = usize::from(field.alias.is_some());
                    if field.selection_set.items.is_empty() {
                        SelectionMetrics {
                            depth:      0,
                            complexity: 1,
                            aliases:    self_alias,
                        }
                    } else {
                        let nested = self.selection_metrics(&field.selection_set, budget);
                        let multiplier = extract_limit_multiplier(&field.arguments);
                        SelectionMetrics {
                            depth:      nested.depth,
                            // Saturating: the multiplier compounds per nesting
                            // level, so a crafted deep query overflows `usize`.
                            // Saturating to `usize::MAX` keeps the score
                            // monotonic and fail-closed (the limit always
                            // rejects it) rather than wrapping under the limit.
                            complexity: 1usize
                                .saturating_add(nested.complexity.saturating_mul(multiplier)),
                            aliases:    self_alias.saturating_add(nested.aliases),
                        }
                    }
                },
                Selection::InlineFragment(inline) => {
                    self.selection_metrics(&inline.selection_set, budget)
                },
                Selection::FragmentSpread(spread) => {
                    self.resolve_fragment(spread.fragment_name.as_str(), budget)
                },
            };
            max_child_depth = max_child_depth.max(child.depth);
            complexity = complexity.saturating_add(child.complexity);
            aliases = aliases.saturating_add(child.aliases);
        }
        let depth = if selection_set.items.is_empty() {
            0
        } else {
            max_child_depth.saturating_add(1)
        };
        SelectionMetrics {
            depth,
            complexity,
            aliases,
        }
    }
}

/// The top-level selection set of any operation kind.
const fn operation_selection_set<'a>(
    op: &'a OperationDefinition<'a, String>,
) -> &'a SelectionSet<'a, String> {
    match op {
        OperationDefinition::Query(q) => &q.selection_set,
        OperationDefinition::Mutation(m) => &m.selection_set,
        OperationDefinition::Subscription(s) => &s.selection_set,
        OperationDefinition::SelectionSet(ss) => ss,
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
