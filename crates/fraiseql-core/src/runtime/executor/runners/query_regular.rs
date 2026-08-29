//! Regular (non-relay) query execution methods for [`QueryRunner`].

use std::sync::Arc;

use futures::StreamExt as _;
use tracing::debug;

use super::{
    super::{null_masked_fields, resolve_inject_value},
    query::QueryRunner,
    query_params::{
        coerce_pagination_arg, combine_explicit_arg_where, compute_projection_reduction,
        enforce_max_page_size, inject_param_where_clause, nearest_order_and_limit,
    },
    query_projection::{
        build_typed_projection_fields, enrich_order_by_clauses, merge_computed_fields,
        vector_distance_projection_fields, where_field_types,
    },
};
use crate::{
    db::{WhereClause, projection_generator::PostgresProjectionGenerator, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    runtime::{JsonbStrategy, ResultProjector},
    schema::SqlProjectionHint,
    security::{
        RlsWhereClause, SecurityContext,
        authorizer::{OperationKind, enforce_authz},
    },
};

/// A direct read resolved down to the statement it will run (#958).
///
/// Produced by [`QueryRunner::resolve_direct_read`] and consumed by both the
/// buffered and the streamed execution of the same read. Owned rather than
/// borrowed from the `QueryMatch`, because the streaming path outlives the call
/// that built it.
pub(in super::super) struct ResolvedDirectRead {
    /// The view to read.
    sql_source:     String,
    /// Security conditions (RLS + `inject_params`) AND-ed with the client filter.
    composed_where: Option<WhereClause>,
    /// Enriched ORDER BY, or `None` for the view's own order.
    order_by:       Option<Vec<crate::db::OrderByClause>>,
    /// Page size, already capped by `max_page_size` (#421).
    limit:          Option<u32>,
    /// Page offset.
    offset:         Option<u32>,
    /// Session variables to pin to the read's connection (#329).
    session_vars:   Vec<(String, String)>,
    /// Field-level RBAC classification for the projection (#886).
    pub access:     crate::runtime::field_filter::FieldAccessResult,
    /// Projection for the computed fields this read selects (#959), or `None`
    /// when it selects none — see [`Self::projection_request`].
    projection:     Option<crate::db::SqlProjectionHint>,
}

impl ResolvedDirectRead {
    /// Borrow the session variables in the shape the adapter takes.
    fn session_pairs(&self) -> Vec<(&str, &str)> {
        self.session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    /// The adapter call shape for this read.
    ///
    /// The projection is `None` unless the read selects a computed field: this
    /// runner reads the whole `data` document and projects in Rust, because the
    /// REST selection may name fields the SQL projection generator does not
    /// model. A vector distance (#959) is not in the document at all, so it is
    /// projected as `data || jsonb_build_object(…)` — additive, so the reason
    /// for reading the whole row still holds.
    fn projection_request(&self) -> crate::db::ProjectionRequest<'_> {
        crate::db::ProjectionRequest {
            view:         &self.sql_source,
            projection:   self.projection.as_ref(),
            where_clause: self.composed_where.as_ref(),
            order_by:     self.order_by.as_deref(),
            limit:        self.limit,
            offset:       self.offset,
        }
    }
}

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
    ///
    /// `pub(super)` so the sibling `query_relay` impl of the same `QueryRunner` can
    /// reuse it for the node-lookup path (#610).
    pub(super) fn resolve_session_vars(
        &self,
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<(String, String)>> {
        let sv = &self.ctx.schema.session_variables;
        match security_context {
            Some(sec) if !sv.variables.is_empty() || sv.inject_started_at => {
                crate::runtime::executor::security::resolve_session_variables(sv, sec)
            },
            _ => Ok(Vec::new()),
        }
    }

    /// AND the server's own scoping predicate
    /// ([`QueryMatch::scope_where`](crate::runtime::QueryMatch::scope_where))
    /// onto the security conditions.
    ///
    /// Unconditional by construction (#1170). The predicate REST resource
    /// embedding builds to scope a relation to its parent used to travel in
    /// `arguments["where"]`, which every read path composes **only** when the
    /// target query's `auto_params.has_where` is set — so a project that turned
    /// off a query's client filter argument also turned off relation scoping for
    /// every parent that embedded it. No error, no warning: each parent's `posts`
    /// became the whole `posts` table, each `posts_count` the whole table's
    /// count, and the `ManyToOne` branch — which takes the *first* row of the
    /// target's result — attributed every child to one arbitrary parent.
    ///
    /// It belongs here, beside RLS and `inject_params`, because it is the same
    /// kind of thing: a condition the server imposes, not one the client asked
    /// for. The client's own `where` is still gated by `has_where` below, which
    /// is what that flag is actually for.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the predicate does not parse as a
    /// `where` document — loudly, because the alternative is dropping it, which
    /// is the defect.
    fn and_scope_where(
        &self,
        security: Option<WhereClause>,
        query_match: &crate::runtime::matcher::QueryMatch,
    ) -> Result<Option<WhereClause>> {
        let Some(scope) = query_match.scope_where.as_ref() else {
            return Ok(security);
        };
        let types = where_field_types(&self.ctx.schema, &query_match.query_def.return_type);
        let scope = WhereClause::from_graphql_json(scope, &types)?;
        Ok(Some(match security {
            Some(sec) => WhereClause::And(vec![sec, scope]),
            None => scope,
        }))
    }

    /// Execute a `<name>Count` sibling query (#938).
    ///
    /// Delegates the whole decision to [`count_rows`](Self::count_rows) — the
    /// same method the REST `Prefer: count=exact` header uses — so the two
    /// surfaces cannot drift on what a filtered total means, and so the count
    /// inherits one implementation of operation authorization, RLS fail-closed,
    /// inject-param scoping and session variables rather than a second copy.
    ///
    /// The caller is responsible for the `requires_role` / anonymous guards; both
    /// entry points below apply them before routing here, exactly as they do for
    /// the list query this counts.
    async fn execute_count_query(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        let total = self.count_rows(query_match, variables, security_context).await?;
        Ok(ResultProjector::wrap_in_data_envelope(
            serde_json::json!(total),
            query_match.response_key(),
        ))
    }

    /// Build the SQL projection for a read, including any vector-distance field
    /// it selects (#386, #959).
    ///
    /// One builder for all three read paths. They had two copies of the typed
    /// projection block and a third path with none, and a projection is where a
    /// field either reaches the response or silently does not — the shape #739
    /// showed costs a surface its answer.
    ///
    /// `full_row` is the caller saying the row must come back whole: a selected
    /// policy-gated field needs the complete parent for the authorizer to decide
    /// on (#423), and the REST direct read may name fields the projection
    /// generator does not model. A distance field still has to be delivered
    /// there, so the projection becomes `data || jsonb_build_object(…)` — the
    /// stored row, plus what is not stored.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when a selected vector-distance
    /// field has no matching `nearest` search, and `FraiseQLError::Internal`
    /// when a projection cannot be built at all — never a silent fallback to
    /// "select everything", which is how a `node(id:)` lookup once served every
    /// column in its view (#827).
    fn build_projection_hint(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        plan: &crate::runtime::planner::ExecutionPlan,
        full_row: bool,
        nearest: Option<&crate::db::OrderByClause>,
    ) -> Result<Option<SqlProjectionHint>> {
        let root_fields = query_match
            .selections
            .first()
            .map_or(&[] as &[_], |s| s.nested_fields.as_slice());
        let distance_fields = vector_distance_projection_fields(
            root_fields,
            &self.ctx.schema,
            &query_match.query_def.return_type,
            nearest,
        )?;

        let generator = PostgresProjectionGenerator::new();
        let internal = |e: FraiseQLError| FraiseQLError::Internal {
            message: format!(
                "could not build a projection for type '{}': {e}",
                query_match.query_def.return_type
            ),
            source:  None,
        };

        if full_row
            || plan.projection_fields.is_empty()
            || plan.jsonb_strategy != JsonbStrategy::Project
        {
            if distance_fields.is_empty() {
                return Ok(None);
            }
            let sql =
                generator.generate_merged_projection_sql(&distance_fields).map_err(internal)?;
            return Ok(Some(SqlProjectionHint::new(self.ctx.adapter.database_type(), sql, 0)));
        }

        let mut typed_fields = build_typed_projection_fields(
            root_fields,
            &self.ctx.schema,
            &query_match.query_def.return_type,
            0,
        );
        merge_computed_fields(&mut typed_fields, distance_fields);
        // A projection that cannot be built is an error, not a licence to
        // select every column: the Rust projector subsets top-level keys but
        // returns nested objects whole, so the fallback leaked sub-blobs the
        // client never asked for (#827's family).
        let projection_sql =
            generator.generate_typed_projection_sql(&typed_fields).map_err(internal)?;
        Ok(Some(SqlProjectionHint::new(
            self.ctx.adapter.database_type(),
            projection_sql,
            compute_projection_reduction(plan.projection_fields.len()),
        )))
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
        operation_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        // 1. Validate security context (check expiration, etc.)
        if security_context.is_expired() {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }

        // 2. Match query to compiled template — for the operation the request selected, not the
        //    document's first one (§ 6.1).
        let query_match =
            self.ctx
                .matcher
                .match_query_with_operation_name(query, variables, operation_name)?;

        // 2b. Enforce requires_role — "not found" (not "forbidden") to prevent enumeration.
        crate::security::role_gate::enforce_requires_role(
            "Query",
            &query_match.query_def.name,
            query_match.query_def.requires_role.as_deref(),
            Some(security_context),
        )?;

        // 2c. Enforce requires_actor (#966), after the role gate so a caller
        //     lacking the role learns nothing here that "not found" did not
        //     already tell them.
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &query_match.query_def.name,
            &query_match.query_def.requires_actor,
            Some(security_context),
        )?;

        // Resolve session variables once. They are applied transaction-locally
        // on the same connection as the read (fixes #329) by passing them into
        // the connection-affine adapter call below, so PostgreSQL RLS policies
        // that read `current_setting()` (e.g. `app.tenant_id`) are effective.
        let resolved_session_vars = self.resolve_session_vars(Some(security_context))?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        // Route relay queries to dedicated handler with security context.
        if query_match.query_def.relay {
            return self
                .execute_relay_query(&query_match, Some(security_context), &session_pairs)
                .await;
        }

        // Count siblings (#938) return a scalar, so none of the projection,
        // response-cache or field-RBAC machinery below applies — there is no row
        // to strip a field from. Routed after the `requires_role` guard above, so
        // a count is exactly as visible as the list it counts.
        if query_match.query_def.returns_count {
            return self.execute_count_query(&query_match, variables, Some(security_context)).await;
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
            let query_key = Self::compute_response_cache_key(&query_match);
            let sec_hash =
                crate::cache::response_cache::hash_security_context(Some(security_context));
            Some((query_key, sec_hash))
        } else {
            None
        };

        // Snapshot before the miss path does any work (#1079). A mutation that commits
        // and invalidates while this request executes would otherwise be undone by the
        // put at the end of this function, which would store a response computed from
        // pre-mutation rows.
        let response_cache_fence =
            self.ctx.response_cache.as_ref().map(|rc| rc.invalidation_generation());

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
        // #1170: the server's own parent scoping is composed here, with RLS and
        //     inject and *before* the client-filter gate below — it is not client
        //     input and must not ride on the client filter surface.
        let combined_where = self.and_scope_where(combined_where, &query_match)?;

        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            // Built only when the request actually carries a filter: with
            // `has_where` on by default, every list query would otherwise pay
            // for a map it never reads.
            let user_where = query_match
                .arguments
                .get("where")
                .map(|w| {
                    let types =
                        where_field_types(&self.ctx.schema, &query_match.query_def.return_type);
                    WhereClause::from_graphql_json(w, &types)
                })
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
                coerce_pagination_arg("limit", query_match.arguments.get("limit"))?
            } else {
                None
            },
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = if query_match.query_def.auto_params.has_offset {
            coerce_pagination_arg("offset", query_match.arguments.get("offset"))?
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
                .transpose()?
        } else {
            None
        };

        // `nearest` similarity search (#386): lowers to a vector-distance
        // ORDER BY + LIMIT k. Conflicts with limit/orderBy error inside the
        // helper, so overriding both here cannot discard a client value.
        let (limit, order_by_clauses) = if let Some((clause, k)) = nearest_order_and_limit(
            &query_match.arguments,
            &self.ctx.schema,
            &query_match.query_def,
        )? {
            (
                enforce_max_page_size(Some(k), self.ctx.config.max_page_size, "nearest.k")?,
                Some(vec![clause]),
            )
        } else {
            (limit, order_by_clauses)
        };

        // 8c. Generate the SQL projection for the requested fields, after the
        //     `nearest` lowering so a selected distance field can be projected
        //     from the clause that ordered the rows.
        let projection_hint = self.build_projection_hint(
            &query_match,
            &plan,
            gated_present,
            order_by_clauses.as_ref().and_then(|c| c.first()),
        )?;

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
                query_match.query_def.read_routing,
            )
            .await?;

        // 10. Apply field-level RBAC filtering (reject / mask / allow)
        let access = super::super::support::security::apply_field_rbac_filtering(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 11. Project results. Masked fields stay in the projection, in their requested position,
        //     and are nulled below — GraphQL requires the response's field order to follow the
        //     query's.
        let projector = ResultProjector::new(access.projected.clone())
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;

        // 11a. #489: recase + project nested list-of-object fields the SQL projection
        //      left as the raw stored sub-blob (snake_case keys, unselected keys). The
        //      SQL side already projected top-level fields and nested single objects.
        crate::runtime::project_nested_lists(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

        // #912: `__typename` is stripped from the SQL projection at every depth (it
        //       is a meta-field, not a JSONB key — projecting it emits a literal
        //       NULL). The root object is stamped by the projector and list elements
        //       by `project_entity`; nested single objects have no other owner.
        crate::runtime::stamp_nested_typenames(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

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
            // `query_match.arguments` is the request's variables, already merged
            // with whole-argument inline values — the same map every other
            // consumer resolves against (#903).
            let gated = authz::collect_top_level_gated_fields(
                &self.ctx.schema,
                return_type,
                root_fields,
                &query_match.arguments,
            )?;
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
            ResultProjector::wrap_in_data_envelope(projected, query_match.response_key());

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
            // Every view this query reads, primary and declared secondary — the
            // same definition the row cache registers under (#761).
            let accessed = crate::cache::extract_accessed_views(&query_match.query_def);
            let cached = Arc::new(response);
            let _ =
                rc.put(query_key, sec_hash, Arc::clone(&cached), accessed, response_cache_fence);
            return Ok(Arc::unwrap_or_clone(cached));
        }

        Ok(response)
    }

    /// Compute a response cache key from a query match.
    ///
    /// Delegates to [`crate::cache::generate_response_cache_key`], which owns
    /// every cache-key derivation in the workspace, so a new dimension added there
    /// reaches this cache too. Combined with the security-context hash, this forms
    /// the full response cache key.
    fn compute_response_cache_key(query_match: &crate::runtime::matcher::QueryMatch) -> u64 {
        crate::cache::generate_response_cache_key(
            &query_match.query_def.name,
            query_match.operation_name.as_deref(),
            &query_match.selections,
            &query_match.arguments,
        )
    }

    /// Execute a regular query, applying RLS/role/inject enforcement when a
    /// [`SecurityContext`] is present and the anonymous fail-closed path otherwise.
    ///
    /// The unified dispatcher and the multi-root fan-out both call this single
    /// entry so the authenticated and anonymous regular-query paths cannot drift
    /// (H19). It is an `async fn`, so it has one opaque future type — callers can
    /// collect homogeneous futures across roots without boxing.
    ///
    /// # Errors
    ///
    /// Propagates the errors of
    /// [`execute_regular_query`](Self::execute_regular_query) or
    /// [`execute_regular_query_with_security`](Self::execute_regular_query_with_security).
    pub(in super::super) async fn execute_regular_query_maybe_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        operation_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        match security_context {
            Some(ctx) => {
                self.execute_regular_query_with_security(query, variables, ctx, operation_name)
                    .await
            },
            None => self.execute_regular_query(query, variables, operation_name).await,
        }
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
        operation_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        // 1. Match query to compiled template — for the operation the request selected, not the
        //    document's first one (§ 6.1).
        let query_match =
            self.ctx
                .matcher
                .match_query_with_operation_name(query, variables, operation_name)?;

        // Guard: role-restricted queries are invisible to unauthenticated users
        if query_match.query_def.requires_role.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", query_match.query_def.name),
                path:    None,
            });
        }

        // The same guard for the actor allow-list (#966): an unauthenticated
        // request has no classification, so it belongs to no class.
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &query_match.query_def.name,
            &query_match.query_def.requires_actor,
            None,
        )?;

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

        // Guard (#784): an RLS-protected deployment must not serve unauthenticated
        // regular queries — the policy cannot be evaluated without a principal.
        // Fail closed exactly as the relay and node paths do.
        if self.ctx.config.rls_policy.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{}' not found in schema", query_match.query_def.name),
                path:    None,
            });
        }

        // Route relay queries to dedicated handler.
        // No session vars: unauthenticated entrypoint (no SecurityContext). See #329.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, None, &[]).await;
        }

        // Count siblings (#938), after the three guards above — a count of rows an
        // anonymous caller may not read is still a disclosure about those rows,
        // so it is refused on exactly the conditions the list is.
        if query_match.query_def.returns_count {
            return self.execute_count_query(&query_match, variables, None).await;
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

        // 2a. #743: static field-level RBAC. An anonymous caller has no roles, so any
        //     selected field carrying `requires_scope` is denied per its own on_deny
        //     policy — exactly what an authenticated-but-unscoped principal gets from
        //     apply_field_rbac_filtering. Classifying here (not after the read) means a
        //     Reject never reaches the database.
        let access = super::super::support::security::apply_anonymous_field_rbac_filtering(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            &plan.projection_fields,
        )?;

        // 3. Execute SQL query
        let sql_source = query_match.query_def.sql_source.as_ref().ok_or_else(|| {
            crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            }
        })?;

        // 3b. Extract auto_params (limit, offset, where, order_by) from arguments
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(|w| {
                    let types =
                        where_field_types(&self.ctx.schema, &query_match.query_def.return_type);
                    WhereClause::from_graphql_json(w, &types)
                })
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

        // #1170: and the server's own parent scoping, which is deliberately NOT
        //     gated on `has_where` above — that flag governs the client's filter
        //     surface, not whether a relation is scoped to its parent.
        let user_where = self.and_scope_where(user_where, &query_match)?;

        // The top-level page size is capped (#421: unbounded-pagination DoS guard).
        let limit = enforce_max_page_size(
            if query_match.query_def.auto_params.has_limit {
                coerce_pagination_arg("limit", query_match.arguments.get("limit"))?
            } else {
                None
            },
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = if query_match.query_def.auto_params.has_offset {
            coerce_pagination_arg("offset", query_match.arguments.get("offset"))?
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
                .transpose()?
        } else {
            None
        };

        // `nearest` similarity search (#386): lowers to a vector-distance
        // ORDER BY + LIMIT k. Conflicts with limit/orderBy error inside the
        // helper, so overriding both here cannot discard a client value.
        let (limit, order_by_clauses) = if let Some((clause, k)) = nearest_order_and_limit(
            &query_match.arguments,
            &self.ctx.schema,
            &query_match.query_def,
        )? {
            (
                enforce_max_page_size(Some(k), self.ctx.config.max_page_size, "nearest.k")?,
                Some(vec![clause]),
            )
        } else {
            (limit, order_by_clauses)
        };

        // 3b. Generate the SQL projection, after the `nearest` lowering so a
        //     selected distance field can be projected from the clause that
        //     ordered the rows.
        let projection_hint = self.build_projection_hint(
            &query_match,
            &plan,
            false,
            order_by_clauses.as_ref().and_then(|c| c.first()),
        )?;

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

        // 4. Project results — masked fields stay in the projection so the response still carries
        //    the key in its requested position, then get nulled below (same as the authenticated
        //    path).
        let projector = ResultProjector::new(access.projected.clone())
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;
        // #489: recase + project nested list-of-object fields left raw by SQL projection.
        crate::runtime::project_nested_lists(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

        // #912: `__typename` is stripped from the SQL projection at every depth (it
        //       is a meta-field, not a JSONB key — projecting it emits a literal
        //       NULL). The root object is stamped by the projector and list elements
        //       by `project_entity`; nested single objects have no other owner.
        crate::runtime::stamp_nested_typenames(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

        // 4a. #743: null out fields denied to the anonymous caller under on_deny=Mask.
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        // 5. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, query_match.response_key());

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
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        let resolved = self.resolve_direct_read(query_match, variables, security_context)?;
        let session_pairs = resolved.session_pairs();

        let results = self
            .ctx
            .adapter
            .execute_with_projection_arc_with_session(
                &resolved.projection_request(),
                &session_pairs,
                query_match.query_def.read_routing,
            )
            .await?;

        let projected = self.project_direct_rows(
            query_match,
            &resolved.access,
            &results,
            query_match.query_def.returns_list,
        )?;

        // Wrap in GraphQL data envelope.
        Ok(ResultProjector::wrap_in_data_envelope(projected, query_match.response_key()))
    }

    /// Resolve a direct read down to the SQL it will run and the field access it
    /// will project with — everything that happens *before* the database, in one
    /// place (#958).
    ///
    /// # Why this is a function and not a comment saying "keep these in sync"
    ///
    /// The buffered read and the streamed read differ only in how rows arrive. Every
    /// security decision — operation authorization, the field-authorization gate, the
    /// RLS policy, the `inject_params` tenant filter, the field-level RBAC
    /// classification — belongs to the *read*, not to its delivery, and the last time
    /// two paths resolved a read separately, one of them composed the tenant filter
    /// and the other did not (#739: a `count=exact` header that contradicted its own
    /// body). A streaming path is a second such reader, so it resolves through this
    /// function or it does not exist.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` when the operation, a gated field, or a
    /// scope-restricted field is refused; `FraiseQLError::Validation` when the query
    /// has no SQL source, when a policy or `inject_params` is configured but there is
    /// no principal to evaluate it for (fail closed, #784), or when an argument does
    /// not parse.
    fn resolve_direct_read(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<ResolvedDirectRead> {
        // #422: operation-level authorization for the REST direct-read chokepoint.
        //       Every REST read (GET/count/streaming/embedding) and the in-core
        //       bulk-by-filter lookup funnel through this runner method, so gating
        //       here (not the `core.rs` wrapper) is leak-proof. Fail-closed → 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = [(OperationKind::Query, query_match.query_def.name.clone())];
            enforce_authz(authorizer.as_ref(), security_context, &ops, variables)?;
        }

        // #1122: `requires_role`, for exactly the reason #966 is here. The role gate
        // lived only in `execute_regular_query_with_security` — a GraphQL entry point
        // this path never enters — plus a pre-check in the REST resolver that read
        // `scopes` instead of `roles`. So a token carrying `scope: "reader"` and no
        // roles at all was served every row of a query gated on the `reader` *role*.
        // Enforced before the actor gate to keep the GraphQL ordering, which
        // `the_role_gate_still_runs_first` pins: the role gate's enumeration-hiding
        // "not found" must not be pre-empted by the actor gate's message.
        crate::security::role_gate::enforce_requires_role(
            "Query",
            &query_match.query_def.name,
            query_match.query_def.requires_role.as_deref(),
            security_context,
        )?;

        // #966: the actor allow-list, for the same reason and in the same place.
        // The GraphQL entry points gate before dispatching; this path does not go
        // through them — `execute_query_direct`, `count_rows` and the streaming
        // reader all enter here — so a gate placed only there served every
        // restricted row over REST. Caught by `actor_predicate_e2e_pg`'s REST
        // case, which is precisely the #808 shape the issue warns about: a
        // predicate enforced on one transport is not enforced.
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &query_match.query_def.name,
            &query_match.query_def.requires_actor,
            security_context,
        )?;

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

        // Evaluate RLS policy if present. Fail closed (#784) when a policy is
        // configured but there is no principal to evaluate it for — the same
        // posture as the relay and node runners.
        let rls_where_clause: Option<RlsWhereClause> =
            match (&self.ctx.config.rls_policy, security_context) {
                (Some(rls_policy), Some(ctx)) => {
                    rls_policy.evaluate(ctx, &query_match.query_def.name)?
                },
                (Some(_), None) => {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Query '{}' not found in schema",
                            query_match.query_def.name
                        ),
                        path:    None,
                    });
                },
                (None, _) => None,
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

        // #886: static field-level RBAC, the same classification both GraphQL paths
        // apply. This runner previously applied none — it projected
        // `plan.projection_fields` verbatim — so a field carrying `requires_scope` was
        // served in full over REST to a caller without the scope. Classifying *before*
        // the read means a `Reject` never reaches the database.
        let access = super::super::support::security::classify_fields_for_read(
            &self.ctx.schema,
            &query_match.query_def.return_type,
            plan.projection_fields.clone(),
            security_context,
        )?;

        // Extract auto_params from arguments.
        let user_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            query_match
                .arguments
                .get("where")
                .map(|w| {
                    let types =
                        where_field_types(&self.ctx.schema, &query_match.query_def.return_type);
                    WhereClause::from_graphql_json(w, &types)
                })
                .transpose()?
        } else {
            None
        };

        // The top-level page size is capped (#421: unbounded-pagination DoS guard).
        let limit = enforce_max_page_size(
            coerce_pagination_arg("limit", query_match.arguments.get("limit"))?,
            self.ctx.config.max_page_size,
            "limit",
        )?;

        let offset = coerce_pagination_arg("offset", query_match.arguments.get("offset"))?;

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
            })
            .transpose()?;

        // `nearest` similarity search (#386) — same lowering as the two
        // GraphQL runners above, so the direct path cannot drift (#739's class).
        let (limit, order_by_clauses) = if let Some((clause, k)) = nearest_order_and_limit(
            &query_match.arguments,
            &self.ctx.schema,
            &query_match.query_def,
        )? {
            (
                enforce_max_page_size(Some(k), self.ctx.config.max_page_size, "nearest.k")?,
                Some(vec![clause]),
            )
        } else {
            (limit, order_by_clauses)
        };

        // Convert explicit arguments to WHERE conditions.
        let user_where = combine_explicit_arg_where(
            user_where,
            &query_match.query_def.arguments,
            &query_match.arguments,
            &query_match.query_def.native_columns,
        );

        // #1170: and the server's own parent scoping, which is deliberately NOT
        //     gated on `has_where` above — that flag governs the client's filter
        //     surface, not whether a relation is scoped to its parent.
        let user_where = self.and_scope_where(user_where, query_match)?;

        // Compose the security conditions — RLS **and** inject — then AND the
        // user-supplied WHERE onto them. Security first, so a client-supplied filter can
        // only ever narrow the result set, never widen it.
        //
        // #739: this block previously resolved each inject value into `let _value = …`
        // and threw it away, with a comment claiming the params were "applied at the SQL
        // level via WHERE clauses". They were not — no predicate was built and nothing
        // was AND-ed on. Every other execution path (`execute_regular_query_with_security`,
        // `count_rows`, `execute_relay_query`, `execute_node_query`) composed them
        // correctly; this one, the runner behind the entire REST read surface, did not.
        // The observable symptom was a `Prefer: count=exact` response whose header
        // (filtered, from `count_rows`) contradicted its own body (unfiltered, from here).
        let mut security_conditions: Vec<WhereClause> = Vec::new();
        if let Some(ref rls) = rls_where_clause {
            security_conditions.push(rls.as_where_clause().clone());
        }
        if !query_match.query_def.inject_params.is_empty() {
            // Fail closed, mirroring `count_rows`: a query that declares row scoping has
            // no safe reading without a principal to scope it to. Silently skipping the
            // filter — the previous behaviour for an anonymous caller — returns every
            // tenant's rows.
            let ctx = security_context.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but no security context is available",
                    query_match.query_def.name
                ),
                path:    None,
            })?;
            for (param_name, source) in &query_match.query_def.inject_params {
                let value = resolve_inject_value(param_name, source, ctx)?;
                security_conditions.push(inject_param_where_clause(
                    param_name,
                    value,
                    &query_match.query_def.native_columns,
                ));
            }
        }

        let security_where = match security_conditions.len() {
            0 => None,
            1 => Some(security_conditions.remove(0)),
            _ => Some(WhereClause::And(security_conditions)),
        };

        let composed_where = match (security_where, &user_where) {
            (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user.clone()])),
            (Some(sec), None) => Some(sec),
            (None, Some(user)) => Some(user.clone()),
            (None, None) => None,
        };

        // Session variables pin to the read's connection (#329).
        let session_vars = self.resolve_session_vars(security_context)?;

        // The direct read projects in Rust, so the only SQL projection it needs
        // is for values that are not in the document — the vector distance a
        // `nearest` search computed (#959).
        let projection = self.build_projection_hint(
            query_match,
            &plan,
            true,
            order_by_clauses.as_ref().and_then(|c| c.first()),
        )?;

        Ok(ResolvedDirectRead {
            sql_source: sql_source.clone(),
            composed_where,
            order_by: order_by_clauses,
            limit,
            offset,
            session_vars,
            access,
            projection,
        })
    }

    /// The same read as [`execute_query_direct`](Self::execute_query_direct),
    /// delivered one projected row at a time (#958).
    ///
    /// Every row is resolved through [`resolve_direct_read`](Self::resolve_direct_read)
    /// and projected through [`project_direct_rows`](Self::project_direct_rows), so an
    /// export sees exactly the rows, fields and masking a buffered read of the same
    /// query would return — the streaming path adds no second interpretation of the
    /// request, only a second way to deliver the answer.
    ///
    /// The result is **not** wrapped in a `data` envelope: the caller frames rows for
    /// its own representation (one NDJSON line, one CSV record, one worksheet row),
    /// and there is no single envelope for a response with no end.
    ///
    /// # Ownership
    ///
    /// Arguments are taken by value because the returned stream outlives this call.
    ///
    /// # Errors
    ///
    /// Returns everything [`resolve_direct_read`](Self::resolve_direct_read) does,
    /// plus `FraiseQLError::Database` for a read that fails before its first row. A
    /// failure after that arrives as an `Err` item in the stream.
    pub(in super::super) async fn stream_query_direct(
        &self,
        query_match: crate::runtime::matcher::QueryMatch,
        variables: Option<serde_json::Value>,
        security_context: Option<SecurityContext>,
    ) -> Result<crate::runtime::JsonRowStream>
    where
        A: 'static,
    {
        let resolved =
            self.resolve_direct_read(&query_match, variables.as_ref(), security_context.as_ref())?;
        let rows = {
            let session_pairs = resolved.session_pairs();
            self.ctx
                .adapter
                .stream_with_projection(
                    &resolved.projection_request(),
                    &session_pairs,
                    query_match.query_def.read_routing,
                )
                .await?
        };

        let runner = Self::new(Arc::clone(&self.ctx));
        let access = resolved.access;
        Ok(Box::pin(rows.map(move |row| {
            let row = row?;
            // Projected as a one-element list so the row goes through the list
            // element path — the same one a buffered list read uses, including
            // `project_entity`'s `__typename` stamping.
            let projected = runner.project_direct_rows(&query_match, &access, &[row], true)?;
            match projected {
                serde_json::Value::Array(mut items) if items.len() == 1 => Ok(items.remove(0)),
                serde_json::Value::Array(items) => Err(FraiseQLError::Internal {
                    message: format!(
                        "streaming projection produced {} rows for one input row",
                        items.len()
                    ),
                    source:  None,
                }),
                _ => Err(FraiseQLError::Internal {
                    message: "streaming projection did not produce a row list".to_string(),
                    source:  None,
                }),
            }
        })))
    }

    /// Project rows a direct read returned, exactly as `execute_query_direct` does.
    ///
    /// Shared with the streaming path, which calls it one row at a time (#958):
    /// projection order, nested-list recasing, `__typename` stamping and mask
    /// nulling are what a *row* looks like in a response, and a streamed row that
    /// skipped any of them would be a second answer to the same question.
    fn project_direct_rows(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        access: &crate::runtime::field_filter::FieldAccessResult,
        rows: &[crate::db::types::JsonbValue],
        returns_list: bool,
    ) -> Result<serde_json::Value> {
        // Masked fields stay in the projection, in their requested position, and are
        // nulled below — the response keeps the key and withholds only the value, as
        // both GraphQL paths do.
        let projector = ResultProjector::new(access.projected.clone())
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected = projector.project_results(rows, returns_list)?;
        // #489: recase + project nested list-of-object fields left raw by SQL projection.
        crate::runtime::project_nested_lists(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

        // #912: `__typename` is stripped from the SQL projection at every depth (it
        //       is a meta-field, not a JSONB key — projecting it emits a literal
        //       NULL). The root object is stamped by the projector and list elements
        //       by `project_entity`; nested single objects have no other owner.
        crate::runtime::stamp_nested_typenames(
            &mut projected,
            &query_match.query_def.return_type,
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice()),
            &self.ctx.schema,
        );

        // #886: null out fields denied to this caller under `on_deny = Mask`.
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        Ok(projected)
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
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<u64> {
        // #422: operation-level authorization for the REST count path (called alone by
        //       the embedding path, and alongside `execute_query_direct` for
        //       `Prefer: count=exact`). Fail-closed → 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = [(OperationKind::Query, query_match.query_def.name.clone())];
            enforce_authz(authorizer.as_ref(), security_context, &ops, variables)?;
        }

        // #1122: the role gate, here as well as in `resolve_direct_read`, because this
        // is a second chokepoint rather than a step inside the first — the embedding
        // path (`rest/embedding/executor.rs`) calls it alone. A count is a read: it
        // reports the cardinality of a set the caller may not select from.
        crate::security::role_gate::enforce_requires_role(
            "Query",
            &query_match.query_def.name,
            query_match.query_def.requires_role.as_deref(),
            security_context,
        )?;

        // #1166: the actor gate, for precisely the reason the role gate above is here,
        // and missing for as long as that one was. `resolve_direct_read` carried both;
        // this peer carried only the role half, so `?select=id,rel.count` reported how
        // many rows a `requires_actor`-gated relation reaches to a caller of the wrong
        // actor class. The rows never leaked — the row path is gated — but a count over
        // a filtered relation is an oracle over it.
        //
        // Ordered after the role gate to match `resolve_direct_read`, so the role
        // gate's enumeration-hiding "not found" is not pre-empted by the actor gate's
        // message on either chokepoint (`the_role_gate_still_runs_first`).
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &query_match.query_def.name,
            &query_match.query_def.requires_actor,
            security_context,
        )?;

        // 1. Evaluate RLS policy. Fail closed (#784) when a policy is configured but there is no
        //    principal — the count must not disagree with the (equally refused) body it describes.
        let rls_where_clause: Option<RlsWhereClause> =
            match (&self.ctx.config.rls_policy, security_context) {
                (Some(rls_policy), Some(ctx)) => {
                    rls_policy.evaluate(ctx, &query_match.query_def.name)?
                },
                (Some(_), None) => {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Query '{}' not found in schema",
                            query_match.query_def.name
                        ),
                        path:    None,
                    });
                },
                (None, _) => None,
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
        // #1170: the server's own parent scoping is composed here, with RLS and
        //     inject and *before* the client-filter gate below — it is not client
        //     input and must not ride on the client filter surface.
        let combined_where = self.and_scope_where(combined_where, query_match)?;

        let combined_where: Option<WhereClause> = if query_match.query_def.auto_params.has_where {
            // Built only when the request actually carries a filter: with
            // `has_where` on by default, every list query would otherwise pay
            // for a map it never reads.
            let user_where = query_match
                .arguments
                .get("where")
                .map(|w| {
                    let types =
                        where_field_types(&self.ctx.schema, &query_match.query_def.return_type);
                    WhereClause::from_graphql_json(w, &types)
                })
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
        //
        //    This used to fetch every matching row and take `.len()`: correct, but
        //    it materialised the whole filtered set to produce one integer, so
        //    `GET /rest/users?limit=10` with `Prefer: count=exact` pulled the
        //    entire table into memory. `count_where_query` pushes it into
        //    `SELECT COUNT(*)`, and is the same method the GraphQL `<name>Count`
        //    sibling uses — one count path, so the two surfaces cannot disagree
        //    about what a filtered total means (#938).
        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.ctx
            .adapter
            .count_where_query(
                sql_source,
                combined_where.as_ref(),
                &session_pairs,
                query_match.query_def.read_routing,
            )
            .await
    }
}
