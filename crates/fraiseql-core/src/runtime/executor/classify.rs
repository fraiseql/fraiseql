//! Query classification — determines operation type for routing.

use crate::{
    error::{FraiseQLError, Result},
    graphql::parse_query,
};

use super::{Executor, QueryType};
use crate::db::traits::DatabaseAdapter;

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
    /// # Performance Notes
    ///
    /// - Introspection and federation use cheap text scans (no parsing)
    /// - Other queries require full GraphQL parsing
    /// - Classification result is used to route to specialized handlers
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Parse` if the query string is malformed GraphQL.
    ///
    /// # Example
    ///
    /// ```no_run
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
        // Check for introspection queries first (highest priority).
        // These use a cheap text scan to avoid parsing queries that only
        // need the built-in introspection response.
        if let Some(introspection_type) = self.detect_introspection(query) {
            return Ok(introspection_type);
        }

        // Check for federation queries (higher priority than regular queries).
        // Also a text scan — federation queries bypass normal execution.
        if let Some(federation_type) = self.detect_federation(query) {
            return Ok(federation_type);
        }

        // Parse the query to extract the root field name and operation type.
        let parsed = parse_query(query).map_err(|e| FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        })?;

        let root_field = &parsed.root_field;

        // Relay global node lookup: root field is exactly "node" on a query operation.
        if parsed.operation_type == "query" && root_field == "node" {
            return Ok(QueryType::NodeQuery);
        }

        // Mutations are routed by operation type
        if parsed.operation_type == "mutation" {
            return Ok(QueryType::Mutation(root_field.clone()));
        }

        // Check if it's an aggregate query (ends with _aggregate)
        if root_field.ends_with("_aggregate") {
            return Ok(QueryType::Aggregate(root_field.clone()));
        }

        // Check if it's a window query (ends with _window)
        if root_field.ends_with("_window") {
            return Ok(QueryType::Window(root_field.clone()));
        }

        // Otherwise, it's a regular query
        Ok(QueryType::Regular)
    }

    /// Detect if a query is an introspection query.
    ///
    /// Returns `Some(QueryType)` for introspection queries, `None` otherwise.
    pub(super) fn detect_introspection(&self, query: &str) -> Option<QueryType> {
        let query_trimmed = query.trim();

        // Check for __schema query
        if query_trimmed.contains("__schema") {
            return Some(QueryType::IntrospectionSchema);
        }

        // Check for __type(name: "...") query
        if query_trimmed.contains("__type") {
            // Extract the type name from __type(name: "TypeName")
            if let Some(type_name) = self.extract_type_argument(query_trimmed) {
                return Some(QueryType::IntrospectionType(type_name));
            }
            // If no type name found, return schema introspection as fallback
            return Some(QueryType::IntrospectionSchema);
        }

        None
    }

    /// Detect if a query is a federation query (_service or _entities).
    ///
    /// Returns `Some(QueryType)` for federation queries, `None` otherwise.
    pub(super) fn detect_federation(&self, query: &str) -> Option<QueryType> {
        let query_trimmed = query.trim();

        // Check for _service query
        if query_trimmed.contains("_service") {
            return Some(QueryType::Federation("_service".to_string()));
        }

        // Check for _entities query
        if query_trimmed.contains("_entities") {
            return Some(QueryType::Federation("_entities".to_string()));
        }

        None
    }

    /// Extract the type name argument from `__type(name: "TypeName")`.
    pub(super) fn extract_type_argument(&self, query: &str) -> Option<String> {
        // Find __type(name: "..." pattern
        // Supports: __type(name: "User"), __type(name:"User"), __type(name: 'User')
        let type_pos = query.find("__type")?;
        let after_type = &query[type_pos + 6..];

        // Find the opening parenthesis
        let paren_pos = after_type.find('(')?;
        let after_paren = &after_type[paren_pos + 1..];

        // Find name: and extract the value
        let name_pos = after_paren.find("name")?;
        let after_name = &after_paren[name_pos + 4..].trim_start();

        // Skip colon
        let after_colon = if let Some(stripped) = after_name.strip_prefix(':') {
            stripped.trim_start()
        } else {
            after_name
        };

        // Extract string value (either "..." or '...')
        let quote_char = after_colon.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }

        let after_quote = &after_colon[1..];
        let end_quote = after_quote.find(quote_char)?;
        Some(after_quote[..end_quote].to_string())
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
