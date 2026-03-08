//! Core query execution — `execute()`, `execute_internal()`, `execute_with_scopes()`.

use std::time::Duration;

use super::{Executor, QueryType, pipeline};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a GraphQL query string and return a serialized JSON response.
    ///
    /// Applies the configured query timeout if one is set. Handles queries,
    /// mutations, introspection, federation, and node lookups.
    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Apply query timeout if configured
        if self.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.config.query_timeout_ms);
            tokio::time::timeout(timeout_duration, self.execute_internal(query, variables))
                .await
                .map_err(|_| {
                    // Truncate query if too long for error reporting
                    let query_snippet = if query.len() > 100 {
                        format!("{}...", &query[..100])
                    } else {
                        query.to_string()
                    };
                    FraiseQLError::Timeout {
                        timeout_ms: self.config.query_timeout_ms,
                        query:      Some(query_snippet),
                    }
                })?
        } else {
            self.execute_internal(query, variables).await
        }
    }

    /// Internal execution logic (called by execute with timeout wrapper).
    pub(super) async fn execute_internal(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Classify query type — also returns the ParsedQuery for Regular
        // queries so we do not parse the same string twice.
        let (query_type, maybe_parsed) = self.classify_query_with_parse(query)?;

        // 2. Route to appropriate handler
        match query_type {
            QueryType::Regular => {
                // Detect multi-root queries and dispatch them in parallel.
                // `maybe_parsed` is always Some for Regular queries (see classify_query_with_parse).
                let parsed = maybe_parsed.expect("parsed present for Regular query type");
                if pipeline::is_multi_root(&parsed) {
                    let pr = self.execute_parallel(&parsed, variables).await?;
                    let data = pr.merge_into_data_map();
                    return serde_json::to_string(&serde_json::json!({ "data": data }))
                        .map_err(|e| FraiseQLError::Internal {
                            message: e.to_string(),
                            source:  None,
                        });
                }
                self.execute_regular_query(query, variables).await
            },
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            QueryType::IntrospectionSchema => {
                // Return pre-built __schema response (zero-cost at runtime)
                Ok(self.introspection.schema_response.clone())
            },
            QueryType::IntrospectionType(type_name) => {
                // Return pre-built __type response (zero-cost at runtime)
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation(mutation_name) => {
                self.execute_mutation_query(&mutation_name, variables).await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }

    /// Execute a GraphQL query with user context for field-level access control.
    ///
    /// This method validates that the user has permission to access all requested
    /// fields before executing the query. If field filtering is enabled in the
    /// `RuntimeConfig` and the user lacks required scopes, this returns an error.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `user_scopes` - User's scopes from JWT token (pass empty slice if unauthenticated)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if access denied
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and authenticated user context.
    /// // See: tests/integration/ for runnable examples.
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let query = r#"query { users { id name salary } }"#;
    /// // let user_scopes = user.scopes.clone();
    /// // let result = executor.execute_with_scopes(query, None, &user_scopes).await?;
    /// # Ok(()) }
    /// ```
    pub async fn execute_with_scopes(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        user_scopes: &[String],
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Validate field access if filter is configured
        if let Some(ref filter) = self.config.field_filter {
            // Only validate for regular queries (not introspection)
            if matches!(query_type, QueryType::Regular) {
                self.validate_field_access(query, variables, user_scopes, filter)?;
            }
        }

        // 3. Route to appropriate handler (same as execute)
        match query_type {
            QueryType::Regular => self.execute_regular_query(query, variables).await,
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            QueryType::IntrospectionSchema => Ok(self.introspection.schema_response.clone()),
            QueryType::IntrospectionType(type_name) => {
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation(mutation_name) => {
                self.execute_mutation_query(&mutation_name, variables).await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }
}
