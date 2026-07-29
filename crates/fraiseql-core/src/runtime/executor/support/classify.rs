//! Query classification — determines operation type for routing.

use super::super::{Executor, MutationRoot, QueryType};
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
        //
        // Named fragment spreads are expanded here, using the same routine as the
        // query matcher and the mutation branch below. This branch used to lift
        // `sel.nested_fields` out of every child whose name starts with `"..."`,
        // which is right for an inline fragment (whose nested fields hold the real
        // selections) and silently *dropped* a named spread, whose pseudo-field
        // always has an empty `nested_fields` (#827). Relay Modern issues
        // `node(id: $id) { id ...Container_data }` for every lookup, so that was
        // the canonical shape.
        //
        // `@skip`/`@include` are evaluated later in `execute_node_query`, where
        // the request variables are available — the same split as mutations, and
        // what keeps this classification cacheable by query string alone.
        if parsed.operation_type == "query" && root_field == "node" {
            let raw = parsed.selections.first().map_or(&[][..], |s| s.nested_fields.as_slice());
            let resolved = crate::graphql::selection_set::resolve(raw, &parsed.fragments)?;

            // Flatten inline fragments: `node { ... on Booking { id startDate } }`.
            // Only `"...on "` entries carry their selections in `nested_fields`;
            // after expansion no bare `"...Name"` spread survives. Lifting the
            // children out discards the inline fragment itself, so its own
            // directives travel with them.
            let mut selections = Vec::with_capacity(resolved.len());
            for sel in resolved {
                if sel.name.starts_with("...on ") {
                    for mut child in sel.nested_fields {
                        child.inherit_directives(&sel.directives);
                        selections.push(child);
                    }
                } else {
                    selections.push(sel);
                }
            }
            return Ok((QueryType::NodeQuery { selections }, None));
        }

        // Mutations are routed by operation type. Carry the full result selection
        // set (inline fragments intact) so the projector can subset and recurse
        // exactly like the query path. Named fragment spreads are resolved here —
        // the same as the query matcher — using the document's fragment
        // definitions; `@skip`/`@include` directives are evaluated later in
        // `execute_mutation_impl`, where the request variables are available.
        //
        // *Every* root is carried, not just the first: the spec requires all of a
        // mutation's root fields to execute, serially, in document order (#759).
        if parsed.operation_type == "mutation" {
            let roots = parsed
                .selections
                .iter()
                .map(|root| {
                    Ok(MutationRoot {
                        response_key: root.response_key().to_string(),
                        name:         root.name.clone(),
                        selections:   crate::graphql::selection_set::resolve(
                            &root.nested_fields,
                            &parsed.fragments,
                        )?,
                        arguments:    root.arguments.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok((QueryType::Mutation { roots }, None));
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

    // `value_json` holds a JSON document. Peeling the outer quotes by hand and
    // unescaping only `\"` is the same defect as #719's writer: a value
    // containing a backslash or a newline came back mangled.
    let decoded = crate::graphql::value_json::decode(&arg.value_json).ok()?;
    Some(decoded.as_str()?.to_string())
}
