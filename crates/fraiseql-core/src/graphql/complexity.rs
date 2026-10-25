// GraphQL query complexity analysis — AST-based to prevent DoS attacks.
//
// Walks the parsed document, correctly handling operation names, arguments,
// fragment spreads, and aliases (which a character-scan approach cannot
// distinguish from field names).

use std::collections::{HashMap, HashSet};

use crate::graphql::{
    document::{Document, Operation},
    types::{FieldSelection, FragmentDefinition, GraphQLArgument},
};

/// A parsed GraphQL document, shared by every stage that reads one request.
pub type SharedDocument = std::sync::Arc<Document>;

/// The outcome of parsing one source, as [`parse_graphql_document`] returns it.
pub type DocumentParse = Result<SharedDocument, ComplexityValidationError>;

/// Parse a GraphQL query source into a [`Document`].
///
/// This is the **only** entry point to the parser in the workspace. Every parse — the
/// validator, the analyzer, the subscription root, the REST `validate` endpoint, the
/// matcher — comes through here, so a guard on the parser is one guard, and the parse is
/// shared (below). `tools/check-graphql-parse-sites.sh` fails the build if a second call
/// site appears.
///
/// # Parsed once per request (#1562), and once per executor
///
/// Inside [`document_scope`], the first source parsed is kept, and a later call
/// with the **same bytes** returns that document instead of parsing again. A
/// request's stages — operation detection, the introspection policy, the
/// structural validator, the cost estimate, GATE-1, classification and matching —
/// each ask for the document by its source, so they share one parse without any
/// of them having to be handed it. Parsing is a function of the source alone, so
/// sharing it cannot change an answer, and a different source (a synthetic
/// single-root document, say) is simply parsed. Outside a scope every call parses.
///
/// A scope opened by [`DocumentCache::scope`] also reads and fills that cache, so a
/// document a read-heavy API sends on every request, and the single-root documents a
/// multi-root one is split into, are parsed once by the executor that owns it.
///
/// # Errors
///
/// Returns [`ComplexityValidationError::MalformedQuery`] when the source is not an
/// executable document (with the line and column of the first error), and when the
/// parser panics.
pub fn parse_graphql_document(query: &str) -> DocumentParse {
    let in_scope = REQUEST_DOCUMENT.try_with(|slot| {
        let scope = slot.borrow();
        let kept = scope.kept.as_ref().and_then(|kept| kept.parse_of(query));
        (kept, scope.documents.clone())
    });
    let documents = match in_scope {
        Ok((Some(parse), _)) => return parse,
        Ok((None, documents)) => documents,
        Err(_) => None,
    };
    let kept = if let Some(entry) = documents.as_ref().and_then(|d| d.get(query)) {
        Kept::Cached(entry)
    } else {
        let parse = parse_uncached(query).map(SharedDocument::new);
        match (&documents, parse) {
            (Some(documents), Ok(document)) => Kept::Cached(documents.insert(query, document)),
            (_, parse) => Kept::Parsed(query.into(), parse),
        }
    };
    let parse = kept.parse();
    let _ = REQUEST_DOCUMENT.try_with(|slot| {
        let mut scope = slot.borrow_mut();
        if scope.kept.is_none() {
            scope.kept = Some(kept);
        }
    });
    parse
}

/// One request's view of the parser: the first document it parsed, and the
/// executor's [`DocumentCache`] when the scope was opened with one.
#[derive(Default)]
struct RequestScope {
    kept:      Option<Kept>,
    documents: Option<DocumentCache>,
}

/// The first document a request parsed, with the source it was parsed from.
enum Kept {
    /// The executor's cache entry, shared rather than copied.
    Cached(std::sync::Arc<(Box<str>, SharedDocument)>),
    /// A parse no cache holds: outside one, or a source that did not parse.
    Parsed(Box<str>, DocumentParse),
}

impl Kept {
    fn parse(&self) -> DocumentParse {
        match self {
            Self::Cached(entry) => Ok(entry.1.clone()),
            Self::Parsed(_, parse) => parse.clone(),
        }
    }

    fn parse_of(&self, query: &str) -> Option<DocumentParse> {
        let source = match self {
            Self::Cached(entry) => &entry.0,
            Self::Parsed(source, _) => source,
        };
        (**source == *query).then(|| self.parse())
    }
}

tokio::task_local! {
    /// The current [`document_scope`].
    static REQUEST_DOCUMENT: std::cell::RefCell<RequestScope>;
}

/// Run `future` as one request: the first document it parses is parsed once and
/// shared by every later [`parse_graphql_document`] call on the same source.
///
/// A transport opens this around everything it does with one request's document,
/// and the executor opens it around each execution, so the engine's own stages
/// share a parse on every transport. Inside an open scope this joins it rather
/// than starting an empty one, which would parse the document a second time.
/// Nothing outlives the outermost future: the document is dropped with it.
pub async fn document_scope<F: std::future::Future>(future: F) -> F::Output {
    scope_with(None, future).await
}

async fn scope_with<F: std::future::Future>(
    documents: Option<DocumentCache>,
    future: F,
) -> F::Output {
    let joined = REQUEST_DOCUMENT.try_with(|slot| {
        let mut scope = slot.borrow_mut();
        if scope.documents.is_none() {
            scope.documents.clone_from(&documents);
        }
    });
    if joined.is_ok() {
        future.await
    } else {
        let scope = RequestScope {
            kept: None,
            documents,
        };
        REQUEST_DOCUMENT.scope(std::cell::RefCell::new(scope), future).await
    }
}

/// The documents an executor has parsed, by source, shared by its requests.
///
/// Parsing is a function of the source alone, so a kept document answers exactly as
/// a fresh parse would. A hit is checked against the source it was parsed from (the
/// key is a 64-bit hash), only documents that parsed are kept, and the least recently
/// used is evicted past the capacity, so a client sending distinct documents costs
/// a parse each, as it would without the cache.
#[derive(Clone)]
pub struct DocumentCache(
    pub(super) moka::sync::Cache<u64, std::sync::Arc<(Box<str>, SharedDocument)>>,
);

impl DocumentCache {
    /// A cache holding at most `capacity` documents.
    #[must_use]
    pub fn new(capacity: u64) -> Self {
        Self(moka::sync::Cache::new(capacity))
    }

    /// The key a source is kept under.
    pub(super) fn key(source: &str) -> u64 {
        xxhash_rust::xxh3::xxh3_64(source.as_bytes())
    }

    fn get(&self, source: &str) -> Option<std::sync::Arc<(Box<str>, SharedDocument)>> {
        let entry = self.0.get(&Self::key(source))?;
        (*entry.0 == *source).then_some(entry)
    }

    fn insert(
        &self,
        source: &str,
        document: SharedDocument,
    ) -> std::sync::Arc<(Box<str>, SharedDocument)> {
        let entry = std::sync::Arc::new((source.into(), document));
        self.0.insert(Self::key(source), entry.clone());
        entry
    }

    /// Run `future` as one request (see [`document_scope`]) whose parses read and fill
    /// this cache. Inside an open scope this joins it, and the scope reads this cache
    /// from then on if it was opened without one.
    pub async fn scope<F: std::future::Future>(&self, future: F) -> F::Output {
        scope_with(Some(self.clone()), future).await
    }
}

impl std::fmt::Debug for DocumentCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentCache").field("entries", &self.0.entry_count()).finish()
    }
}

/// The parse itself, behind the guard [`parse_graphql_document`] documents.
fn parse_uncached(query: &str) -> Result<Document, ComplexityValidationError> {
    #[cfg(any(test, feature = "test-utils"))]
    PARSES_ON_THIS_THREAD.with(|n| n.set(n.get() + 1));
    if query.trim().is_empty() {
        return Err(ComplexityValidationError::MalformedQuery("Empty query".to_string()));
    }
    // Defence in depth. The parser is error-resilient by design and a fuzz campaign found
    // no input that panics it (see `document`), but a parser panic on a client's document
    // must cost that client a refused query, never the connection's other responses.
    // libFuzzer's panic hook aborts before unwinding, so this hides nothing from the fuzz
    // target, which calls the parser directly.
    match std::panic::catch_unwind(|| crate::graphql::document::parse(query)) {
        Ok(result) => result.map_err(|e| ComplexityValidationError::MalformedQuery(e.to_string())),
        Err(_) => Err(ComplexityValidationError::MalformedQuery(
            "the GraphQL parser failed on this document".to_string(),
        )),
    }
}

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static PARSES_ON_THIS_THREAD: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many documents the parser has actually parsed on the calling thread.
///
/// Test-only (#1562): every parse in the workspace goes through
/// [`parse_graphql_document`], and a document shared from [`document_scope`] is
/// not counted, so a request driven on a current-thread runtime can assert how
/// many times its document was parsed by reading this before and after.
#[cfg(any(test, feature = "test-utils"))]
#[must_use]
pub fn document_parses_on_this_thread() -> usize {
    PARSES_ON_THIS_THREAD.with(std::cell::Cell::get)
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

/// A document's [`QueryMetrics`] under the depth and complexity limits it was scored by.
#[derive(Debug, Clone)]
pub(super) struct Scored {
    limits:  (usize, usize),
    metrics: QueryMetrics,
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
        self.analyze_with_variables(query, None)
    }

    /// Compute query metrics without enforcing any limits, resolving
    /// variable-valued pagination arguments (`first: $n`) against `variables`.
    ///
    /// With `variables = None` an unresolvable pagination variable scores at
    /// the fail-closed ceiling (#869); callers that have the request variables
    /// must pass them or legitimate variable-paginated queries are over-scored.
    ///
    /// # Errors
    ///
    /// Returns [`ComplexityValidationError::MalformedQuery`] if the query cannot be parsed.
    pub fn analyze_with_variables(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<QueryMetrics, ComplexityValidationError> {
        if query.trim().is_empty() {
            return Err(ComplexityValidationError::MalformedQuery("Empty query".to_string()));
        }
        let document = parse_graphql_document(query)?;
        Ok(self.document_metrics(&document, variables))
    }

    /// Compute depth, complexity, and alias count in a single memoizing pass.
    ///
    /// Resolving each fragment's contribution exactly once (and rejecting
    /// fragment cycles) keeps this linear in document size regardless of
    /// fragment-spread topology — without it, B-way branching across D chained
    /// spreads costs B^D walks and the validation step itself becomes the `DoS`
    /// (audit H4).
    ///
    /// The score is a function of the document and the two limits unless a
    /// variable-valued page decided part of it. A score no variable decided is kept on
    /// the document, so a later request sharing the document reads it instead of
    /// walking again.
    fn document_metrics<'a>(
        &self,
        document: &'a Document,
        variables: Option<&'a serde_json::Value>,
    ) -> QueryMetrics {
        let limits = (self.max_depth, self.max_complexity);
        if let Some(scored) = document.scored.get().filter(|scored| scored.limits == limits) {
            return scored.metrics.clone();
        }
        let fragments = collect_fragments(document);
        let mut analyzer = DocumentAnalyzer::new(
            document,
            &fragments,
            self.max_depth,
            self.max_complexity,
            variables,
        );
        let metrics = analyzer.analyze_document(document);
        if !analyzer.read_a_variable {
            let _ = document.scored.set(Scored {
                limits,
                metrics: metrics.clone(),
            });
        }
        metrics
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

        let document = parse_graphql_document(query)?;
        self.validate_query_doc(&document, None)
    }

    /// Validate an already-parsed GraphQL document, enforcing configured limits.
    ///
    /// This is the AST-only entry point used by the server's HTTP handler so the
    /// document is parsed exactly once per request (rather than once by the
    /// validator and once by the executor's matcher). See F001 in
    /// `docs/history/IMPROVEMENTS.md`.
    ///
    /// # Errors
    ///
    /// Returns [`ComplexityValidationError`] if the document violates depth,
    /// complexity, or alias-amplification limits.
    pub fn validate_query_doc<'a>(
        &self,
        document: &'a Document,
        variables: Option<&'a serde_json::Value>,
    ) -> Result<(), ComplexityValidationError> {
        if self.is_no_op() {
            return Ok(());
        }

        let metrics = self.document_metrics(document, variables);

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
    by_name:         HashMap<&'a str, &'a FragmentDefinition>,
    memo:            HashMap<&'a str, SelectionMetrics>,
    visiting:        HashSet<&'a str>,
    max_depth:       usize,
    max_complexity:  usize,
    /// Request variables for resolving variable-valued pagination arguments
    /// (#869). `None` means unavailable: a variable-valued `first`/`limit`/
    /// `take`/`last` then scores the clamp ceiling (fail closed), never 1.
    variables:       Option<&'a serde_json::Map<String, serde_json::Value>>,
    /// Integer defaults the document's operations declare for their variables: an
    /// omitted pagination variable takes its default (§ 6.4.1, #1504), so it scores
    /// that value. Two operations declaring the same name keep the larger default.
    defaults:        HashMap<&'a str, i64>,
    /// Whether a variable-valued page decided part of the score, which makes the score
    /// the request's rather than the document's.
    read_a_variable: bool,
}

impl<'a> DocumentAnalyzer<'a> {
    fn new(
        document: &'a Document,
        fragments: &[&'a FragmentDefinition],
        max_depth: usize,
        max_complexity: usize,
        variables: Option<&'a serde_json::Value>,
    ) -> Self {
        let mut defaults: HashMap<&'a str, i64> = HashMap::new();
        for op in &document.operations {
            for var in &op.variables {
                let default = var.default_value.as_deref().and_then(|json| {
                    crate::graphql::value_json::decode(json).ok().and_then(|v| v.as_i64())
                });
                if let Some(n) = default {
                    let slot = defaults.entry(var.name.as_str()).or_insert(n);
                    *slot = (*slot).max(n);
                }
            }
        }
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
            variables: variables.and_then(serde_json::Value::as_object),
            defaults,
            read_a_variable: false,
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
    fn analyze_document(&mut self, document: &'a Document) -> QueryMetrics {
        let mut depth = 0usize;
        let mut complexity = 0usize;
        let mut aliases = 0usize;
        for op in &document.operations {
            let m = self.selection_metrics(&op.selections, FRAGMENT_SPREAD_DEPTH_LIMIT);
            depth = depth.max(m.depth);
            complexity = complexity.saturating_add(m.complexity);
            aliases = aliases.saturating_add(m.aliases);
        }
        for f in &document.fragments {
            let m = self.resolve_fragment(f.name.as_str(), FRAGMENT_SPREAD_DEPTH_LIMIT);
            depth = depth.max(m.depth);
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
        let m = self.selection_metrics(&frag.selections, budget - 1);
        self.visiting.remove(name);
        self.memo.insert(name, m);
        m
    }

    fn selection_metrics(
        &mut self,
        selection_set: &'a [FieldSelection],
        budget: usize,
    ) -> SelectionMetrics {
        let mut max_child_depth = 0usize;
        let mut complexity = 0usize;
        let mut aliases = 0usize;
        for sel in selection_set {
            let child = match Kind::of(sel) {
                Kind::Field => {
                    let self_alias = usize::from(sel.alias.is_some());
                    if sel.nested_fields.is_empty() {
                        SelectionMetrics {
                            depth:      0,
                            complexity: 1,
                            aliases:    self_alias,
                        }
                    } else {
                        let nested = self.selection_metrics(&sel.nested_fields, budget);
                        let multiplier = self.limit_multiplier(&sel.arguments);
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
                Kind::InlineFragment => self.selection_metrics(&sel.nested_fields, budget),
                Kind::Spread(name) => self.resolve_fragment(name, budget),
            };
            max_child_depth = max_child_depth.max(child.depth);
            complexity = complexity.saturating_add(child.complexity);
            aliases = aliases.saturating_add(child.aliases);
        }
        let depth = if selection_set.is_empty() {
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

    /// The pagination multiplier of a field's arguments, noting whether a variable
    /// decided it.
    fn limit_multiplier(&mut self, arguments: &'a [GraphQLArgument]) -> usize {
        let (multiplier, from_a_variable) =
            extract_limit_multiplier(arguments, self.variables, &self.defaults);
        self.read_a_variable |= from_a_variable;
        multiplier
    }

    /// Complexity contribution of a single field — `1` for a leaf, else
    /// `1 + nested_complexity * pagination_multiplier`. This is exactly the
    /// per-field score [`selection_metrics`](Self::selection_metrics) assigns, so
    /// a cost walk with no `@cost` overrides equals the complexity score.
    fn field_complexity(&mut self, field: &'a FieldSelection, budget: usize) -> usize {
        if field.nested_fields.is_empty() {
            1
        } else {
            let nested = self.selection_metrics(&field.nested_fields, budget);
            let multiplier = self.limit_multiplier(&field.arguments);
            1usize.saturating_add(nested.complexity.saturating_mul(multiplier))
        }
    }

    /// Document cost for per-tenant budget enforcement.
    ///
    /// Equals the complexity score, except a top-level (root) operation field
    /// whose name carries an `@cost` weight contributes exactly that weight (its
    /// subtree is not walked). Root inline fragments/spreads fall back to the
    /// normal complexity contribution.
    fn document_cost<S: std::hash::BuildHasher>(
        &mut self,
        document: &'a Document,
        root_weights: &HashMap<String, usize, S>,
    ) -> usize {
        document.operations.iter().fold(0usize, |cost, op: &'a Operation| {
            cost.saturating_add(self.root_cost(&op.selections, root_weights))
        })
    }

    fn root_cost<S: std::hash::BuildHasher>(
        &mut self,
        selection_set: &'a [FieldSelection],
        root_weights: &HashMap<String, usize, S>,
    ) -> usize {
        let mut cost = 0usize;
        for sel in selection_set {
            let c = match Kind::of(sel) {
                Kind::Field => root_weights
                    .get(sel.name.as_str())
                    .copied()
                    .unwrap_or_else(|| self.field_complexity(sel, FRAGMENT_SPREAD_DEPTH_LIMIT)),
                Kind::InlineFragment => self.root_cost(&sel.nested_fields, root_weights),
                Kind::Spread(name) => {
                    self.resolve_fragment(name, FRAGMENT_SPREAD_DEPTH_LIMIT).complexity
                },
            };
            cost = cost.saturating_add(c);
        }
        cost
    }
}

/// What a selection is: [`document`](crate::graphql::document) writes a fragment spread as a
/// selection named `...<fragment>` and an inline fragment as one named `...on <type>`, and
/// no field name begins with `.`.
enum Kind<'a> {
    Field,
    InlineFragment,
    Spread(&'a str),
}

impl<'a> Kind<'a> {
    fn of(selection: &'a FieldSelection) -> Self {
        match selection.name.strip_prefix("...") {
            None => Self::Field,
            Some(rest) if rest.starts_with("on ") => Self::InlineFragment,
            Some(fragment) => Self::Spread(fragment),
        }
    }
}

/// Estimate a parsed document's cost for per-tenant budget enforcement.
///
/// The cost equals the AST complexity score (so it is consistent with the
/// `max_complexity` gate), except a top-level operation field whose name appears
/// in `root_cost_weights` contributes exactly that `@cost` weight instead of its
/// walked subtree. With an empty map the result equals
/// [`RequestValidator::analyze`]'s `complexity`.
#[must_use]
pub fn estimate_query_cost<'a, S: std::hash::BuildHasher>(
    document: &'a Document,
    root_cost_weights: &HashMap<String, usize, S>,
    variables: Option<&'a serde_json::Value>,
) -> usize {
    let fragments = collect_fragments(document);
    // usize::MAX sentinels so a fragment cycle yields a saturating (rejected) cost
    // rather than an artificial cap.
    let mut analyzer =
        DocumentAnalyzer::new(document, &fragments, usize::MAX, usize::MAX, variables);
    analyzer.document_cost(document, root_cost_weights)
}

/// A direct read's projection, in the shape the cost estimator scores it.
///
/// The leaf fields this level materialises, the page it materialises them over, and
/// every level composed **into the same statement** beneath it.
///
/// Every read this codebase issues today is [`flat`](Self::flat) — leaves, a page, no
/// nesting — and scores exactly what a bare field count scored before this type
/// existed. The nesting is here because the REST `?select=` fan-out is to be composed
/// into the parent statement with `LATERAL`, and a composed statement materialises its
/// embedded levels itself. Handed to a flat estimator it would be charged for the
/// parent's own columns alone, and `per_request_max` would loosen by whatever the embed
/// adds — silently, because the number the operator wrote would not have changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirectReadProjection {
    /// Scalar fields projected at this level.
    pub leaf_fields: usize,
    /// The page this level is read over. `None` is an unpaginated read and scores a
    /// multiplier of 1, for the reason given on [`estimate_direct_read_cost`].
    pub limit:       Option<u32>,
    /// The levels composed into the same statement beneath this one, each with its
    /// own page — which is the point of `LATERAL`: the embedded `ORDER BY`/`LIMIT`
    /// live inside the subquery, so a nested level's page is genuinely its own and
    /// not a share of the parent's.
    pub nested:      Vec<DirectReadProjection>,
}

impl DirectReadProjection {
    /// A projection with nothing composed beneath it.
    ///
    /// `leaf_fields` scalars over one page — the shape of every direct read today.
    #[must_use]
    pub const fn flat(leaf_fields: usize, limit: Option<u32>) -> Self {
        Self {
            leaf_fields,
            limit,
            nested: Vec::new(),
        }
    }

    /// This level's complexity, and its subtree's.
    ///
    /// `DocumentAnalyzer::field_complexity`'s arithmetic, reached through a projection
    /// rather than an AST: a level with nothing under it scores 1, and otherwise
    /// `1 + subtree × page`, where the subtree is this level's own scalars plus the
    /// score of each level composed beneath it.
    ///
    /// Sharing the arithmetic is the whole point — see [`estimate_direct_read_cost`].
    fn complexity(&self) -> usize {
        let inner = self
            .nested
            .iter()
            .fold(self.leaf_fields, |acc, level| acc.saturating_add(level.complexity()));
        if inner == 0 {
            return 1;
        }
        let multiplier =
            self.limit.map_or(1, |l| (l as usize).clamp(1, PAGINATION_MULTIPLIER_CEILING));
        1usize.saturating_add(inner.saturating_mul(multiplier))
    }
}

/// Estimate the cost of a **resolved direct read** — one that never had a GraphQL
/// document to parse (#1351).
///
/// `[security.cost_budget] per_request_max` was enforced only in `run_gate1`, which
/// takes a query string and scores its AST. Every read that does not carry a document
/// — the whole REST direct-read surface and both gRPC read arms — therefore reached
/// the database without the ceiling the operator compiled in. It was never a gRPC gap:
/// routing gRPC through the engine could not fix it, because the engine's own
/// direct-read path did not have it either.
///
/// # Why this mirrors the document scoring rather than inventing a formula
///
/// `per_request_max` is one number, written once by an operator who does not know
/// which transport a client will pick. Scoring a direct read on a different scale
/// would make the same cap refuse different requests depending on how they arrived —
/// which is the defect class this is fixing, one layer up. So the arithmetic is the
/// document path's, for the document a direct read *is*: a single root field over the
/// projection it materialises.
///
/// - A query whose name carries an `@cost` weight contributes exactly that weight, subtree unwalked
///   — as a root field does in `DocumentAnalyzer::root_cost`.
/// - Otherwise `1 + subtree × multiplier`, the case of `DocumentAnalyzer::field_complexity`, where
///   each projected scalar contributes 1 and each composed level contributes its own score.
/// - `multiplier` is the page size clamped to `[1, 100]`, as `extract_limit_multiplier` clamps
///   `first`/`limit`/`take`/`last`.
///
/// `limit: None` scores 1, which is what the document path scores for a list field
/// carrying no pagination argument. That is inherited permissiveness, not a decision
/// taken here: an unbounded list read is the most expensive one either path can issue,
/// and it scores lowest on both. `max_page_size` (#421) is what bounds it today.
///
/// # Why the projection is a tree
///
/// Because the `?select=` fan-out is to become one composed statement, and the score
/// has to survive that change without the ceiling moving. It does, exactly: for a
/// parent of `p` scalars over a page of `M` with one embed of `c` scalars over a page
/// of `m`, today's fan-out charges `1 + p·M` for the parent read and up to `M`
/// sub-reads of `1 + c·m`, which is `1 + M·(p + 1 + c·m)` — and that is the number
/// this function returns for the composed projection. The identity is not a
/// coincidence and it is not arranged: it is what `field_complexity` already computed
/// for the same shape, which is why the document path is the right formula to borrow
/// rather than a second one to invent.
///
/// `a_composed_embed_scores_what_its_fan_out_charges` is that identity as a test, and
/// `the_composed_score_is_the_document_paths_score` pins it against
/// [`estimate_query_cost`] itself rather than against a restatement of it.
#[must_use]
pub fn estimate_direct_read_cost<S: std::hash::BuildHasher>(
    query_name: &str,
    root_cost_weights: &HashMap<String, usize, S>,
    projection: &DirectReadProjection,
) -> usize {
    if let Some(weight) = root_cost_weights.get(query_name) {
        return *weight;
    }
    projection.complexity()
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
fn collect_fragments(document: &Document) -> Vec<&FragmentDefinition> {
    document.fragments.iter().collect()
}

/// Extract pagination limit from field arguments to use as a cost multiplier.
/// Upper clamp for the pagination multiplier — also the fail-closed score
/// for a variable-valued pagination argument that cannot be resolved
/// (#869): scoring it 1 is what let `first: $n` bypass `max_complexity`.
///
/// Module-level since #1351, because the direct-read estimator has to clamp the same
/// way the document estimator does: a cap the operator wrote once must mean the same
/// number whether or not the request arrived as a GraphQL document.
const PAGINATION_MULTIPLIER_CEILING: usize = 100;

/// The pagination multiplier of a field's arguments, and whether a variable decided it.
fn extract_limit_multiplier(
    arguments: &[GraphQLArgument],
    variables: Option<&serde_json::Map<String, serde_json::Value>>,
    defaults: &HashMap<&str, i64>,
) -> (usize, bool) {
    for argument in arguments {
        if !matches!(argument.name.as_str(), "first" | "limit" | "take" | "last") {
            continue;
        }
        let Ok(value) = crate::graphql::value_json::decode(&argument.value_json) else {
            continue;
        };
        if let Some(var) = crate::graphql::value_json::variable_name(&value) {
            // A supplied value wins, `null` included (it scores the ceiling);
            // an omitted variable takes its declared default.
            let supplied = variables.and_then(|vars| vars.get(var));
            let multiplier = supplied
                .map_or_else(|| defaults.get(var).copied(), serde_json::Value::as_i64)
                .map_or(PAGINATION_MULTIPLIER_CEILING, |n| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // Reason: value is clamped to [1, 100] immediately after; truncation
                    // and sign loss are safe
                    let limit = n as usize;
                    limit.clamp(1, PAGINATION_MULTIPLIER_CEILING)
                });
            return (multiplier, true);
        }
        if argument.value_type == "int" {
            if let Some(n) = value.as_i64() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // Reason: value is clamped to [1, 100] immediately after; truncation and
                // sign loss are safe
                let limit = n as usize;
                return (limit.clamp(1, PAGINATION_MULTIPLIER_CEILING), false);
            }
        }
    }
    (1, false)
}

#[cfg(test)]
#[path = "complexity_tests.rs"]
mod complexity_tests;
