//! Security-aware execution — field access, RBAC filtering, JWT inject resolution,
//! `execute_with_context()`, `execute_with_security()`, `execute_json()`.

use super::{Executor, support};
use crate::{
    error::{FraiseQLError, Result},
    runtime::ExecutionContext,
    schema::SessionVariablesConfig,
    security::{FieldAccessError, SecurityContext},
};

/// Resolve session variable mappings against the current security context.
///
/// Returns the `(name, value)` pairs to apply transaction-locally with
/// `set_config` before the statement, so PostgreSQL RLS policies reading
/// `current_setting()` see the caller's identity (#329).
///
/// Resolution follows each mapping's [`SessionVariableSource`]: a `Jwt` claim is
/// looked up in the context's attributes, falling back to `user_id` for
/// `sub`/`user_id` and to `tenant_id`/`email`/`name` for their own claims; a
/// `Header` is read from attributes; a `Literal` is used as-is; and an
/// `Enrichment` field reads the reserved `fraiseql.enriched.*` namespace with
/// **no** fallback — a missing enriched field is an error, never a silently
/// absent GUC (#539). With `inject_started_at`, the started-at directive is
/// prepended for the adapter to stamp on the database clock.
///
/// This is the one construction site: the query, mutation and aggregate runners
/// call it, and so does the admin SQL console's RLS preview (#962). A preview
/// computed by a second implementation would be a preview of that
/// implementation.
///
/// [`SessionVariableSource`]: crate::schema::SessionVariableSource
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if a `SessionVariableSource::Enrichment`
/// mapping references an enriched field absent from the resolved identity (#539).
pub fn resolve_session_variables(
    config: &SessionVariablesConfig,
    security_context: &SecurityContext,
) -> crate::error::Result<Vec<(String, String)>> {
    support::security::resolve_session_variables(config, security_context)
}

impl Executor {
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
    /// - **Mutations**: the security context is forwarded so server-side `inject` parameters (e.g.
    ///   `jwt:sub`) are resolved from the caller's JWT claims.
    /// - **Multi-root queries** (e.g. `{ users { id } posts { id } }`): each root is dispatched in
    ///   parallel with the security context applied to every root (H19).
    /// - **Aggregations, window queries, federation, node lookups**: the security context **is**
    ///   forwarded to each handler (RLS / `requires_role` / `inject` gates apply).
    /// - **Introspection**: served from the pre-built response (no per-user data).
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
        // Authenticated entry: delegate to the shared dispatch with the principal.
        // GATE-1, the parse cache, the multi-root fan-out, and every per-operation
        // runner are threaded with `Some(security_context)` in `execute_dispatch`,
        // so this path cannot drift from the anonymous one (H19, L-gate1-skip,
        // L-parse-cache). The timeout wrapper is shared via `execute_with_timeout`.
        self.execute_with_timeout(query, variables, Some(security_context), None).await
    }

    /// Authenticated execution that selects the operation named by
    /// `operation_name` (GraphQL § 6.1 *`GetOperation`*).
    ///
    /// [`execute_with_security`](Self::execute_with_security) is this with
    /// `None`, which requires the document to define exactly one operation.
    /// Every HTTP request carries an `operationName` field, so the server path
    /// uses this entry point.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Parse`] — the document does not parse, names an operation that does not
    ///   exist, or defines several operations while `operation_name` is `None`.
    /// - Any error returned by [`execute_with_security`](Self::execute_with_security).
    pub async fn execute_operation_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
        operation_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.execute_with_timeout(
            query,
            variables,
            Some(security_context),
            super::execution::normalize_operation_name(operation_name),
        )
        .await
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
        // started_at must come first
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].0, fraiseql_db::STARTED_AT_VAR);
        // It carries the clock-timestamp directive (DB-clock-stamped at apply
        // time), NOT an app-clock literal — see resolve_session_variables.
        assert_eq!(vars[0].1, fraiseql_db::CLOCK_TIMESTAMP_DIRECTIVE);
        assert_eq!(vars[1].0, "app.locale");
    }

    #[test]
    fn inject_started_at_disabled() {
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables:         vec![],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
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
        let vars = resolve_session_variables(&config, &ctx).unwrap();
        assert!(vars.is_empty(), "missing email should be silently skipped");
    }

    #[test]
    fn resolve_session_variables_enrichment_reads_namespace() {
        let mut ctx = make_context();
        // A resolved enriched field, merged by the server under the reserved
        // namespace (the extractor strips `fraiseql.` claims, so a token can't
        // forge this key).
        ctx.attributes
            .insert("fraiseql.enriched.actor_role".to_string(), serde_json::json!("manager"));
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.actor_role".to_string(),
                source: SessionVariableSource::Enrichment {
                    field: "actor_role".to_string(),
                },
            }],
            inject_started_at: false,
        };
        let vars = resolve_session_variables(&config, &ctx).unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "app.actor_role");
        assert_eq!(vars[0].1, "manager");
    }

    #[test]
    fn resolve_session_variables_enrichment_missing_field_errors() {
        // Enrichment declared but the field is absent from the namespace: a
        // hard error, never a silently-skipped/empty GUC (DESIGN §3.2, §5.2).
        let ctx = make_context();
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.actor_role".to_string(),
                source: SessionVariableSource::Enrichment {
                    field: "actor_role".to_string(),
                },
            }],
            inject_started_at: false,
        };
        assert!(resolve_session_variables(&config, &ctx).is_err());
    }

    #[test]
    fn resolve_session_variables_enrichment_does_not_fall_back_to_raw_claim() {
        // A raw claim of the same name is present in attributes, but the
        // Enrichment source reads ONLY the reserved namespace — so it must still
        // fail, never impersonate a DB-derived field with an attacker-influenced
        // claim (the security property `Enrichment` exists for).
        let mut ctx = make_context();
        ctx.attributes.insert("actor_role".to_string(), serde_json::json!("admin"));
        let config = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.actor_role".to_string(),
                source: SessionVariableSource::Enrichment {
                    field: "actor_role".to_string(),
                },
            }],
            inject_started_at: false,
        };
        assert!(
            resolve_session_variables(&config, &ctx).is_err(),
            "Enrichment must not fall back to a raw JWT claim"
        );
    }
}

// ── #1336: the backstop fires at every engine entry, not only in isolation ──
//
// `enforce_enrichment_resolved` has its own unit tests; these assert it is actually
// *reached* from each family of entry point. The distinction matters: the defect this
// guards against is a call site that does not consult a rule, so testing the rule
// without testing the sites would reproduce the original failure exactly.
#[cfg(test)]
mod enrichment_entry_point_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use chrono::Utc;

    use super::super::mutation::any_write_selections;
    use crate::{
        error::FraiseQLError,
        runtime::{Executor, executor::test_support::MockAdapter},
        schema::{
            CompiledSchema, MutationDefinition, QueryDefinition, SessionVariableMapping,
            SessionVariableSource, TypeDefinition,
        },
        security::{EnrichmentMark, SecurityContext},
    };

    /// A schema that reads enriched identity, with one query and one mutation so
    /// every entry family has something to dispatch to.
    fn schema() -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        schema.session_variables.variables.push(SessionVariableMapping {
            name:   "app.actor_id".to_string(),
            source: SessionVariableSource::Enrichment {
                field: "actor_id".to_string(),
            },
        });

        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields = vec![crate::schema::FieldDefinition::new(
            "id",
            crate::schema::FieldType::Id,
        )];
        schema.types.push(order);

        let mut query = QueryDefinition::new("orders", "Order");
        query.sql_source = Some("v_order".to_string());
        query.returns_list = true;
        schema.queries.push(query);

        let mut create = MutationDefinition::new("createOrder", "Order");
        create.sql_source = Some("fn_create_order".to_string());
        schema.mutations.push(create);

        schema.build_indexes();
        schema
    }

    fn executor() -> Executor {
        Executor::new(schema(), Arc::new(MockAdapter::new(vec![])))
    }

    /// A principal exactly as a transport that never resolved would produce it.
    fn unresolved() -> SecurityContext {
        SecurityContext {
            user_id:          crate::types::UserId::new("user-1336"),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::new(),
            request_id:       "req-1336".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn resolved() -> SecurityContext {
        let mut ctx = unresolved();
        ctx.mark_enrichment(EnrichmentMark::Resolved);
        ctx
    }

    fn is_refusal(err: &FraiseQLError) -> bool {
        matches!(err, FraiseQLError::Authorization { .. })
    }

    #[tokio::test]
    async fn the_graphql_document_entry_refuses_an_unresolved_principal() {
        // `execute_with_security` is how `fraiseql-arrow`'s Flight server reaches the
        // engine, from a crate that cannot see the resolver at all.
        let err = executor()
            .execute_with_security("{ orders { id } }", None, &unresolved())
            .await
            .expect_err("an unresolved principal must not execute");

        assert!(is_refusal(&err), "expected an authorization refusal, got: {err}");
    }

    #[tokio::test]
    async fn the_graphql_document_entry_admits_a_resolved_principal() {
        // The twin that keeps the case above honest: this query fails for its own
        // reasons against a mock adapter, but it must not fail as a *refusal*.
        let outcome =
            executor().execute_with_security("{ orders { id } }", None, &resolved()).await;

        assert!(
            outcome.as_ref().err().is_none_or(|e| !is_refusal(e)),
            "a resolved principal must get past this guard; got: {outcome:?}"
        );
    }

    #[tokio::test]
    async fn the_direct_read_entries_refuse_an_unresolved_principal() {
        // REST does not go through the GraphQL document path, so the guard has to be
        // at these entries too — the gap that let #808 and #739 ship.
        let executor = executor();
        // A real match, built the way REST builds one — route resolution first, then
        // the pre-resolved `QueryMatch` handed straight to the executor.
        let query_match = crate::runtime::QueryMatcher::new(schema())
            .match_query("{ orders { id } }", None)
            .expect("the fixture query matches");

        let read = executor.execute_query_direct(&query_match, None, Some(&unresolved())).await;
        assert!(
            read.as_ref().err().is_some_and(is_refusal),
            "execute_query_direct must refuse; got: {read:?}"
        );

        let count = executor.count_rows(&query_match, None, Some(&unresolved())).await;
        assert!(
            count.as_ref().err().is_some_and(is_refusal),
            "count_rows must refuse too — it is the second chokepoint every REST read \
             passes through, and a guard on one of the pair leaves the other open; \
             got: {count:?}"
        );
    }

    #[tokio::test]
    async fn the_streaming_read_entry_refuses_an_unresolved_principal() {
        // Found by mutating the guard away: with only the two non-streaming read cases
        // above, this call site could be deleted and the suite stayed green. REST's
        // NDJSON/CSV/XLSX routes are the ones that reach it, and they carry exactly the
        // same principal the JSON route does.
        let executor = executor();
        let query_match = crate::runtime::QueryMatcher::new(schema())
            .match_query("{ orders { id } }", None)
            .expect("the fixture query matches");

        let stream = executor.stream_query_direct(query_match, None, Some(unresolved())).await;

        assert!(
            stream.as_ref().err().is_some_and(is_refusal),
            "stream_query_direct must refuse an unresolved principal too; got an Ok or a \
             non-authorization error"
        );
    }

    #[tokio::test]
    async fn the_mutation_chokepoint_refuses_an_unresolved_principal() {
        let err = executor()
            .execute_mutation_as("createOrder", None, Some(&unresolved()), any_write_selections())
            .await
            .expect_err("an unresolved principal must not write");

        assert!(is_refusal(&err), "expected an authorization refusal, got: {err}");
    }
}
