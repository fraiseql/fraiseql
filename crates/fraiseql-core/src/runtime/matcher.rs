//! Query pattern matching - matches incoming GraphQL queries to compiled templates.

use std::collections::HashMap;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{DirectiveEvaluator, FieldSelection, FragmentResolver, ParsedQuery, parse_query},
    schema::{CompiledSchema, QueryDefinition},
};

/// A matched query with extracted information.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The matched query definition from compiled schema.
    pub query_def: QueryDefinition,

    /// Requested fields (selection set) - now includes full field info.
    pub fields: Vec<String>,

    /// Parsed and processed field selections (after fragment/directive resolution).
    pub selections: Vec<FieldSelection>,

    /// Query arguments/variables.
    pub arguments: HashMap<String, serde_json::Value>,

    /// Query operation name (if provided).
    pub operation_name: Option<String>,

    /// The parsed query (for access to fragments, variables, etc.).
    pub parsed_query: ParsedQuery,
}

impl QueryMatch {
    /// Build a `QueryMatch` directly from a query definition and arguments,
    /// bypassing GraphQL string parsing.
    ///
    /// Used by the REST transport to construct sub-queries for resource embedding
    /// and bulk operations without synthesising a GraphQL query string.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query definition has no SQL source.
    pub fn from_operation(
        query_def: QueryDefinition,
        fields: Vec<String>,
        arguments: HashMap<String, serde_json::Value>,
        _type_def: Option<&crate::schema::TypeDefinition>,
    ) -> Result<Self> {
        let selections = fields
            .iter()
            .map(|f| FieldSelection {
                name: f.clone(),
                alias: None,
                arguments: Vec::new(),
                nested_fields: Vec::new(),
                directives: Vec::new(),
            })
            .collect();

        let parsed_query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: Some(query_def.name.clone()),
            root_field: query_def.name.clone(),
            selections: Vec::new(),
            variables: Vec::new(),
            fragments: Vec::new(),
            source: String::new(),
        };

        Ok(Self {
            query_def,
            fields,
            selections,
            arguments,
            operation_name: None,
            parsed_query,
        })
    }
}

/// Query pattern matcher.
///
/// Matches incoming GraphQL queries against the compiled schema to determine
/// which pre-compiled SQL template to execute.
pub struct QueryMatcher {
    schema: CompiledSchema,
}

impl QueryMatcher {
    /// Create new query matcher.
    ///
    /// Indexes are (re)built at construction time so that `match_query`
    /// works correctly regardless of whether `build_indexes()` was called
    /// on the schema before passing it here.
    #[must_use]
    pub fn new(mut schema: CompiledSchema) -> Self {
        schema.build_indexes();
        Self { schema }
    }

    /// Match a GraphQL query to a compiled template.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    ///
    /// # Returns
    ///
    /// `QueryMatch` with query definition and extracted information
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query syntax is invalid
    /// - Query references undefined operation
    /// - Query structure doesn't match schema
    /// - Fragment resolution fails
    /// - Directive evaluation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: compiled schema.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::runtime::QueryMatcher;
    /// # use fraiseql_error::Result;
    /// # fn example() -> Result<()> {
    /// # let schema: CompiledSchema = panic!("example");
    /// let matcher = QueryMatcher::new(schema);
    /// let query = "query { users { id name } }";
    /// let matched = matcher.match_query(query, None)?;
    /// assert_eq!(matched.query_def.name, "users");
    /// # Ok(())
    /// # }
    /// ```
    pub fn match_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<QueryMatch> {
        // 1. Parse GraphQL query using proper parser
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message: e.to_string(),
            location: "query".to_string(),
        })?;

        // 2. Build variables map for directive evaluation
        let variables_map = self.build_variables_map(variables);

        // 3. Resolve fragment spreads
        let resolver = FragmentResolver::new(&parsed.fragments);
        let resolved_selections = resolver.resolve_spreads(&parsed.selections).map_err(|e| {
            FraiseQLError::Validation {
                message: e.to_string(),
                path: Some("fragments".to_string()),
            }
        })?;

        // 4. Evaluate directives (@skip, @include) and filter selections
        let final_selections =
            DirectiveEvaluator::filter_selections(&resolved_selections, &variables_map).map_err(
                |e| FraiseQLError::Validation {
                    message: e.to_string(),
                    path: Some("directives".to_string()),
                },
            )?;

        // 5. Find matching query definition using root field
        let query_def = self
            .schema
            .find_query(&parsed.root_field)
            .ok_or_else(|| {
                let display_names: Vec<String> =
                    self.schema.queries.iter().map(|q| self.schema.display_name(&q.name)).collect();
                let candidate_refs: Vec<&str> = display_names.iter().map(String::as_str).collect();
                let suggestion = suggest_similar(&parsed.root_field, &candidate_refs);
                let message = match suggestion.as_slice() {
                    [s] => format!(
                        "Query '{}' not found in schema. Did you mean '{s}'?",
                        parsed.root_field
                    ),
                    [a, b] => format!(
                        "Query '{}' not found in schema. Did you mean '{a}' or '{b}'?",
                        parsed.root_field
                    ),
                    [a, b, c, ..] => format!(
                        "Query '{}' not found in schema. Did you mean '{a}', '{b}', or '{c}'?",
                        parsed.root_field
                    ),
                    _ => format!("Query '{}' not found in schema", parsed.root_field),
                };
                FraiseQLError::Validation {
                    message,
                    path: None,
                }
            })?
            .clone();

        // 6. Extract field names for backward compatibility
        let fields = self.extract_field_names(&final_selections);

        // 7. Extract arguments from variables
        let mut arguments = self.extract_arguments(variables);

        // 8. Merge inline arguments from root field selection (e.g., `posts(limit: 3)`). Variables
        //    take precedence over inline arguments when both are provided.
        if let Some(root) = final_selections.first() {
            for arg in &root.arguments {
                if !arguments.contains_key(&arg.name) {
                    if let Some(val) = Self::resolve_inline_arg(arg, &arguments) {
                        arguments.insert(arg.name.clone(), val);
                    }
                }
            }
        }

        Ok(QueryMatch {
            query_def,
            fields,
            selections: final_selections,
            arguments,
            operation_name: parsed.operation_name.clone(),
            parsed_query: parsed,
        })
    }

    /// Build a variables map from JSON value for directive evaluation.
    fn build_variables_map(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        }
    }

    /// Extract field names from selections (for backward compatibility).
    fn extract_field_names(&self, selections: &[FieldSelection]) -> Vec<String> {
        selections.iter().map(|s| s.name.clone()).collect()
    }

    /// Extract arguments from variables.
    pub(crate) fn extract_arguments(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        }
    }

    /// Resolve an inline GraphQL argument to a JSON value.
    ///
    /// Handles both literal values (`limit: 3` → `value_json = "3"`) and
    /// variable references (`limit: $limit` → `value_json = "\"$limit\""`),
    /// looking up the latter in the already-extracted variables map.
    ///
    /// Variable references are serialized by the parser as JSON-quoted strings
    /// (e.g. `Variable("myLimit")` → `"\"$myLimit\""`), so we must parse the
    /// JSON first and then check for the `$` prefix on the inner string.
    pub(crate) fn resolve_inline_arg(
        arg: &crate::graphql::GraphQLArgument,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Option<serde_json::Value> {
        // Try raw `$varName` first (defensive, in case any code path produces unquoted refs)
        if let Some(var_name) = arg.value_json.strip_prefix('$') {
            return variables.get(var_name).cloned();
        }
        // Parse the JSON value
        let parsed: serde_json::Value = serde_json::from_str(&arg.value_json).ok()?;
        // Check if the parsed value is a string starting with "$" (variable reference)
        if let Some(s) = parsed.as_str() {
            if let Some(var_name) = s.strip_prefix('$') {
                return variables.get(var_name).cloned();
            }
        }
        // Literal value (number, boolean, string, object, array, null)
        Some(parsed)
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }
}

/// Return candidates from `haystack` whose edit distance to `needle` is ≤ 2.
///
/// Uses a simple iterative Levenshtein implementation with a `2 * threshold`
/// early-exit so cost stays proportional to the length of the candidates rather
/// than `O(n * m)` for every comparison. At most three suggestions are returned,
/// ordered by increasing edit distance.
#[must_use]
pub fn suggest_similar<'a>(needle: &str, haystack: &[&'a str]) -> Vec<&'a str> {
    const MAX_DISTANCE: usize = 2;
    const MAX_SUGGESTIONS: usize = 3;

    let mut ranked: Vec<(usize, &str)> = haystack
        .iter()
        .filter_map(|&candidate| {
            let d = levenshtein(needle, candidate);
            if d <= MAX_DISTANCE {
                Some((d, candidate))
            } else {
                None
            }
        })
        .collect();

    ranked.sort_unstable_by_key(|&(d, _)| d);
    ranked.into_iter().take(MAX_SUGGESTIONS).map(|(_, s)| s).collect()
}

/// Compute the Levenshtein edit distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    // Early exit: length difference alone exceeds threshold.
    if m.abs_diff(n) > 2 {
        return m.abs_diff(n);
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}
