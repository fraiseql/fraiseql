//! Query pattern matching - matches incoming GraphQL queries to compiled templates.

use std::collections::HashMap;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, ParsedQuery, parse_query_with_operation_name, selection_set},
    runtime::{argument_validation, argument_value_validation},
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

    /// The **server's own** row-scoping predicate, in the same
    /// `{field: {op: value}}` shape a client `where` uses, or `None` when the
    /// caller is a plain GraphQL document.
    ///
    /// Kept apart from `arguments["where"]` because the two are not the same
    /// kind of thing and must not share a fate (#1170). REST resource embedding
    /// used to build its parent-scoping join predicate into `arguments["where"]`
    /// and dispatch it through the ordinary read path — which composes that
    /// argument only when the target query's `auto_params.has_where` is set.
    /// `has_where` governs the **client-facing filter surface**; a project that
    /// turns it off is saying "clients may not filter this", not "and relations
    /// that embed it may go unscoped". Turning it off silently discarded the
    /// predicate and reported the target's *entire* result set as the relation's
    /// contents — every row under every parent, and, on the `ManyToOne` branch,
    /// the wrong parent attributed outright.
    ///
    /// Composed unconditionally alongside RLS and `inject_params`, which is
    /// where server-side scoping belongs, and never read from client input: a
    /// GraphQL document has no syntax that reaches this field.
    pub scope_where: Option<serde_json::Value>,

    /// The parsed query (for access to fragments, variables, etc.).
    pub parsed_query: ParsedQuery,
}

impl QueryMatch {
    /// The key this query's result appears under in `data`.
    ///
    /// The document's alias when it supplies one, otherwise the field name as
    /// written. The envelope used to be keyed by the *compiled* query
    /// definition's name, so `{ a: users { id } }` answered under `users` and two
    /// aliased selections of one query collapsed into a single key.
    ///
    /// Falls back to the definition name when there is no selection to read —
    /// the REST transport builds a `QueryMatch` with no GraphQL document behind
    /// it.
    #[must_use]
    pub fn response_key(&self) -> &str {
        self.selections
            .first()
            .map_or(self.query_def.name.as_str(), FieldSelection::response_key)
    }

    /// Build a `QueryMatch` directly from a query definition and arguments,
    /// bypassing GraphQL string parsing.
    ///
    /// Used by the REST transport to construct sub-queries for resource embedding
    /// and bulk operations without synthesising a GraphQL query string.
    ///
    /// The result has the **same shape** as [`QueryMatcher::match_query`]'s: a single
    /// root selection named for the query, carrying the requested fields as its
    /// `nested_fields`. Every consumer of a `QueryMatch` reads the requested field set
    /// as `selections.first().nested_fields` — the planner's projection extraction, the
    /// `#423` field-authorization gate, and `project_nested_lists` all do — so a *flat*
    /// list of leaf selections reads to all of them as "no fields requested".
    ///
    /// That was `#886`: this constructor built the flat shape, so every REST read
    /// projected zero fields (`{"data":[{},{},{}]}`) and the field-authorization gate
    /// was handed an empty slice and never fired. The two defects masked each other —
    /// no gated value leaked only because no value was served at all.
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
        let leaf = |name: &String| FieldSelection {
            name:          name.clone(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: Vec::new(),
            directives:    Vec::new(),
        };
        let selections = vec![FieldSelection {
            name:          query_def.name.clone(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: fields.iter().map(leaf).collect(),
            directives:    Vec::new(),
        }];

        let parsed_query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: Some(query_def.name.clone()),
            root_field:     query_def.name.clone(),
            selections:     Vec::new(),
            variables:      Vec::new(),
            fragments:      Vec::new(),
            source:         std::sync::Arc::from(""),
        };

        Ok(Self {
            query_def,
            fields,
            selections,
            arguments,
            operation_name: None,
            scope_where: None,
            parsed_query,
        })
    }

    /// Attach the server's own scoping predicate to a `QueryMatch` built by a
    /// non-GraphQL transport.
    ///
    /// See [`scope_where`](Self::scope_where) for why this is not simply another
    /// entry in `arguments`.
    #[must_use]
    pub fn with_scope_where(mut self, scope_where: serde_json::Value) -> Self {
        self.scope_where = Some(scope_where);
        self
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
        self.match_query_with_operation_name(query, variables, None)
    }

    /// Match a document against the compiled schema, selecting the operation
    /// named by `operation_name` (GraphQL § 6.1).
    ///
    /// [`match_query`](Self::match_query) is this with `None`, which requires
    /// the document to define exactly one operation.
    ///
    /// The executor threads the request's name here as well as into
    /// classification: this function re-parses the document, so leaving it to
    /// take the first operation would classify one operation and then match a
    /// different one.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Parse`] if the document does not parse, names
    /// no such operation, or is ambiguous; [`FraiseQLError::NotFound`] if the
    /// root field matches no compiled query.
    pub fn match_query_with_operation_name(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        operation_name: Option<&str>,
    ) -> Result<QueryMatch> {
        // 1. Parse GraphQL query using proper parser
        let parsed = parse_query_with_operation_name(query, operation_name)
            .map_err(|e| crate::graphql::operation_selection_error(&e))?;

        // 2. Build the variables map once. The same map is used for `@skip`/`@include` directive
        //    evaluation (by reference) and then moved onto the returned `QueryMatch` as
        //    `arguments`, so we never pay for a second clone of the JSON tree.
        let variables_map = Self::variables_to_map(variables);

        // 3. Reduce the document to the fields the client asked for: expand fragment spreads, then
        //    evaluate `@skip`/`@include`. Shared with the multi-root fan-out, `node(id:)` and
        //    mutations so no entry point can answer this question differently (#826, #827).
        let final_selections = selection_set::resolve_and_filter(
            &parsed.selections,
            &parsed.fragments,
            &variables_map,
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

        // 5b. #939: the selected fields must exist on the type being queried
        //     (GraphQL § 5.3.1). An undeclared name used to be lowered into the
        //     projection as `data->>'phantom_field'`, which is SQL NULL and
        //     serialises as a legitimate-looking `null` under a 200 — a client
        //     typo shipped silently. Spreads are already expanded above, so a
        //     field contributed by one validates like any other.
        //
        //     Relay connections are exempt: `query_def.return_type` is the *node*
        //     type, while the selection set is scoped to the generated
        //     `XxxConnection` (`edges`/`pageInfo`/`totalCount`). Validating the
        //     one against the other would reject every relay query.
        //     Count siblings (#938) are exempt for the same reason in the other
        //     direction: `return_type` is the entity type, but the field is a
        //     scalar `Int!` with no sub-selection at all, so there is nothing to
        //     validate against it.
        if !query_def.relay && !query_def.returns_count {
            if let Some(root) = final_selections.first() {
                crate::graphql::validate_selection_set(
                    &self.schema,
                    &query_def.return_type,
                    &root.nested_fields,
                )?;
            }
        }

        // 5c. #1154: the arguments written on the root field must be defined on it
        //     (GraphQL § 5.4.1). An undeclared argument used to be dropped —
        //     only declared arguments become WHERE conditions and only the
        //     auto-wired names reach the pagination paths — so a client filter
        //     the schema no longer carries returned the *unfiltered* set under a
        //     200. Unlike 5b this applies to relay and count queries too: their
        //     argument surfaces differ from a plain list query's, and
        //     `accepted_argument_names` is what encodes each one.
        //
        //     5d. #1197: and the *value* written against each of those names must
        //     have that name's type (GraphQL § 5.6.1, § 5.8.5), as must every
        //     supplied variable against its own declaration (§ 6.1.2). The same
        //     failure mode one level down: `limit: "2"` was read with `as_u64()`,
        //     answered `None`, and `None` is indistinguishable from "the client
        //     did not paginate" — so a request for two rows returned the table.
        //     Argument *types* come from `graphql_arguments`, which is where the
        //     auto-wired `limit`/`offset` acquire theirs; `accepted_argument_names`
        //     above is deliberately wider and carries no types.
        if let Some(root) = final_selections.first() {
            let field_label = format!("Query.{}", self.schema.display_name(&query_def.name));
            argument_validation::validate_argument_names(
                &field_label,
                &query_def.accepted_argument_names(&self.schema),
                &root.arguments,
            )?;
            argument_value_validation::validate_argument_values(
                &field_label,
                &query_def.graphql_arguments(&self.schema),
                &root.arguments,
                &parsed.variables,
            )?;
        }
        argument_value_validation::validate_variable_values(
            parsed.operation_name.as_deref(),
            &parsed.variables,
            variables,
        )?;

        // 6. Extract field names for backward compatibility
        let fields = self.extract_field_names(&final_selections);

        // 7. Take ownership of the variables map for `QueryMatch.arguments`. `variables_map` was
        //    built once at step 2 and only borrowed by the directive evaluator; no additional clone
        //    needed.
        let mut arguments = variables_map;

        // 8. Merge inline arguments from root field selection (e.g., `posts(limit: 3)`). Variables
        //    take precedence over inline arguments when both are provided.
        if let Some(root) = final_selections.first() {
            for arg in &root.arguments {
                if !arguments.contains_key(&arg.name) {
                    if let Some(val) = Self::resolve_inline_arg(arg, &arguments)? {
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
            // A GraphQL document cannot carry one: server scoping is attached by
            // the transport that owns it, never parsed from client input.
            scope_where: None,
            parsed_query: parsed,
        })
    }

    /// Convert the optional `variables` JSON object into an owned
    /// `HashMap<String, Value>` suitable for both `@skip`/`@include` directive
    /// evaluation and the public `QueryMatch::arguments` field.
    ///
    /// This used to be two separate helpers (`build_variables_map` and
    /// `extract_arguments`) that walked the same JSON object and cloned every
    /// key-value pair twice per request. Folding them into a single conversion
    /// halves the per-request allocation cost for variable-heavy mutations
    /// (see F005, F024 in `docs/history/IMPROVEMENTS.md`).
    fn variables_to_map(
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        selection_set::variables_map(variables)
    }

    /// Extract field names from selections (for backward compatibility).
    fn extract_field_names(&self, selections: &[FieldSelection]) -> Vec<String> {
        selections.iter().map(|s| s.name.clone()).collect()
    }

    /// Build the variables map exposed on [`QueryMatch::arguments`] from the
    /// raw GraphQL `variables` JSON payload.
    ///
    /// This is the public entry point used by tests; internally the
    /// `match_query` hot path now constructs the same map exactly once
    /// via a private helper.
    #[must_use]
    pub fn extract_arguments(
        variables: Option<&serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        Self::variables_to_map(variables)
    }

    /// Resolve an inline GraphQL argument to a JSON value.
    ///
    /// Handles both literal values (`limit: 3`) and variable references
    /// (`limit: $limit`), at the top level and nested inside object or list
    /// literals (`where: { field: { eq: $var } }`). The encoding is owned by
    /// [`crate::graphql::value_json`].
    ///
    /// `Ok(None)` means the argument names a variable the request did not
    /// supply, which drops it — GraphQL's treatment of an omitted nullable
    /// argument. A *malformed* stored value is an error, not a drop: dropping a
    /// `where:` argument widens the result set instead of narrowing it (#719).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if `value_json` is not valid JSON.
    pub(crate) fn resolve_inline_arg(
        arg: &crate::graphql::GraphQLArgument,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        let parsed = crate::graphql::value_json::decode(&arg.value_json)?;
        // A whole-argument variable that the request omitted drops the argument
        // rather than binding null, so `limit: $limit` with no `limit` variable
        // falls back to the query's default instead of forcing `LIMIT NULL`.
        if let Some(var_name) = crate::graphql::value_json::variable_name(&parsed) {
            return Ok(variables.get(var_name).cloned());
        }
        Ok(Some(crate::graphql::value_json::resolve_variables(parsed, variables)))
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }
}

// Re-exported so `crate::runtime::suggest_similar` keeps its call sites while the one
// implementation lives in `fraiseql-db` — the WHERE parser needs it too, and `fraiseql-core`
// depends on `fraiseql-db`, not the reverse.
pub use fraiseql_db::utils::suggest_similar;
