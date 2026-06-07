//! Security-aware execution — field access, RBAC filtering, JWT inject resolution,
//! `execute_with_context()`, `execute_with_security()`, `execute_json()`.

use std::time::Duration;

use super::{Executor, QueryType, runners, support};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    runtime::ExecutionContext,
    schema::SessionVariablesConfig,
    security::{FieldAccessError, SecurityContext},
};

/// Resolve session variable mappings against the current security context.
///
/// See [`support::security::resolve_session_variables`] for full documentation.
#[must_use]
pub fn resolve_session_variables(
    config: &SessionVariablesConfig,
    security_context: &SecurityContext,
) -> Vec<(String, String)> {
    support::security::resolve_session_variables(config, security_context)
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
        let query_match = self.ctx.matcher.match_query(query, variables)?;

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
    ) -> Result<serde_json::Value> {
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
    ) -> Result<serde_json::Value> {
        // Apply query timeout if configured
        if self.ctx.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.ctx.config.query_timeout_ms);
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
                    timeout_ms: self.ctx.config.query_timeout_ms,
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
    ) -> Result<serde_json::Value> {
        // 1. Classify query type. Retain the parsed AST for `Regular` queries — the authorizer
        //    needs the per-root operation names to gate multi-root requests.
        let (query_type, parsed) = self.classify_query_with_parse(query)?;

        // 1b. Operation-level authorization (#422): consult the configured `Authorizer`
        //     before dispatch. Mutations are gated downstream at `execute_mutation_impl`
        //     (the single point every mutation entry path converges), so
        //     `collect_authz_ops` returns no ops for the `Mutation` variant here.
        //     Fail-closed: a `Deny` or any policy error returns 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = support::authz::collect_authz_ops(&query_type, parsed.as_ref());
            crate::security::authorizer::enforce_authz(
                authorizer.as_ref(),
                Some(security_context),
                &ops,
                variables,
            )?;
        }

        // 2. Route to appropriate handler (with RLS support for regular queries)
        match query_type {
            QueryType::Regular => {
                self.query_runner()
                    .execute_regular_query_with_security(query, variables, security_context)
                    .await
            },
            QueryType::Aggregate(query_name) => {
                self.aggregate_runner()
                    .execute_aggregate_dispatch(&query_name, variables, Some(security_context))
                    .await
            },
            QueryType::Window(query_name) => {
                self.aggregate_runner()
                    .execute_window_dispatch(&query_name, variables, Some(security_context))
                    .await
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
            QueryType::IntrospectionSchema => {
                Ok(self.ctx.introspection.schema_response.as_ref().clone())
            },
            QueryType::IntrospectionType(type_name) => {
                Ok(self.ctx.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation { name, selections } => {
                runners::mutation::execute_mutation_impl(
                    &self.ctx,
                    &name,
                    variables,
                    Some(security_context),
                    &selections,
                )
                .await
            },
            QueryType::NodeQuery { selections } => {
                self.query_runner().execute_node_query(query, variables, &selections).await
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
        if let Some(ref filter) = self.ctx.config.field_filter {
            filter.can_access(type_name, field_name, user_scopes)
        } else {
            // No filter configured, allow all access
            Ok(())
        }
    }

    /// Execute a query and return parsed JSON.
    ///
    /// This method is now equivalent to `execute()` since `execute()` already
    /// returns `serde_json::Value`.
    ///
    /// # Errors
    ///
    /// Returns any error from `execute()`.
    #[deprecated(
        since = "2.2.0",
        note = "use execute() directly — it now returns Value"
    )]
    pub async fn execute_json(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.execute(query, variables).await
    }
}

#[cfg(test)]
mod session_variable_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Utc;

    use super::resolve_session_variables;
    use crate::{
        schema::{SessionVariableMapping, SessionVariableSource, SessionVariablesConfig},
        security::SecurityContext,
    };

    fn make_context() -> SecurityContext {
        let mut attributes = std::collections::HashMap::new();
        attributes.insert("tenant_id".to_string(), serde_json::json!("tenant-abc"));
        attributes.insert("x-tenant-id".to_string(), serde_json::json!("header-tenant"));
        attributes.insert("region".to_string(), serde_json::json!("eu-west-1"));
        SecurityContext {
            user_id: crate::types::UserId::new("user-42"),
            roles: vec!["admin".to_string()],
            tenant_id: Some(crate::types::TenantId::new("tenant-123")),
            scopes: vec![],
            attributes,
            request_id: "req-test".to_string(),
            ip_address: None,
            authenticated_at: Utc::now(),
            expires_at: Utc::now(),
            issuer: None,
            audience: None,
            email: None,
            display_name: None,
        }
    }

    #[test]
    fn resolve_session_variables_jwt_claim() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
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
            variables:         vec![SessionVariableMapping {
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
            variables:         vec![SessionVariableMapping {
                name:   "app.locale".to_string(),
                source: SessionVariableSource::Literal {
                    value: "en".to_string(),
                },
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
            variables:         vec![SessionVariableMapping {
                name:   "app.locale".to_string(),
                source: SessionVariableSource::Literal {
                    value: "en".to_string(),
                },
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
            variables:         vec![SessionVariableMapping {
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

    #[test]
    fn resolve_session_variables_jwt_email() {
        let mut ctx = make_context();
        ctx.email = Some("user@corp.com".to_string());
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.email".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "email".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.email");
        assert_eq!(vars[0].1, "user@corp.com");
    }

    #[test]
    fn resolve_session_variables_jwt_display_name() {
        let mut ctx = make_context();
        ctx.display_name = Some("Jane Doe".to_string());
        let config = SessionVariablesConfig {
            variables:         vec![
                SessionVariableMapping {
                    name:   "app.name".to_string(),
                    source: SessionVariableSource::Jwt {
                        claim: "name".to_string(),
                    },
                },
                SessionVariableMapping {
                    name:   "app.display_name".to_string(),
                    source: SessionVariableSource::Jwt {
                        claim: "display_name".to_string(),
                    },
                },
            ],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].1, "Jane Doe");
        assert_eq!(vars[1].1, "Jane Doe");
    }

    #[test]
    fn resolve_session_variables_missing_email_skipped() {
        let ctx = make_context(); // email is None
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.email".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "email".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx);
        assert!(vars.is_empty(), "missing email should be silently skipped");
    }
}
