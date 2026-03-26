//! Security-aware execution — field access, RBAC filtering, JWT inject resolution,
//! `execute_with_context()`, `execute_with_security()`, `execute_json()`.

use std::time::Duration;

use super::{Executor, QueryType};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    runtime::{ExecutionContext, classify_field_access},
    security::{FieldAccessError, SecurityContext},
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Validate that user has access to all requested fields.
    pub(super) fn validate_field_access(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        user_scopes: &[String],
        filter: &crate::security::FieldFilter,
    ) -> Result<()> {
        // Parse query to get field selections
        let query_match = self.matcher.match_query(query, variables)?;

        // Get the return type name from the query definition
        let type_name = &query_match.query_def.return_type;

        // Validate each requested field
        let field_refs: Vec<&str> = query_match.fields.iter().map(String::as_str).collect();
        let errors = filter.validate_fields(type_name, &field_refs, user_scopes);

        if errors.is_empty() {
            Ok(())
        } else {
            // Return the first error (could aggregate all errors if desired)
            let first_error = &errors[0];
            Err(FraiseQLError::Authorization {
                message:  first_error.message.clone(),
                action:   Some("read".to_string()),
                resource: Some(format!("{}.{}", first_error.type_name, first_error.field_name)),
            })
        }
    }

    /// Execute a GraphQL query with cancellation support via ExecutionContext.
    ///
    /// This method allows graceful cancellation of long-running queries through a
    /// cancellation token. If the token is cancelled during execution, the query
    /// returns a `FraiseQLError::Cancelled` error.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `ctx` - ExecutionContext with cancellation token
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if cancelled or execution fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and running tokio runtime.
    /// // See: tests/integration/ for runnable examples.
    /// use fraiseql_core::runtime::ExecutionContext;
    /// use fraiseql_core::error::FraiseQLError;
    /// use std::time::Duration;
    ///
    /// let ctx = ExecutionContext::new("user-query-123".to_string());
    /// let cancel_token = ctx.cancellation_token().clone();
    ///
    /// // Spawn a task to cancel after 5 seconds
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(Duration::from_secs(5)).await;
    ///     cancel_token.cancel();
    /// });
    ///
    /// // let result = executor.execute_with_context(query, None, &ctx).await;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Cancelled` if the context is cancelled before or
    /// during execution, or any error from the underlying `execute()` call.
    pub async fn execute_with_context(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        ctx: &ExecutionContext,
    ) -> Result<String> {
        // Check if already cancelled before starting
        if ctx.is_cancelled() {
            return Err(FraiseQLError::cancelled(
                ctx.query_id().to_string(),
                "Query cancelled before execution".to_string(),
            ));
        }

        let token = ctx.cancellation_token().clone();

        // Use tokio::select! to race between execution and cancellation
        tokio::select! {
            result = self.execute(query, variables) => {
                result
            }
            () = token.cancelled() => {
                Err(FraiseQLError::cancelled(
                    ctx.query_id().to_string(),
                    "Query cancelled during execution".to_string(),
                ))
            }
        }
    }

    /// Execute a GraphQL query or mutation with a JWT [`SecurityContext`].
    ///
    /// This is the **main authenticated entry point** for the executor. It routes the
    /// incoming request to the appropriate handler based on the query type:
    ///
    /// - **Regular queries**: RLS `WHERE` clauses are applied so each user only sees their own
    ///   rows, as determined by the RLS policy in `RuntimeConfig`.
    /// - **Mutations**: The security context is forwarded to `execute_mutation_query_with_security`
    ///   so server-side `inject` parameters (e.g. `jwt:sub`) are resolved from the caller's JWT
    ///   claims.
    /// - **Aggregations, window queries, federation, introspection**: Delegated to their respective
    ///   handlers (security context is not yet applied to these).
    ///
    /// If `query_timeout_ms` is non-zero in the `RuntimeConfig`, the entire
    /// execution is raced against a Tokio deadline and returns
    /// [`FraiseQLError::Timeout`] when the deadline is exceeded.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string (e.g. `"query { posts { id title } }"`)
    /// * `variables` - Optional JSON object of GraphQL variable values
    /// * `security_context` - Authenticated user context extracted from a validated JWT
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response string on success, conforming to the
    /// [GraphQL over HTTP](https://graphql.github.io/graphql-over-http/) specification.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Parse`] — the query string is not valid GraphQL
    /// * [`FraiseQLError::Validation`] — unknown mutation name, missing `sql_source`, or a mutation
    ///   requires `inject` params but the security context is absent
    /// * [`FraiseQLError::Database`] — the underlying adapter returns an error
    /// * [`FraiseQLError::Timeout`] — execution exceeded `query_timeout_ms`
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and a SecurityContext from authentication.
    /// // See: tests/integration/ for runnable examples.
    /// use fraiseql_core::security::SecurityContext;
    ///
    /// // let query = r#"query { posts { id title } }"#;
    /// // Returns a JSON string: {"data":{"posts":[...]}}
    /// // let result = executor.execute_with_security(query, None, &context).await?;
    /// ```
    pub async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // Apply query timeout if configured
        if self.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.config.query_timeout_ms);
            tokio::time::timeout(
                timeout_duration,
                self.execute_with_security_internal(query, variables, security_context),
            )
            .await
            .map_err(|_| {
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
            self.execute_with_security_internal(query, variables, security_context).await
        }
    }

    /// Resolve and emit session variables from the compiled schema configuration.
    ///
    /// Reads `session_variables_config` from the schema, resolves each variable's
    /// value from the security context (JWT claims or HTTP headers), and calls
    /// `adapter.set_session_variables()` to emit `SET LOCAL` on PostgreSQL.
    ///
    /// For mutations, also injects the built-in `fraiseql.started_at` timestamp
    /// if `inject_started_at` is enabled (default: true).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if a required JWT claim is missing.
    /// Returns `FraiseQLError::Database` if `set_config()` fails.
    async fn emit_session_variables(
        &self,
        security_context: &SecurityContext,
        is_mutation: bool,
    ) -> Result<()> {
        use crate::schema::SessionVariableSource;

        let Some(config) = self.schema.session_variables_config.as_ref() else {
            return Ok(());
        };

        let mut resolved: Vec<(String, String)> = Vec::with_capacity(
            config.variables.len() + usize::from(is_mutation && config.inject_started_at),
        );

        for mapping in &config.variables {
            let value = match &mapping.source {
                SessionVariableSource::Jwt { claim } => match claim.as_str() {
                    "sub" => security_context.user_id.clone(),
                    "tenant_id" | "org_id" => {
                        security_context.tenant_id.clone().unwrap_or_default()
                    },
                    other => security_context
                        .attributes
                        .get(other)
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default(),
                },
                SessionVariableSource::Header { name } => {
                    // Headers are forwarded via SecurityContext.attributes with "header:" prefix
                    security_context
                        .attributes
                        .get(&format!("header:{name}"))
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default()
                },
            };
            resolved.push((mapping.pg_name.clone(), value));
        }

        // Built-in: fraiseql.started_at for mutations
        if is_mutation && config.inject_started_at {
            resolved.push(("fraiseql.started_at".to_string(), chrono::Utc::now().to_rfc3339()));
        }

        if resolved.is_empty() {
            return Ok(());
        }

        let pairs: Vec<(&str, &str)> =
            resolved.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.adapter.set_session_variables(&pairs).await
    }

    /// Internal execution logic with security context (called by execute_with_security with timeout
    /// wrapper).
    async fn execute_with_security_internal(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 1a. Emit session variables if configured
        let is_mutation = matches!(query_type, QueryType::Mutation(_));
        self.emit_session_variables(security_context, is_mutation).await?;

        // 2. Route to appropriate handler (with RLS support for regular queries)
        match query_type {
            QueryType::Regular => {
                self.execute_regular_query_with_security(query, variables, security_context)
                    .await
            },
            // Other query types don't support RLS yet (relay is handled inside execute_regular_query_with_security)
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
                self.execute_mutation_query_with_security(
                    &mutation_name,
                    variables,
                    Some(security_context),
                )
                .await
            },
            QueryType::NodeQuery => self.execute_node_query(query, variables).await,
        }
    }

    /// Check if a specific field can be accessed with given scopes.
    ///
    /// This is a convenience method for checking field access without executing a query.
    ///
    /// # Arguments
    ///
    /// * `type_name` - The GraphQL type name
    /// * `field_name` - The field name
    /// * `user_scopes` - User's scopes from JWT token
    ///
    /// # Returns
    ///
    /// `Ok(())` if access is allowed, `Err(FieldAccessError)` if denied
    ///
    /// # Errors
    ///
    /// Returns `FieldAccessError` if the user lacks the required scope for the field.
    pub fn check_field_access(
        &self,
        type_name: &str,
        field_name: &str,
        user_scopes: &[String],
    ) -> std::result::Result<(), FieldAccessError> {
        if let Some(ref filter) = self.config.field_filter {
            filter.can_access(type_name, field_name, user_scopes)
        } else {
            // No filter configured, allow all access
            Ok(())
        }
    }

    /// Apply field-level RBAC filtering to projection fields.
    ///
    /// Classifies each requested field against the user's security context:
    /// - **Allowed**: user has the required scope (or field is public)
    /// - **Masked**: user lacks scope, but `on_deny = Mask` → field value will be nulled
    /// - **Rejected**: user lacks scope, `on_deny = Reject` → query fails with FORBIDDEN
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Forbidden` if any requested field has `on_deny = Reject`
    /// and the user lacks the required scope.
    pub(super) fn apply_field_rbac_filtering(
        &self,
        return_type: &str,
        projection_fields: Vec<String>,
        security_context: &SecurityContext,
    ) -> Result<super::super::field_filter::FieldAccessResult> {
        use super::super::field_filter::FieldAccessResult;

        // Try to extract security config from compiled schema
        if let Some(security_config) = self.schema.security.as_ref() {
            if let Some(type_def) = self.schema.types.iter().find(|t| t.name == return_type) {
                return classify_field_access(
                    security_context,
                    security_config,
                    &type_def.fields,
                    projection_fields,
                )
                .map_err(|rejected_field| FraiseQLError::Authorization {
                    message:  format!(
                        "Access denied: field '{rejected_field}' on type '{return_type}' \
                         requires a scope you do not have"
                    ),
                    action:   Some("read".to_string()),
                    resource: Some(format!("{return_type}.{rejected_field}")),
                });
            }
        }

        // No security config or type not found → all fields allowed, none masked
        Ok(FieldAccessResult {
            allowed: projection_fields,
            masked:  Vec::new(),
        })
    }

    /// Execute a query and return parsed JSON.
    ///
    /// Same as `execute()` but returns parsed `serde_json::Value` instead of string.
    ///
    /// # Errors
    ///
    /// Returns any error from `execute()`, or `FraiseQLError` if the result
    /// string is not valid JSON.
    pub async fn execute_json(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let result_str = self.execute(query, variables).await?;
        Ok(serde_json::from_str(&result_str)?)
    }
}
