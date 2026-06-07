//! Regular (non-relay) query execution methods for [`QueryRunner`].

use std::sync::Arc;

use tracing::debug;

use super::{
    super::{null_masked_fields, resolve_inject_value},
    query::QueryRunner,
    query_params::{
        combine_explicit_arg_where, compute_projection_reduction, enforce_max_page_size,
        inject_param_where_clause,
    },
    query_projection::{build_typed_projection_fields, enrich_order_by_clauses},
};
use crate::{
    db::{WhereClause, projection_generator::PostgresProjectionGenerator, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    runtime::{JsonbStrategy, ResultProjector},
    schema::SqlProjectionHint,
    security::{RlsWhereClause, SecurityContext},
};

impl<A: DatabaseAdapter> QueryRunner<A> {
    /// Resolve configured session variables for `security_context` into owned
    /// `(name, value)` pairs.
    ///
    /// The caller borrows these into a `&[(&str, &str)]` slice for the
    /// connection-affine `*_with_session` adapter methods, which apply them
    /// transaction-locally on the same connection as the read (fixes #329 for
    /// RLS policies backed by `current_setting()`).
    ///
    /// Returns an empty vec when there is no security context or no session
    /// variables are configured; the adapter treats an empty slice as "no
    /// session variables" with zero overhead.
    fn resolve_session_vars(
        &self,
        security_context: Option<&SecurityContext>,
    ) -> Vec<(String, String)> {
        let sv = &self.ctx.schema.session_variables;
        match security_context {
            Some(sec) if !sv.variables.is_empty() || sv.inject_started_at => {
                crate::runtime::executor::security::resolve_session_variables(sv, sec)
            },
            _ => Vec::new(),
        }
    }

    /// Execute a regular query with row-level security (RLS) filtering.
    ///
    /// This method:
    /// 1. Validates the user's security context (token expiration, etc.)
    /// 2. Evaluates RLS policies to determine what rows the user can access
    /// 3. Composes RLS filters with user-provided WHERE clauses
    /// 4. Passes the composed filter to the database adapter for SQL-level filtering
    ///
    /// RLS filtering happens at the database level, not in Rust, ensuring:
    /// - High performance (database can optimize filters)
    /// - Correct handling of pagination (LIMIT applied after RLS filtering)
    /// - Type-safe composition via `WhereClause` enum
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — security token expired, role check failed, or query not
    ///   found in schema.
    /// * [`FraiseQLError::Database`] — the database adapter returned an error.
    pub(in super::super) async fn execute_regular_query_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<serde_json::Value> {
        // 1. Validate security context (check expiration, etc.)
        if security_context.is_expired() {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }

        // 2. Match query to compiled template
        let query_match = self.ctx.matcher.match_query(query, variables)?;

        // 2b. Enforce requires_role — return "not found" (not "forbidden") to prevent enumeration
        if let Some(ref required_role) = query_match.query_def.requires_role {
            if !security_context.roles.iter().any(|r| r == required_role) {
                return Err(FraiseQLError::Validation {
                    message: format!("Query '{}' not found in schema", query_match.query_def.name),
                    path:    None,
                });
            }
        }

        // Resolve session variables once. They are applied transaction-locally
        // on the same connection as the read (fixes #329) by passing them into
        // the connection-affine adapter call below, so PostgreSQL RLS policies
        // that read `current_setting()` (e.g. `app.tenant_id`) are effective.
        let resolved_session_vars = self.resolve_session_vars(Some(security_context));
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        // Route relay queries to dedicated handler with security context.
        if query_match.query_def.relay {
            return self
                .execute_relay_query(
                    &query_match,
                    variables,
                    Some(security_context),
                    &session_pairs,
                )
                .await;
        }

        // 0a. Detect whether a policy-gated field (#423) is selected (top-level or
        //     nested). When so, the per-row dynamic authorizer decision is neither
        //     cacheable (D5b) nor compatible with a selection-stripped row, so the
        //     response cache and the SQL projection hint are both bypassed below.
        let root_fields: &[crate::graphql::FieldSelection] =
            query_match.selections.first().map_or(&[], |r| r.nested_fields.as_slice());
        let gated_present = crate::security::field_authorizer::selection_set_selects_gated_field(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            root_fields,
        );

        // 0. Check response cache (skips all projection/RBAC/serialization work on hit)
        let response_cache_key = if !gated_present
            && self.ctx.response_cache.as_ref().is_some_and(|rc| rc.is_enabled())
        {
            let query_key = Self::compute_response_cache_key(&query_match)?;
            let sec_hash =
                crate::cache::response_cache::hash_security_context(Some(security_context));
            Some((query_key, sec_hash))
        } else {
            None
        };

        if let (Some((query_key, sec_hash)), Some(rc)) =
            (response_cache_key, self.ctx.response_cache.as_ref())
        {
            if let Some(cached) = rc.get(query_key, sec_hash)? {
                // F040: explicit hit event so operators can correlate slow
                // requests with cache state from logs alone.
                debug!(
                    target: "fraiseql::cache::response",
                    event = "hit",
                    query = %query_match.query_def.name,
                    query_key,
                    sec_hash,
                    "response cache hit"
                );
                // F002: `Arc::unwrap_or_clone` takes ownership when the cache
                // entry is uniquely held (the common case once moka has
                // returned an `Arc::clone`), avoiding the recursive deep
                // clone of every JSON node. The fallback clone only fires
                // when another reader is racing on the same key.
                return Ok(Arc::unwrap_or_clone(cached));
            }
            // F040: miss → DB execution will run below. Emit before the
            // expensive plan/projection work so the event timestamps the
            // start of the slow path.
            debug!(
                target: "fraiseql::cache::response",
                event = "miss",
                query = %query_match.query_def.name,
                query_key,
                sec_hash,
                "response cache miss"
            );
        } else {
            debug!(
                target: "fraiseql::cache::response",
                event = "disabled",
                query = %query_match.query_def.name,
                "response cache disabled or no key available"
            );
        }

        // 3. Create execution plan
        let plan = self.ctx.planner.plan(&query_match)?;

        // 4. Evaluate RLS policy and build WHERE clause filter. The return type is
        //    Option<RlsWhereClause> — a compile-time proof that the clause passed through RLS
        //    evaluation.
        let rls_where_clause: Option<RlsWhereClause> =
            if let Some(ref rls_policy) = self.ctx.config.rls_policy {
                // Evaluate RLS policy with user's security context
                rls_policy.evaluate(security_context, &query_match.query_def.name)?
            } else {
                // No RLS policy configured, allow all access
                None
            };

        // 5. Get SQL source from query definition
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 6. Generate SQL projection hint for requested fields (optimization). Build a recursive
        //    ProjectionField tree from the selection set so that composite sub-fields are projected
        //    with nested jsonb_build_object instead of returning the full blob. When a policy-gated
        //    field is selected (#423), the hint is skipped so the adapter returns the full row,
        //    giving the field authorizer the complete `parent` to decide on.
        let projection_hint = if !gated_present
            && !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let root_fields = query_match
                .selections
                .first()
                .map_or(&[] as &[_], |s| s.nested_fields.as_slice());
            let typed_fields = build_typed_projection_fields(
                root_fields,
                &self.ctx.schema,
                &query_match.query_def.return_type,
                0,
            );

            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint::new(
                self.ctx.adapter.database_type(),
                projection_sql,
                compute_projection_reduction(plan.projection_fields.len()),
            ))
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 7. AND inject conditions onto the RLS WHERE clause. Inject conditions always come after
        //    RLS so they cannot bypass it.
        let combined_where: Option<WhereClause> = if query_match.query_def.inject_params.is_empty()
        {
            // Common path: unwrap RlsWhereClause into WhereClause for the adapter
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let mut conditions: Vec<WhereClause> = query_match
                .query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value = resolve_inject_value(col, source, security_context)?;
                    Ok(inject_param_where_clause(col, value, &query_match.query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // 5b. Compose user-supplied WHERE from GraphQL arguments when has_where is enabled.
        //     Security conditions (RLS + inject) are always first so they cannot be bypassed.
        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            let user_where = query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?;
            match (combined_where, user_where) {
                (None, None) => None,
                (Some(sec), None) => Some(sec),
                (None, Some(user)) => Some(user),
                (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
            }
        } else {
            combined_where
        };

        // 5c. Convert explicit query arguments (e.g. id, slug) to WHERE conditions.
        //     This handles single-entity lookups like `user(id: "...")` where the
        //     arguments are direct equality filters, not the structured `where` argument.
        let combined_where = combine_explicit_arg_where(
            combined_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // 8. Extract limit/offset from query arguments when auto_params are enabled
        // The top-level page size is capped (#421: unbounded-pagination DoS guard).
        let limit = enforce_max_page_size(
            if query_match.query_def.auto_params.has_limit {
                query_match
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
            } else {
                None
            },
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = if query_match.query_def.auto_params.has_offset {
            query_match
                .arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        // 8b. Extract order_by from query arguments when has_order_by is enabled,
        //     then enrich each clause with the schema field type so the SQL generator
        //     emits correct type casts (e.g., `(data->>'amount')::numeric`).
        let order_by_clauses = if query_match.query_def.auto_params.has_order_by {
            query_match
                .arguments
                .get("orderBy")
                .map(crate::db::OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.ctx.schema,
                        &query_match.query_def.return_type,
                        &query_match.query_def.native_columns,
                    )
                })
        } else {
            None
        };

        // 9. Execute query with combined WHERE clause filter, pinning session variables to the
        //    read's connection (fixes #329 for RLS).
        let results = self
            .ctx
            .adapter
            .execute_with_projection_arc_with_session(
                &crate::db::ProjectionRequest {
                    view: sql_source,
                    projection: projection_hint.as_ref(),
                    where_clause: combined_where.as_ref(),
                    order_by: order_by_clauses.as_deref(),
                    limit,
                    offset,
                },
                &session_pairs,
            )
            .await?;

        // 10. Apply field-level RBAC filtering (reject / mask / allow)
        let access = super::super::support::security::apply_field_rbac_filtering(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 11. Project results — include both allowed and masked fields in projection
        let mut all_projection_fields = access.allowed;
        all_projection_fields.extend(access.masked.iter().cloned());
        let projector = ResultProjector::new(all_projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;

        // 11. Null out masked fields in the projected result
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        // 11c. Apply the dynamic field authorizer (#423) per row. The static gate (step
        //      10) ran first — AND-composition: a field shown only if both allow. Fail-closed:
        //      a Reject decision or any policy error returns 403; the value is never served.
        if gated_present {
            use crate::security::field_authorizer as authz;

            let return_type = &query_match.query_def.return_type;
            // A gated field is selected but no authorizer is configured → fail closed.
            let Some(authorizer) = self.ctx.config.field_authorizer.as_ref() else {
                return Err(FraiseQLError::Authorization {
                    message:  format!(
                        "Field-level authorization is required for a selected field on type \
                         '{return_type}' but no field authorizer is configured"
                    ),
                    action:   Some("read".to_string()),
                    resource: Some(return_type.clone()),
                });
            };
            // This version enforces only top-level entity-row fields; a gated field nested
            // inside a sub-selection is fail-closed (tracked follow-up: extend to nesting).
            if authz::selection_set_has_nested_gated_field(
                &self.ctx.schema,
                return_type,
                root_fields,
            ) {
                return Err(FraiseQLError::Authorization {
                    message:  format!(
                        "Field-level authorization of nested fields on type '{return_type}' is \
                         not supported in this version"
                    ),
                    action:   Some("read".to_string()),
                    resource: Some(return_type.clone()),
                });
            }
            let gated =
                authz::collect_top_level_gated_fields(&self.ctx.schema, return_type, root_fields);
            let pass = authz::FieldAuthzPass {
                authorizer:        authorizer.as_ref(),
                principal:         security_context,
                type_name:         return_type,
                gated:             &gated,
                statically_masked: &access.masked,
            };
            authz::apply_field_authorizer(
                &pass,
                &results,
                &mut projected,
                query_match.query_def.returns_list,
            )?;
        }

        // 12. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 13. Store in response cache (if enabled) and return value.
        //
        // F002: wrap once in `Arc`, hand the `Arc` to the cache, and
        // `unwrap_or_clone` for the return path. When no other reader has
        // touched the entry yet, the unwrap is free and the only cost is
        // the original `Arc::new` heap allocation — replacing the previous
        // pattern that deep-cloned the projected JSON to satisfy both the
        // cache and the return type.
        if let (Some((query_key, sec_hash)), Some(rc)) =
            (response_cache_key, self.ctx.response_cache.as_ref())
        {
            let sql_source = query_match.query_def.sql_source.as_deref().unwrap_or("");
            let cached = Arc::new(response);
            let _ = rc.put(query_key, sec_hash, Arc::clone(&cached), vec![sql_source.to_string()]);
            return Ok(Arc::unwrap_or_clone(cached));
        }

        Ok(response)
    }

    /// Compute a response cache key from a query match.
    ///
    /// Hashes the query name, matched fields, and arguments to produce
    /// a u64 key. Combined with the security context hash, this forms
    /// the full response cache key.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when any argument value fails
    /// JSON serialization. The previous implementation silently collapsed
    /// failures to an empty string, which could cause two distinct argument
    /// trees to map to the same cache key (F044).
    fn compute_response_cache_key(
        query_match: &crate::runtime::matcher::QueryMatch,
    ) -> Result<u64> {
        use std::hash::{Hash, Hasher};
        let mut hasher = ahash::AHasher::default();
        query_match.query_def.name.hash(&mut hasher);
        for field in &query_match.fields {
            field.hash(&mut hasher);
        }
        // Hash arguments (sorted keys for determinism)
        let mut keys: Vec<&String> = query_match.arguments.keys().collect();
        keys.sort();
        // F044: stream the serialized JSON straight into the hasher via a
        // scratch buffer; this avoids the intermediate `String` allocation
        // *and* satisfies clippy's `collection_is_never_read` lint (the prior
        // `let serialized = ...; serialized.hash(...)` shape was flagged).
        let mut scratch: Vec<u8> = Vec::new();
        for key in keys {
            key.hash(&mut hasher);
            scratch.clear();
            serde_json::to_writer(&mut scratch, &query_match.arguments[key]).map_err(|e| {
                FraiseQLError::Validation {
                    message: format!(
                        "failed to serialize argument '{key}' for response cache key: {e}"
                    ),
                    path:    Some(format!("arguments.{key}")),
                }
            })?;
            scratch.hash(&mut hasher);
        }
        Ok(hasher.finish())
    }

    /// Execute a regular (non-aggregate, non-relay) GraphQL query.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the query does not match a compiled
    /// template or requires a security context that is not present.
    /// Returns [`FraiseQLError::Database`] if the SQL execution or result projection fails.
    pub(in super::super) async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // 1. Match query to compiled template
        let query_match = self.ctx.matcher.match_query(query, variables)?;

        // Guard: role-restricted queries are invisible to unauthenticated users
        if query_match.query_def.requires_role.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", query_match.query_def.name),
                path:    None,
            });
        }

        // Guard: queries with inject params require a security context.
        if !query_match.query_def.inject_params.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_match.query_def.name
                ),
                path:    None,
            });
        }

        // Route relay queries to dedicated handler.
        // No session vars: unauthenticated entrypoint (no SecurityContext). See #329.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, variables, None, &[]).await;
        }

        // #423: the unauthenticated path has no principal, so a selected policy-gated
        // field cannot be authorized — fail closed.
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
        crate::security::field_authorizer::deny_if_gated_field_selected(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            root_fields,
            "unauthenticated query",
        )?;

        // 2. Create execution plan
        let plan = self.ctx.planner.plan(&query_match)?;

        // 3. Execute SQL query
        let sql_source = query_match.query_def.sql_source.as_ref().ok_or_else(|| {
            crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            }
        })?;

        // 3a. Generate SQL projection hint for requested fields (optimization).
        //     Recursive typed projection: composite sub-fields are projected with nested
        //     jsonb_build_object instead of returning the full blob.
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let root_fields = query_match
                .selections
                .first()
                .map_or(&[] as &[_], |s| s.nested_fields.as_slice());
            let typed_fields = build_typed_projection_fields(
                root_fields,
                &self.ctx.schema,
                &query_match.query_def.return_type,
                0,
            );
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint::new(
                self.ctx.adapter.database_type(),
                projection_sql,
                compute_projection_reduction(plan.projection_fields.len()),
            ))
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 3b. Extract auto_params (limit, offset, where, order_by) from arguments
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // 3c. Convert explicit query arguments (e.g. id, slug) to WHERE conditions.
        let user_where = combine_explicit_arg_where(
            user_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // The top-level page size is capped (#421: unbounded-pagination DoS guard).
        let limit = enforce_max_page_size(
            if query_match.query_def.auto_params.has_limit {
                query_match
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
            } else {
                None
            },
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = if query_match.query_def.auto_params.has_offset {
            query_match
                .arguments
                .get("offset")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
        } else {
            None
        };

        let order_by_clauses = if query_match.query_def.auto_params.has_order_by {
            query_match
                .arguments
                .get("orderBy")
                .map(crate::db::OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.ctx.schema,
                        &query_match.query_def.return_type,
                        &query_match.query_def.native_columns,
                    )
                })
        } else {
            None
        };

        // No session vars: this is the unauthenticated entrypoint (no
        // SecurityContext), so there is nothing to resolve session variables
        // from. See #329 / resolve_session_vars.
        let results = self
            .ctx
            .adapter
            .execute_with_projection_arc(&crate::db::ProjectionRequest {
                view: sql_source,
                projection: projection_hint.as_ref(),
                where_clause: user_where.as_ref(),
                order_by: order_by_clauses.as_deref(),
                limit,
                offset,
            })
            .await?;

        // 4. Project results
        let projector = ResultProjector::new(plan.projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 5. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 6. Serialize to JSON string
        Ok(response)
    }

    /// Execute a pre-built `QueryMatch` directly, bypassing GraphQL string parsing.
    ///
    /// Used by the REST transport for embedded sub-queries and NDJSON streaming
    /// where the query parameters are already resolved from HTTP request parameters.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub(in super::super) async fn execute_query_direct(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        _variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // #423: the REST direct projection path does not run per-row field
        // authorization; fail closed if a policy-gated field is selected.
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
        crate::security::field_authorizer::deny_if_gated_field_selected(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            root_fields,
            "REST",
        )?;

        // Evaluate RLS policy if present.
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.ctx.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_match.query_def.name)?
        } else {
            None
        };

        // Get SQL source.
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // Build execution plan.
        let plan = self.ctx.planner.plan(query_match)?;

        // Extract auto_params from arguments.
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // The top-level page size is capped (#421: unbounded-pagination DoS guard).
        let limit = enforce_max_page_size(
            query_match
                .arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok()),
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = query_match
            .arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok());

        let order_by_clauses = query_match
            .arguments
            .get("orderBy")
            .map(crate::db::OrderByClause::from_graphql_json)
            .transpose()?
            .map(|clauses| {
                enrich_order_by_clauses(
                    clauses,
                    &self.ctx.schema,
                    &query_match.query_def.return_type,
                    &query_match.query_def.native_columns,
                )
            });

        // Convert explicit arguments to WHERE conditions.
        let user_where = combine_explicit_arg_where(
            user_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // Compose RLS and user WHERE clauses.
        let composed_where = match (&rls_where_clause, &user_where) {
            (Some(rls), Some(user)) => {
                Some(WhereClause::And(vec![rls.as_where_clause().clone(), user.clone()]))
            },
            (Some(rls), None) => Some(rls.as_where_clause().clone()),
            (None, Some(user)) => Some(user.clone()),
            (None, None) => None,
        };

        // Inject security-derived params.
        if !query_match.query_def.inject_params.is_empty() {
            if let Some(ctx) = security_context {
                for (param_name, source) in &query_match.query_def.inject_params {
                    let _value = resolve_inject_value(param_name, source, ctx)?;
                    // Injected params are applied at the SQL level via WHERE clauses,
                    // not via GraphQL variables, so no mutation of variables is needed here.
                }
            }
        }

        // Execute, pinning session variables to the read's connection (#329).
        let resolved_session_vars = self.resolve_session_vars(security_context);
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let results = self
            .ctx
            .adapter
            .execute_with_projection_arc_with_session(
                &crate::db::ProjectionRequest {
                    view: sql_source,
                    projection: None,
                    where_clause: composed_where.as_ref(),
                    order_by: order_by_clauses.as_deref(),
                    limit,
                    offset,
                },
                &session_pairs,
            )
            .await?;

        // Project results.
        let projector = ResultProjector::new(plan.projection_fields)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // Wrap in GraphQL data envelope.
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        Ok(response)
    }

    /// Count the total number of rows matching the query's WHERE and RLS conditions.
    ///
    /// Issues a `SELECT COUNT(*) FROM {view} WHERE {conditions}` query, ignoring
    /// pagination (ORDER BY, LIMIT, OFFSET). Useful for REST `X-Total-Count` headers
    /// and `count=exact` query parameter support.
    ///
    /// # Arguments
    ///
    /// * `query_match` - Pre-built query match identifying the SQL source and filters
    /// * `variables` - Optional variables (unused for count, reserved for future use)
    /// * `security_context` - Optional authenticated user context for RLS and inject
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source, or if
    /// inject params are required but no security context is provided.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub(in super::super) async fn count_rows(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        _variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<u64> {
        // 1. Evaluate RLS policy
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.ctx.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_match.query_def.name)?
        } else {
            None
        };

        // 2. Get SQL source
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 3. Build combined WHERE clause (RLS + inject)
        let combined_where: Option<WhereClause> = if query_match.query_def.inject_params.is_empty()
        {
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let ctx = security_context.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but no security context is available",
                    query_match.query_def.name
                ),
                path:    None,
            })?;
            let mut conditions: Vec<WhereClause> = query_match
                .query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value = resolve_inject_value(col, source, ctx)?;
                    Ok(inject_param_where_clause(col, value, &query_match.query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // 3b. Compose user-supplied WHERE when has_where is enabled (same as execute_from_match).
        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            let user_where = query_match
                .arguments
                .get("where")
                .map(WhereClause::from_graphql_json)
                .transpose()?;
            match (combined_where, user_where) {
                (None, None) => None,
                (Some(sec), None) => Some(sec),
                (None, Some(user)) => Some(user),
                (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
            }
        } else {
            combined_where
        };

        // 4. Execute COUNT query via adapter, pinning session variables to the read's connection so
        //    RLS counts match the filtered rows (#329).
        let resolved_session_vars = self.resolve_session_vars(security_context);
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let rows = self
            .ctx
            .adapter
            .execute_where_query_arc_with_session(
                sql_source,
                combined_where.as_ref(),
                None,
                None,
                None,
                &session_pairs,
            )
            .await?;

        // Return the row count
        #[allow(clippy::cast_possible_truncation)] // Reason: row count fits u64
        Ok(rows.len() as u64)
    }
}
