//! Core query execution — `execute()`, the shared `execute_dispatch()`, and
//! `execute_with_scopes()`.

use std::{sync::Arc, time::Duration};

use super::{Executor, QueryType, pipeline, root_type_name, support};
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    security::{QueryValidator, SecurityContext},
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a GraphQL query string and return a serialized JSON response.
    ///
    /// Applies the configured query timeout if one is set. Handles queries,
    /// mutations, introspection, federation, and node lookups.
    ///
    /// If `RuntimeConfig::query_validation` is set, `QueryValidator::validate()`
    /// runs first (before parsing or SQL dispatch) to enforce size, depth, and
    /// complexity limits. This protects direct `fraiseql-core` embedders that do
    /// not route through `fraiseql-server`.
    ///
    /// # Errors
    ///
    /// - `FraiseQLError::Validation` — query violates configured depth/complexity/alias limits
    ///   (only when `RuntimeConfig::query_validation` is `Some`).
    /// - `FraiseQLError::Timeout` — query exceeded `RuntimeConfig::query_timeout_ms`.
    /// - Any error returned by `execute_dispatch`.
    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Anonymous entry: no principal. GATE-1, the parse cache, multi-root
        // fan-out, and dispatch all live in the shared `execute_dispatch`.
        self.execute_with_timeout(query, variables, None).await
    }

    /// Apply the configured query timeout (if any) around `execute_dispatch`.
    ///
    /// Shared by the anonymous [`execute`](Self::execute) and the authenticated
    /// `execute_with_security` entry points so both honor `query_timeout_ms`
    /// identically.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Timeout`] — execution exceeded `query_timeout_ms`.
    /// - Any error returned by [`execute_dispatch`](Self::execute_dispatch).
    pub(super) async fn execute_with_timeout(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        if self.ctx.config.query_timeout_ms > 0 {
            let timeout_duration = Duration::from_millis(self.ctx.config.query_timeout_ms);
            tokio::time::timeout(
                timeout_duration,
                self.execute_dispatch(query, variables, security_context),
            )
            .await
            .map_err(|_| {
                // Truncate query (char-boundary-safe) for error reporting.
                let query_snippet = crate::utils::text::truncate_for_display(query, 100);
                FraiseQLError::Timeout {
                    timeout_ms: self.ctx.config.query_timeout_ms,
                    query:      Some(query_snippet),
                }
            })?
        } else {
            self.execute_dispatch(query, variables, security_context).await
        }
    }

    /// Unified query dispatch for both the anonymous and authenticated entry
    /// points (H19). `security_context` is `None` for anonymous requests and
    /// `Some` for authenticated ones; it threads through GATE-1, the parse
    /// cache, the multi-root fan-out, and every per-operation runner so the two
    /// paths cannot diverge in which roots they return, whether GATE-1 runs, or
    /// whether the parse cache is consulted.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Validation`] — GATE-1 limits exceeded, or a runner rejects.
    /// - [`FraiseQLError::Parse`] — GraphQL query string is not valid GraphQL syntax.
    /// - [`FraiseQLError::NotFound`] — the query name does not match any compiled query template.
    /// - [`FraiseQLError::Database`] — the underlying database returned an error.
    /// - [`FraiseQLError::Internal`] — response serialisation failed.
    /// - [`FraiseQLError::Authorization`] — field-level access control denied a field.
    pub(super) async fn execute_dispatch(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // GATE 1: query-structure validation (DoS protection for direct embedders).
        // Runs on BOTH the anonymous and authenticated paths (L-gate1-skip).
        if let Some(ref cfg) = self.ctx.config.query_validation {
            QueryValidator::from_config(cfg.clone()).validate(query).map_err(|e| {
                FraiseQLError::Validation {
                    message: e.to_string(),
                    path:    Some("query".to_string()),
                }
            })?;
        }

        // 1. Classify query type — also returns the ParsedQuery for Regular
        // queries so we do not parse the same string twice.
        //
        // The parse result is memoised in `parse_cache` (keyed by xxHash64 of
        // the query string) so repeated identical queries skip re-parsing — on
        // both the anonymous and authenticated paths (L-parse-cache).
        let cache_key = xxhash_rust::xxh3::xxh3_64(query.as_bytes());
        let (query_type, maybe_parsed) = if let Some(arc) = self.ctx.parse_cache.get(&cache_key) {
            arc.as_ref().clone()
        } else {
            let pair = self.classify_query_with_parse(query)?;
            self.ctx.parse_cache.insert(cache_key, Arc::new(pair.clone()));
            pair
        };

        // 1b. Operation-level authorization (#422). Runs before single- AND
        //     multi-root dispatch so a deny short-circuits the parallel pipeline.
        //     Mutations are gated downstream at `execute_mutation_impl`.
        //     Fail-closed: a `Deny` or any policy error → 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = support::authz::collect_authz_ops(&query_type, maybe_parsed.as_ref());
            crate::security::authorizer::enforce_authz(
                authorizer.as_ref(),
                security_context,
                &ops,
                variables,
            )?;
        }

        // 2. Route to appropriate handler, threading the (optional) principal.
        match query_type {
            QueryType::Regular => {
                // Detect multi-root queries and dispatch them in parallel.
                // `maybe_parsed` is always Some for Regular queries (see
                // classify_query_with_parse).
                let parsed = maybe_parsed.ok_or_else(|| FraiseQLError::Internal {
                    message: "classifier returned Regular without a parsed query — this is a bug"
                        .to_string(),
                    source:  None,
                })?;
                if pipeline::is_multi_root(&parsed) {
                    let pr = self.execute_parallel(&parsed, variables, security_context).await?;
                    let data = pr.merge_into_data_map();
                    return Ok(serde_json::json!({ "data": data }));
                }
                self.query_runner()
                    .execute_regular_query_maybe_security(query, variables, security_context)
                    .await
            },
            QueryType::Aggregate(query_name) => {
                self.aggregate_runner()
                    .execute_aggregate_dispatch(&query_name, variables, security_context)
                    .await
            },
            QueryType::Window(query_name) => {
                self.aggregate_runner()
                    .execute_window_dispatch(&query_name, variables, security_context)
                    .await
            },
            #[cfg(feature = "federation")]
            QueryType::Federation(query_name) => {
                // The `_entities` path fails closed for RLS-/inject-/role-gated
                // types when `security_context` is `None` (C1b).
                self.execute_federation_query(&query_name, query, variables, security_context)
                    .await
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
                // Return pre-built __schema response (zero-cost at runtime)
                Ok(self.ctx.introspection.schema_response.as_ref().clone())
            },
            QueryType::IntrospectionType(type_name) => {
                // Return pre-built __type response (zero-cost at runtime)
                Ok(self.ctx.introspection.get_type_response(&type_name))
            },
            QueryType::Mutation {
                name,
                selections,
                arguments,
            } => {
                self.execute_mutation_query(
                    &name,
                    variables,
                    security_context,
                    &selections,
                    &arguments,
                )
                .await
            },
            QueryType::NodeQuery { selections } => {
                // The node runner fails closed for any RLS/inject/role-gated type
                // when `security_context` is `None` (H2 IDOR fix).
                self.query_runner()
                    .execute_node_query(query, variables, &selections, security_context)
                    .await
            },
            QueryType::TypeName {
                response_key,
                operation_type,
            } => {
                // Root `__typename` meta-field: resolve to the operation's root
                // type name with no DB round-trip (spec §"Type Name Introspection").
                let ty = root_type_name(&operation_type);
                Ok(serde_json::json!({ "data": { response_key: ty } }))
            },
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
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — query validation fails, or the user's scopes do not
    ///   include a field required by the `field_filter` policy.
    /// * Propagates errors from query classification and execution.
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
    ) -> Result<serde_json::Value> {
        // GATE 1: Query structure validation (mirrors execute() — DoS protection).
        if let Some(ref cfg) = self.ctx.config.query_validation {
            QueryValidator::from_config(cfg.clone()).validate(query).map_err(|e| {
                FraiseQLError::Validation {
                    message: e.to_string(),
                    path:    Some("query".to_string()),
                }
            })?;
        }

        // 2. Classify query type
        let query_type = self.classify_query(query)?;

        // 3. Validate field access if filter is configured
        if let Some(ref filter) = self.ctx.config.field_filter {
            // Only validate for regular queries (not introspection)
            if matches!(query_type, QueryType::Regular) {
                self.validate_field_access(query, variables, user_scopes, filter)?;
            }
        }

        // 4. Delegate to execute_dispatch — single source of routing truth. Field-access validation
        //    (step 3) has already run for Regular queries; all other query types (introspection,
        //    aggregate, federation, …) are routed correctly via execute_dispatch without
        //    duplication. Scope-based filtering is not RLS, so no SecurityContext is threaded.
        self.execute_dispatch(query, variables, None).await
    }
}
