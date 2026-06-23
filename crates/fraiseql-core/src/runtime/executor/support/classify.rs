//! Query classification — determines operation type for routing.

use super::super::{Executor, QueryType};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    graphql::parse_query,
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Classify a GraphQL query into its operation type for routing.
    ///
    /// This is the first phase of query execution. It determines which handler
    /// to invoke based on the query structure and conventions:
    ///
    /// - **Introspection** (`__schema`, `__type`) → Uses pre-built responses (zero-cost)
    /// - **Federation** (`_service`, `_entities`) → Fed-specific logic
    /// - **Relay node** (`node(id: "...")`) → Global ID lookup
    /// - **Mutations** (`mutation { ... }`) → Write operations
    /// - **Aggregates** (root field ends with `_aggregate`) → Analytics queries
    /// - **Windows** (root field ends with `_window`) → Time-series queries
    /// - **Regular** (default) → Standard field selections
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Parse` if the query string is malformed GraphQL.
    ///
    /// # Example
    ///
    /// ```text
    /// // Illustrative — classify_query() is internal.
    /// // Use executor.execute(query, None).await? for the public API.
    ///
    /// // Regular query → Regular
    /// // Mutation       → Mutation  → execute_mutation_query()
    /// // __schema       → Introspection
    /// // _entities      → Federation
    /// ```
    pub(in crate::runtime::executor) fn classify_query(&self, query: &str) -> Result<QueryType> {
        self.classify_query_with_parse(query).map(|(qt, _)| qt)
    }

    /// Classify a query and simultaneously return the parsed AST for `Regular`
    /// queries, avoiding a redundant parse in the multi-root pipeline path.
    ///
    /// Returns `(QueryType, Some(ParsedQuery))` for `Regular` queries and
    /// `(QueryType, None)` for all other types (introspection, federation, etc.).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Parse`] if the query string is malformed GraphQL.
    pub(in crate::runtime::executor) fn classify_query_with_parse(
        &self,
        query: &str,
    ) -> Result<(QueryType, Option<crate::graphql::ParsedQuery>)> {
        // Parse the query once; the AST is the canonical source of truth.
        // Substring scans on the raw string produce false-positives on aliases,
        // comments, and string argument values (e.g. `{ search(q: "_service") }`
        // would be mis-routed as a federation query by a text scan).
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        let root_field = &parsed.root_field;

        // Introspection (highest priority): `__schema` or `__type`.
        // These are meta-fields defined by the GraphQL spec — always a root query.
        if root_field == "__schema" {
            return Ok((QueryType::IntrospectionSchema, None));
        }
        if root_field == "__type" {
            let type_name = extract_root_string_arg(&parsed, "name");
            return Ok((QueryType::IntrospectionType(type_name.unwrap_or_default()), None));
        }

        // Root `__typename` meta-field (GraphQL spec §"Type Name Introspection"):
        // a single-root selection consisting solely of `__typename` (optionally
        // aliased) resolves to the operation's root type name without a DB
        // round-trip. Placed before the mutation branch so `mutation { __typename }`
        // and `subscription { __typename }` resolve correctly instead of being
        // routed as a (missing) mutation field. The `len() == 1` guard is
        // load-bearing: mixed roots like `{ __typename users { id } }` fall through
        // to `Regular` and are resolved by the multi-root pipeline.
        if root_field == "__typename" && parsed.selections.len() == 1 {
            return Ok((
                QueryType::TypeName {
                    selection:      Box::new(parsed.selections[0].clone()),
                    operation_type: parsed.operation_type.clone(),
                },
                None,
            ));
        }

        // Federation (Apollo Federation v1/v2 entry-points).
        if root_field == "_service" || root_field == "_entities" {
            return Ok((QueryType::Federation(root_field.clone()), None));
        }

        // Relay global node lookup: root field is exactly "node" on a query.
        // Extract selections from inline fragments (... on TypeName { fields })
        // so the execution layer can project only requested fields.
        if parsed.operation_type == "query" && root_field == "node" {
            let selections = parsed
                .selections
                .first()
                .map(|node_sel| {
                    // Flatten inline fragments: `node { ... on Booking { id startDate } }`
                    // Inline fragments are represented as FieldSelection with name "...on TypeName"
                    let mut fields = Vec::new();
                    for sel in &node_sel.nested_fields {
                        if sel.name.starts_with("...") {
                            // Inline fragment — lift its nested_fields up
                            fields.extend(sel.nested_fields.clone());
                        } else {
                            fields.push(sel.clone());
                        }
                    }
                    fields
                })
                .unwrap_or_default();
            return Ok((QueryType::NodeQuery { selections }, None));
        }

        // Mutations are routed by operation type. Carry the full result selection
        // set (inline fragments intact) so the projector can subset and recurse
        // exactly like the query path. Named fragment spreads are resolved here —
        // the same as the query matcher — using the document's fragment
        // definitions; `@skip`/`@include` directives are evaluated later in
        // `execute_mutation_impl`, where the request variables are available.
        if parsed.operation_type == "mutation" {
            let raw = parsed.selections.first().map_or(&[][..], |s| s.nested_fields.as_slice());
            let resolver = crate::graphql::FragmentResolver::new(&parsed.fragments);
            let selections =
                resolver.resolve_spreads(raw).map_err(|e| FraiseQLError::Validation {
                    message: e.to_string(),
                    path:    Some("fragments".to_string()),
                })?;
            // Carry the root field's inline arguments (e.g. `input: { ... }`) so
            // the mutation runner can resolve inline-literal inputs with nested
            // `$var` references against the request variables.
            let arguments =
                parsed.selections.first().map(|s| s.arguments.clone()).unwrap_or_default();
            return Ok((
                QueryType::Mutation {
                    name: root_field.clone(),
                    selections,
                    arguments,
                },
                None,
            ));
        }

        // Aggregate queries (root field ends with `_aggregate`).
        if root_field.ends_with("_aggregate") {
            return Ok((QueryType::Aggregate(root_field.clone()), None));
        }

        // Window queries (root field ends with `_window`).
        if root_field.ends_with("_window") {
            return Ok((QueryType::Window(root_field.clone()), None));
        }

        // Regular query — return the already-parsed AST to avoid re-parsing in
        // the multi-root pipeline path.
        Ok((QueryType::Regular, Some(parsed)))
    }
}

/// Extract the value of a named string argument from the first (root) field of
/// a parsed query.
///
/// For `{ __type(name: "User") { ... } }`, calling `extract_root_string_arg(parsed, "name")`
/// returns `Some("User".to_string())`.
///
/// Returns `None` if the argument is absent or is not a JSON string literal.
fn extract_root_string_arg(parsed: &crate::graphql::ParsedQuery, arg_name: &str) -> Option<String> {
    let root_field = parsed.selections.first()?;
    let arg = root_field.arguments.iter().find(|a| a.name == arg_name)?;

    // value_json is serialized as `"TypeName"` (with surrounding quotes) for
    // string values.  Strip the outer quotes to get the raw string.
    let json = &arg.value_json;
    if json.starts_with('"') && json.ends_with('"') && json.len() >= 2 {
        Some(json[1..json.len() - 1].replace("\\\"", "\""))
    } else {
        None
    }
}
