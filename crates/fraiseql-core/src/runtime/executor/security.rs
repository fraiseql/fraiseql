//! Security-aware execution — field access, RBAC filtering, JWT inject resolution,
//! `execute_with_context()`, `execute_with_security()`, `execute_json()`.

use std::time::Duration;

use super::{Executor, QueryType};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    runtime::{ExecutionContext, classify_field_access},
    schema::{SessionVariableSource, SessionVariablesConfig},
    security::{FieldAccessError, SecurityContext},
};

/// Resolve session variable mappings against the current security context.
///
/// Returns a list of `(name, value)` pairs to inject as PostgreSQL transaction-scoped
/// session variables via `set_config()`.
///
/// Resolution rules:
/// - [`SessionVariableSource::Jwt`] — looks up the claim in
///   `security_context.attributes`; falls back to `user_id` for `"sub"` and to
///   `tenant_id` for `"tenant_id"`.  Missing claims are silently skipped.
/// - [`SessionVariableSource::Header`] — looks up the header name in
///   `security_context.attributes`.  Missing headers are silently skipped.
/// - [`SessionVariableSource::Literal`] — uses the fixed value as-is.
///
/// When `config.inject_started_at` is `true`, the pair
/// `("fraiseql.started_at", <RFC 3339 now>)` is **prepended** to the returned list.
#[must_use]
pub fn resolve_session_variables(
    config: &SessionVariablesConfig,
    security_context: &SecurityContext,
) -> Vec<(String, String)> {
    use chrono::Utc;

    let mut vars: Vec<(String, String)> = Vec::new();

    if config.inject_started_at {
        vars.push(("fraiseql.started_at".to_string(), Utc::now().to_rfc3339()));
    }

    for mapping in &config.variables {
        let value: Option<String> = match &mapping.source {
            SessionVariableSource::Jwt { claim } => {
                // Check custom attributes first (raw JWT claims forwarded there).
                // Fall back to well-known SecurityContext fields for `sub`/`user_id`
                // and `tenant_id` so that schemas that populate only those fields
                // (not attributes) still work.
                if let Some(v) = security_context.attributes.get(claim.as_str()) {
                    Some(if let serde_json::Value::String(s) = v {
                        s.clone()
                    } else {
                        v.to_string()
                    })
                } else if claim == "sub" || claim == "user_id" {
                    Some(security_context.user_id.clone())
                } else if claim == "tenant_id" {
                    security_context.tenant_id.clone()
                } else {
                    None
                }
            },
            SessionVariableSource::Header { header } => {
                // HTTP headers are forwarded into attributes
                security_context
                    .attributes
                    .get(header.as_str())
                    .map(|v| {
                        if let serde_json::Value::String(s) = v {
                            s.clone()
                        } else {
                            v.to_string()
                        }
                    })
            },
            SessionVariableSource::Literal { value } => Some(value.clone()),
        };
        if let Some(v) = value {
            vars.push((mapping.name.clone(), v));
        }
    }

    vars
}

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

    /// Execute a GraphQL query with cancellation support via `ExecutionContext`.
    ///
    /// This method allows graceful cancellation of long-running queries through a
    /// cancellation token. If the token is cancelled during execution, the query
    /// returns a `FraiseQLError::Cancelled` error.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    /// * `ctx` - `ExecutionContext` with cancellation token
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string, or error if cancelled or execution fails
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Cancelled`] — the cancellation token was triggered before or during
    ///   execution.
    /// * Propagates any error from the underlying [`execute`](Self::execute) call.
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

    /// Internal execution logic with security context (called by `execute_with_security` with
    /// timeout wrapper).
    async fn execute_with_security_internal(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Route to appropriate handler (with RLS support for regular queries)
        match query_type {
            QueryType::Regular => {
                self.execute_regular_query_with_security(query, variables, security_context)
                    .await
            },
            // Other query types don't support RLS yet (relay is handled inside
            // execute_regular_query_with_security)
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            },
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            },
            #[cfg(feature = "federation")]
            QueryType::Federation(query_name) => {
                self.execute_federation_query(&query_name, query, variables).await
            },
            #[cfg(not(feature = "federation"))]
            QueryType::Federation(_) => {
                let _ = (query, variables);
                Err(FraiseQLError::Validation {
                    message: "Federation is not enabled in this build".to_string(),
                    path:    None,
                })
            },
            QueryType::IntrospectionSchema => Ok(self.introspection.schema_response.clone()),
            QueryType::IntrospectionType(type_name) => {
                Ok(self.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation { name, type_selections } => {
                self.execute_mutation_query_with_security(
                    &name,
                    variables,
                    Some(security_context),
                    &type_selections,
                )
                .await
            },
            QueryType::NodeQuery { selections } => {
                self.execute_node_query(query, variables, &selections).await
            },
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
    /// Returns `FieldAccessError::AccessDenied` if the user's scopes do not include the
    /// required scope for the field.
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
    /// Propagates all errors from [`Self::execute`] and additionally returns
    /// [`FraiseQLError::Database`] if the response string is not valid JSON.
    pub async fn execute_json(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let result_str = self.execute(query, variables).await?;
        Ok(serde_json::from_str(&result_str)?)
    }
}

#[cfg(test)]
mod session_variable_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Utc;

    use super::resolve_session_variables;
    use crate::{
        schema::{
            SessionVariableMapping, SessionVariableSource, SessionVariablesConfig,
        },
        security::SecurityContext,
    };

    fn make_context() -> SecurityContext {
        let mut attributes = std::collections::HashMap::new();
        attributes.insert("tenant_id".to_string(), serde_json::json!("tenant-abc"));
        attributes.insert("x-tenant-id".to_string(), serde_json::json!("header-tenant"));
        attributes.insert("region".to_string(), serde_json::json!("eu-west-1"));
        SecurityContext {
            user_id:          "user-42".to_string(),
            roles:            vec!["admin".to_string()],
            tenant_id:        Some("tenant-123".to_string()),
            scopes:           vec![],
            attributes,
            request_id:       "req-test".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now(),
            issuer:           None,
            audience:         None,
        }
    }

    #[test]
    fn resolve_session_variables_jwt_claim() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables: vec![SessionVariableMapping {
                name:   "app.tenant_id".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "tenant_id".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        // tenant_id is in attributes
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.tenant_id");
        assert_eq!(vars[0].1, "tenant-abc");
    }

    #[test]
    fn resolve_session_variables_jwt_well_known_sub() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables: vec![SessionVariableMapping {
                name:   "app.user_id".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "sub".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.user_id");
        assert_eq!(vars[0].1, "user-42");
    }

    #[test]
    fn resolve_session_variables_literal() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables: vec![SessionVariableMapping {
                name:   "app.locale".to_string(),
                source: SessionVariableSource::Literal { value: "en".to_string() },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.locale");
        assert_eq!(vars[0].1, "en");
    }

    #[test]
    fn inject_started_at_prepended() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables: vec![SessionVariableMapping {
                name:   "app.locale".to_string(),
                source: SessionVariableSource::Literal { value: "en".to_string() },
            }],
            inject_started_at: true,
        };
        let vars = resolve_session_variables(&config, &ctx);
        // started_at must come first
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].0, "fraiseql.started_at");
        // Verify it's an ISO 8601 / RFC 3339 string (contains 'T')
        assert!(vars[0].1.contains('T'), "started_at should be ISO 8601");
        assert_eq!(vars[1].0, "app.locale");
    }

    #[test]
    fn inject_started_at_disabled() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables:         vec![],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert!(vars.is_empty());
        assert!(!vars.iter().any(|(k, _)| k == "fraiseql.started_at"));
    }

    #[test]
    fn resolve_session_variables_header() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables: vec![SessionVariableMapping {
                name:   "app.tenant".to_string(),
                source: SessionVariableSource::Header {
                    header: "x-tenant-id".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.tenant");
        assert_eq!(vars[0].1, "header-tenant");
    }
}
