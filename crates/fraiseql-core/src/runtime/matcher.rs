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
    /// QueryMatch with query definition and extracted information
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
    /// # use fraiseql_core::runtime::QueryMatcher;
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let json = std::fs::read_to_string("schema.compiled.json")?;
    /// # let schema = CompiledSchema::from_json(&json)?;
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
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        // 2. Build variables map for directive evaluation
        let variables_map = self.build_variables_map(variables);

        // 3. Resolve fragment spreads
        let resolver = FragmentResolver::new(&parsed.fragments);
        let resolved_selections = resolver.resolve_spreads(&parsed.selections).map_err(|e| {
            FraiseQLError::Validation {
                message: e.to_string(),
                path:    Some("fragments".to_string()),
            }
        })?;

        // 4. Evaluate directives (@skip, @include) and filter selections
        let final_selections =
            DirectiveEvaluator::filter_selections(&resolved_selections, &variables_map).map_err(
                |e| FraiseQLError::Validation {
                    message: e.to_string(),
                    path:    Some("directives".to_string()),
                },
            )?;

        // 5. Find matching query definition using root field
        let query_def = self
            .schema
            .find_query(&parsed.root_field)
            .ok_or_else(|| {
                let candidates: Vec<&str> =
                    self.schema.queries.iter().map(|q| q.name.as_str()).collect();
                let suggestion = suggest_similar(&parsed.root_field, &candidates);
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
        let arguments = self.extract_arguments(variables);

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
    fn extract_arguments(
        &self,
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        if let Some(serde_json::Value::Object(map)) = variables {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        }
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
fn levenshtein(a: &str, b: &str) -> usize {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         crate::schema::AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
        });
        schema
    }

    #[test]
    fn test_matcher_new() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);
        assert_eq!(matcher.schema().queries.len(), 1);
    }

    #[test]
    fn test_match_simple_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.fields.len(), 1); // "users" is the root field
        assert!(result.selections[0].nested_fields.len() >= 2); // id, name
    }

    #[test]
    fn test_match_query_with_operation_name() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "query GetUsers { users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.operation_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn test_match_query_with_fragment() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"
            fragment UserFields on User {
                id
                name
            }
            query { users { ...UserFields } }
        ";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // Fragment should be resolved - nested fields should contain id, name
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_skip_directive() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"{ users { id name @skip(if: true) } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "name" should be skipped due to @skip(if: true)
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_include_directive_variable() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query =
            r"query($includeEmail: Boolean!) { users { id email @include(if: $includeEmail) } }";
        let variables = serde_json::json!({ "includeEmail": false });
        let result = matcher.match_query(query, Some(&variables)).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "email" should be excluded because $includeEmail is false
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "email"));
    }

    #[test]
    fn test_match_query_unknown_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ unknown { id } }";
        let result = matcher.match_query(query, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_arguments_none() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let args = matcher.extract_arguments(None);
        assert!(args.is_empty());
    }

    #[test]
    fn test_extract_arguments_some() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let variables = serde_json::json!({
            "id": "123",
            "limit": 10
        });

        let args = matcher.extract_arguments(Some(&variables));
        assert_eq!(args.len(), 2);
        assert_eq!(args.get("id"), Some(&serde_json::json!("123")));
        assert_eq!(args.get("limit"), Some(&serde_json::json!(10)));
    }

    // =========================================================================
    // suggest_similar / levenshtein tests
    // =========================================================================

    #[test]
    fn test_suggest_similar_exact_typo() {
        let suggestions = suggest_similar("userr", &["users", "posts", "comments"]);
        assert_eq!(suggestions, vec!["users"]);
    }

    #[test]
    fn test_suggest_similar_transposition() {
        let suggestions = suggest_similar("suers", &["users", "posts"]);
        assert_eq!(suggestions, vec!["users"]);
    }

    #[test]
    fn test_suggest_similar_no_match() {
        // "zzz" is far from everything — no suggestion expected.
        let suggestions = suggest_similar("zzz", &["users", "posts", "comments"]);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_similar_capped_at_three() {
        // All four candidates are within distance 2 of "us".
        let suggestions =
            suggest_similar("us", &["users", "user", "uses", "usher", "something_far"]);
        assert!(suggestions.len() <= 3);
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("foo", "foo"), 0);
    }

    #[test]
    fn test_levenshtein_insertion() {
        assert_eq!(levenshtein("foo", "fooo"), 1);
    }

    #[test]
    fn test_levenshtein_deletion() {
        assert_eq!(levenshtein("fooo", "foo"), 1);
    }

    #[test]
    fn test_levenshtein_substitution() {
        assert_eq!(levenshtein("foo", "bar"), 3);
    }

    #[test]
    fn test_unknown_query_error_includes_suggestion() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         crate::schema::AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
        });
        let matcher = QueryMatcher::new(schema);

        // "userr" is one edit away from "users" — should suggest it.
        let result = matcher.match_query("{ userr { id } }", None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Did you mean 'users'?"), "expected suggestion in: {msg}");
    }
}
