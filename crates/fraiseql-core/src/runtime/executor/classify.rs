//! Query classification — determines operation type for routing.

use super::{Executor, QueryType};
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
    /// ```ignore
    /// // Requires: live executor with compiled schema.
    /// // Regular query
    /// let query_type = executor.classify_query("{ users { id } }")?;
    /// assert_eq!(query_type, QueryType::Regular);
    ///
    /// // Mutation
    /// let query_type = executor.classify_query("mutation { createUser(...) { id } }")?;
    /// // Routes to execute_mutation_query()
    ///
    /// // Introspection (uses pre-built response)
    /// let query_type = executor.classify_query("{ __schema { types { name } } }")?;
    /// // Routes to introspection.schema_response
    /// ```
    pub(super) fn classify_query(&self, query: &str) -> Result<QueryType> {
        self.classify_query_with_parse(query).map(|(qt, _)| qt)
    }

    /// Classify a query and simultaneously return the parsed AST for `Regular`
    /// queries, avoiding a redundant parse in the multi-root pipeline path.
    ///
    /// Returns `(QueryType, Some(ParsedQuery))` for `Regular` queries and
    /// `(QueryType, None)` for all other types (introspection, federation, etc.).
    pub(super) fn classify_query_with_parse(
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

        // Federation (Apollo Federation v1/v2 entry-points).
        if root_field == "_service" || root_field == "_entities" {
            return Ok((QueryType::Federation(root_field.clone()), None));
        }

        // Relay global node lookup: root field is exactly "node" on a query.
        if parsed.operation_type == "query" && root_field == "node" {
            return Ok((QueryType::NodeQuery, None));
        }

        // Mutations are routed by operation type.
        if parsed.operation_type == "mutation" {
            return Ok((QueryType::Mutation(root_field.clone()), None));
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

    /// Extract an inline node ID literal from a `node(id: "...")` query string.
    ///
    /// Used as a fallback when the ID is not provided via variables.
    /// Returns `None` if no inline string literal can be found.
    pub(super) fn extract_inline_node_id(query: &str) -> Option<String> {
        // Look for  node(  ...  id:  "value"  or  id: 'value'
        let after_node = query.find("node(")?;
        let args_region = &query[after_node..];
        // Find `id:` within the argument region.
        let after_id = args_region.find("id:")?;
        let after_colon = args_region[after_id + 3..].trim_start();
        // Expect a quoted string.
        let quote_char = after_colon.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }
        let inner = &after_colon[1..];
        let end = inner.find(quote_char)?;
        Some(inner[..end].to_string())
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
