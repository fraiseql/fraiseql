//! Regular (non-relay) query execution methods for [`QueryRunner`].

use std::sync::Arc;

use futures::StreamExt as _;

use super::{
    query::QueryRunner,
    query_params::compute_projection_reduction,
    query_projection::{
        build_typed_projection_fields, merge_computed_fields, projects_decimals_as_text,
        vector_distance_projection_fields, where_field_types,
    },
    root_read::{DirectReadCost, Door, GatedFieldHandling, RootRoute},
};
use crate::{
    backend::{WhereClause, projection_generator::PostgresProjectionGenerator},
    error::{FraiseQLError, Result},
    graphql::selection_set::KeptProjection,
    runtime::{JsonbStrategy, ResultProjector},
    schema::{FieldType, SqlProjectionHint},
    security::SecurityContext,
};

/// A row-shaped read resolved down to the arguments the column-shaped adapter
/// methods take (#1351).
///
/// One struct rather than a tuple because the buffered and the streamed arm both
/// consume it, and the two gRPC read arms have already drifted apart once (#1348
/// found one fail-closed and the other fail-open on the same RLS failure). A named
/// field is harder to pass in the wrong position than the fourth element of a tuple.
struct RowReadPlan {
    /// Everything resolved before the database — gates, RLS, limit, field access.
    resolved:  super::root_read::RootRead,
    /// The row-shaped view this read targets.
    view:      String,
    /// The projection, after field-level RBAC narrowed it.
    columns:   Vec<fraiseql_db::types::ColumnSpec>,
    /// The composed predicate, lowered to SQL.
    where_sql: Option<String>,
    /// The resolved ordering, lowered to SQL.
    order_sql: Option<String>,
    /// Per-row #423 adjudication, or `None` when this read is not gated (#1351).
    ///
    /// Carried on the plan rather than recomputed by each row entry, so the buffered
    /// and streamed arms cannot disagree about whether a read is gated — the drift
    /// #1348 found between exactly these two arms, and the reason
    /// `resolve_root_read` is a function.
    gate:      Option<crate::security::field_authorizer::RowFieldGate>,
    /// The localized columns and the request locale's chain (#1513), or `None` when the
    /// read has none. Carried on the plan for the reason `gate` is: one decision for
    /// both arms.
    localizer: Option<RowLocalizer>,
}

/// Reads the localized columns of a row through the request locale's chain (#1513).
///
/// A row view's column for a localized field holds the stored locale map, which arrives as
/// [`ColumnValue::Json`](fraiseql_db::types::ColumnValue::Json); it leaves as the label (or
/// `Null`), through [`localize`](crate::runtime::localize), the rule the JSON path's SQL
/// follows.
#[derive(Clone)]
struct RowLocalizer {
    columns: Vec<usize>,
    chain:   Vec<String>,
}

impl RowLocalizer {
    fn new(
        type_def: &crate::schema::TypeDefinition,
        columns: &[fraiseql_db::types::ColumnSpec],
        schema: &crate::schema::CompiledSchema,
    ) -> Option<Self> {
        let localized: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| type_def.find_field(&c.name).is_some_and(|f| f.localized))
            .map(|(i, _)| i)
            .collect();
        if localized.is_empty() {
            return None;
        }
        Some(Self {
            columns: localized,
            chain:   crate::runtime::localization_chain(schema)?,
        })
    }

    fn apply(&self, row: &mut [fraiseql_db::types::ColumnValue]) {
        use fraiseql_db::types::ColumnValue;
        for &i in &self.columns {
            let Some(cell) = row.get_mut(i) else {
                continue;
            };
            if let ColumnValue::Json(text) = cell {
                let stored = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
                *cell = match crate::runtime::localize(&stored, &self.chain) {
                    serde_json::Value::String(label) => ColumnValue::Text(label),
                    _ => ColumnValue::Null,
                };
            }
        }
    }
}

/// A row-shaped read and the projection it was actually read with (#1351).
///
/// The columns travel with the rows because field-level RBAC (#886) can narrow
/// them: the adapter zips values to specs positionally, so a caller that encoded
/// with the specs it *asked* for rather than the ones the read *used* would shift
/// every value one column to the left the moment a field was withheld — serving
/// one field's value under another field's name.
#[derive(Debug)]
pub struct RowRead {
    /// The columns read, after field-level RBAC narrowed them.
    pub columns: Vec<fraiseql_db::types::ColumnSpec>,
    /// The rows, each in `columns` order.
    pub rows:    Vec<Vec<fraiseql_db::types::ColumnValue>>,
}

/// The streamed twin of [`RowRead`], carrying its projection for the same reason.
pub struct StreamedRowRead {
    /// The columns read, after field-level RBAC narrowed them.
    pub columns: Vec<fraiseql_db::types::ColumnSpec>,
    /// The rows, each in `columns` order.
    pub stream:  fraiseql_db::ColumnRowStream,
}

impl QueryRunner {
    /// Resolve configured session variables for `security_context` into owned
    /// `(name, value)` pairs.
    ///
    /// The caller borrows these into a `&[(&str, &str)]` slice for the
    /// connection-affine `*_with_session` adapter methods, which apply them
    /// transaction-locally on the same connection as the read (fixes #329 for
    /// RLS policies backed by `current_setting()`).
    ///
    /// The configured variables (none without a principal) plus the request locale when
    /// the schema declares `[locale]` (#1512). An empty slice means "no session variables"
    /// to the adapter, with zero overhead.
    ///
    /// `pub(super)` so the sibling `query_relay` impl of the same `QueryRunner` can
    /// reuse it for the node-lookup path (#610).
    pub(super) fn resolve_session_vars(
        &self,
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<(String, String)>> {
        crate::runtime::executor::support::security::read_session_variables(
            &self.ctx.schema,
            security_context,
        )
    }

    /// AND the server's own scoping predicate
    /// ([`QueryMatch::scope_where`](crate::runtime::QueryMatch::scope_where))
    /// onto the security conditions.
    ///
    /// Unconditional by construction (#1170). The predicate REST resource
    /// embedding builds to scope a relation to its parent used to travel in
    /// `arguments["where"]`, which no read path composed unless the target
    /// query's `auto_params.has_where` was set — so a project that turned off a
    /// query's client filter argument also turned off relation scoping for every
    /// parent that embedded it. No error, no warning: each parent's `posts`
    /// became the whole `posts` table, each `posts_count` the whole table's
    /// count, and the `ManyToOne` branch — which takes the *first* row of the
    /// target's result — attributed every child to one arbitrary parent.
    ///
    /// It belongs here, beside RLS and `inject_params`, because it is the same
    /// kind of thing: a condition the server imposes, not one the client asked
    /// for. The client's own `where` is still governed by `has_where` below —
    /// which is what that flag is actually for — and since #1283 a client filter
    /// the flag turns off is refused rather than dropped, so neither predicate
    /// can go missing under a `200` any more.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the predicate does not parse as a
    /// `where` document — loudly, because the alternative is dropping it, which
    /// is the defect.
    pub(super) fn and_scope_where(
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
    pub(super) fn build_projection_hint(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        plan: &crate::runtime::planner::PlannedProjection,
        full_row: bool,
        nearest: Option<&crate::backend::OrderByClause>,
    ) -> Result<Option<KeptProjection>> {
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
            // The stored row whole: its numbers arrive as written.
            return Ok(Some(KeptProjection {
                hint:             Arc::new(SqlProjectionHint::new(
                    self.ctx.adapter.database_type(),
                    sql,
                    0,
                )),
                decimals_as_text: false,
            }));
        }

        // A projection of the document's own fields is built from them and the request's
        // `ProjectionKey` alone, so a repeated document builds it once per key. A distance
        // field reads the request's `nearest`, so a projection with one is built per request.
        let key = crate::graphql::selection_set::ProjectionKey {
            request_chain: crate::runtime::localization_chain(&self.ctx.schema),
        };
        let projection = DocumentProjection {
            schema:          &self.ctx.schema,
            return_type:     &query_match.query_def.return_type,
            database:        self.ctx.adapter.database_type(),
            projected_count: plan.projection_fields.len(),
            decimals:        &self.ctx.decimals,
        };
        match query_match.prepared() {
            Some(prepared) if distance_fields.is_empty() => prepared
                .projection_once(&key, |fields, key| {
                    let root_fields =
                        fields.first().map_or(&[] as &[_], |s| s.nested_fields.as_slice());
                    projection.build(root_fields, Vec::new(), key)
                })
                .map(Some),
            _ => projection.build(root_fields, distance_fields, &key).map(Some),
        }
    }

    /// Execute a regular (non-aggregate) GraphQL query root, for an authenticated caller or
    /// an anonymous one.
    ///
    /// The unified dispatcher and the multi-root fan-out both call this single entry, and
    /// every decision it makes before the database is
    /// [`resolve_root_read`](Self::resolve_root_read)'s, so the authenticated and the
    /// anonymous read cannot drift (H19). It is an `async fn`, so it has one opaque future
    /// type: callers collect homogeneous futures across roots without boxing.
    ///
    /// # Errors
    ///
    /// Everything [`resolve_root_read`](Self::resolve_root_read) returns, the function,
    /// Relay and count runners' errors for those roots, the response ceiling's refusal and
    /// the field authorizer's (#423), and `FraiseQLError::Database` from the adapter.
    pub(in super::super) async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        operation_name: Option<&str>,
    ) -> Result<serde_json::Value> {
        // The operation the request selected, not the document's first one (§ 6.1).
        let query_match =
            self.ctx
                .matcher
                .match_query_for(query, variables, operation_name, security_context)?;
        self.execute_regular_match(query_match, variables, security_context).await
    }

    /// [`execute_regular_query`](Self::execute_regular_query) for a document already
    /// matched: the dispatcher matches a repeated document from what its classification
    /// kept, without parsing it again.
    ///
    /// # Errors
    ///
    /// As [`execute_regular_query`](Self::execute_regular_query), after matching.
    pub(in super::super) async fn execute_regular_match(
        &self,
        query_match: crate::runtime::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        self.execute_regular_match_body(query_match, variables, security_context, false)
            .await
            .and_then(crate::runtime::ResponseBody::into_value)
    }

    /// [`execute_regular_match`](Self::execute_regular_match), answered as the body a
    /// transport sends. With `raw`, a read whose rows need no shaping in Rust is answered
    /// as the bytes the database serialized; completion leaves such a body unchanged.
    ///
    /// # Errors
    ///
    /// As [`execute_regular_match`](Self::execute_regular_match).
    pub(in super::super) async fn execute_regular_match_body(
        &self,
        query_match: crate::runtime::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        raw: bool,
    ) -> Result<crate::runtime::ResponseBody> {
        use crate::runtime::ResponseBody;

        let mut read = match self.resolve_root_read(
            &query_match,
            variables,
            security_context,
            Door::Document,
        )? {
            RootRoute::Function => {
                return self
                    .execute_function_backed_query(&query_match, variables, security_context)
                    .await
                    .map(ResponseBody::Value);
            },
            RootRoute::Relay => {
                let session_vars = self.resolve_session_vars(security_context)?;
                let session_pairs: Vec<(&str, &str)> =
                    session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                return self
                    .execute_relay_query(&query_match, variables, security_context, &session_pairs)
                    .await
                    .map(ResponseBody::Value);
            },
            // A count sibling returns a scalar: no projection, cache or field RBAC applies.
            RootRoute::Count => {
                return self
                    .execute_count_query(&query_match, variables, security_context)
                    .await
                    .map(ResponseBody::Value);
            },
            RootRoute::Rows(read) => *read,
        };
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
        let return_type = &query_match.query_def.return_type;

        // The read, with the session variables pinned to its connection (#329). With nested
        // levels to gate, the same read is the root of a composed statement carrying them.
        let session_vars = std::mem::take(&mut read.session_vars);
        let session_pairs: Vec<(&str, &str)> =
            session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let composed = !read.nested.is_empty();
        let raw_read = if raw {
            self.read_raw(&read, &query_match, &session_pairs).await?
        } else {
            None
        };
        let results = if let Some(raw_read) = raw_read {
            match raw_read {
                super::query_raw::RawRead::Body(body) => return Ok(ResponseBody::Raw(body)),
                super::query_raw::RawRead::Rows(rows) => rows,
            }
        } else if composed {
            self.execute_composed_document_read(
                read.composed_level(),
                &session_pairs,
                query_match.query_def.read_routing,
            )
            .await?
        } else {
            self.ctx
                .adapter
                .execute_with_projection_arc_with_session(
                    &read.projection_request(),
                    &session_pairs,
                    query_match.query_def.read_routing,
                )
                .await?
        };
        let results = self.settle_nearest(
            &query_match,
            read.nearest_k,
            read.nearest_count.is_some(),
            results,
        )?;

        // The response-bytes ceiling (`[validation] max_response_bytes`). GATE-1 scored the
        // document's depth and complexity, but neither knows what a row weighs: a
        // materialised read is one fetch whatever its nesting, and its cost is its bytes.
        if let Some(budget) = read.budget() {
            budget.charge_jsonb_rows(&results)?;
        }

        // Project. Masked fields stay in the projection, in their requested position, and are
        // nulled below — GraphQL requires the response's field order to follow the query's.
        // The rows the field authorizer reads are kept only on the branch that needs them, so
        // the other can give its rows up to the projector instead of cloning them.
        let abstract_read = if read.abstract_read {
            super::query_abstract::AbstractRead::of(&self.ctx.schema, &query_match.query_def)
        } else {
            None
        };
        let mut authorizer_rows = None;
        let mut projected = if let Some(abstract_read) = &abstract_read {
            // Each row as the object type it is, its own masks applied (#1612).
            let mut projected =
                abstract_read.project(&results, root_fields, &self.ctx.schema, &read.selection)?;
            read.selection
                .null_masked(&mut projected, return_type, root_fields, &self.ctx.schema);
            projected
        } else if composed || read.gated_present {
            // Stored documents with their gated levels merged in, or read whole because a
            // policy-gated field bypassed the SQL projection: projected the way a nested list
            // element is, at every depth — aliases, recasing, `__typename`, a localized field
            // in its selection's locale and its translations sibling included.
            let mut projected = super::query_nested::project_documents(
                &results,
                return_type,
                root_fields,
                &self.ctx.schema,
                query_match.query_def.returns_list,
            );
            authorizer_rows = Some(results);
            read.selection
                .null_masked(&mut projected, return_type, root_fields, &self.ctx.schema);
            projected
        } else {
            let projector = ResultProjector::new(read.access().projected.clone())
                // #1192: a `String` field whose text parses as JSON must stay a string.
                .with_declared_scalars(&self.ctx.schema, return_type)
                .configure_typename_from_selections(&query_match.selections, return_type);
            // #1556: the rows are this request's alone unless the row cache holds them too, so
            // their values are moved into the response rather than cloned.
            let mut projected =
                projector.project_results_owned(results, query_match.query_def.returns_list)?;
            // Every level below the root rebuilt from its selection, its masked fields null.
            crate::runtime::shape_projected(
                &mut projected,
                return_type,
                root_fields,
                &self.ctx.schema,
                Some(read.selection.masked_set()),
            );
            projected
        };

        // The dynamic field authorizer (#423), per row, after the static gate —
        // AND-composition: a field is shown only if both allow. Fail-closed: a `Reject` or any
        // policy error refuses the read; the value is never served.
        if read.gated_present {
            let (Some(principal), Some(rows)) = (security_context, authorizer_rows) else {
                return Err(FraiseQLError::Internal {
                    message: "a gated read reached the field authorizer without its principal \
                              or its rows — this is a bug"
                        .to_string(),
                    source:  None,
                });
            };
            self.apply_dynamic_field_authorizer(
                &query_match,
                principal,
                read.access(),
                &rows,
                &mut projected,
            )?;
        }

        Ok(ResponseBody::Value(ResultProjector::wrap_in_data_envelope(
            projected,
            query_match.response_key(),
        )))
    }

    /// Apply the dynamic field authorizer (#423) to a projected result.
    ///
    /// Composes with the **static** scope gate (`requires_scope`) rather than
    /// replacing it: a field is shown only if both allow. Fail-closed throughout — a
    /// `Reject`, a policy error, a missing authorizer, or a gated field nested inside
    /// a sub-selection all refuse rather than serve.
    ///
    /// One method with two call sites since #1329, not two copies. A function-backed
    /// root field returns documents of the same declared type, so a gated field on that
    /// type is gated there too; a read path that skipped this would be a hole in #423
    /// that no test of the SQL path could see.
    ///
    /// Takes a principal by value rather than as an `Option`: a per-row policy
    /// decision needs one. The anonymous path cannot call this at all, which is why it
    /// refuses a selected gated field outright (`deny_if_gated_field_selected`) instead.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Authorization`] on any of the four refusals above.
    pub(super) fn apply_dynamic_field_authorizer(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        security_context: &SecurityContext,
        access: &crate::runtime::field_filter::FieldAccessResult,
        rows: &[crate::backend::types::JsonbValue],
        projected: &mut serde_json::Value,
    ) -> Result<()> {
        use crate::security::field_authorizer as authz;

        let return_type = &query_match.query_def.return_type;
        let root_fields: &[crate::graphql::FieldSelection] =
            query_match.selections.first().map_or(&[], |r| r.nested_fields.as_slice());

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
        if authz::selection_set_has_nested_gated_field(&self.ctx.schema, return_type, root_fields) {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Field-level authorization of nested fields on type '{return_type}' is not \
                     supported in this version"
                ),
                action:   Some("read".to_string()),
                resource: Some(return_type.clone()),
            });
        }
        // `query_match.arguments` is the request's variables, already merged with
        // whole-argument inline values — the same map every other consumer resolves
        // against (#903).
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
        authz::apply_field_authorizer(&pass, rows, projected, query_match.query_def.returns_list)
    }

    /// Execute a pre-built `QueryMatch` directly, bypassing GraphQL string parsing.
    ///
    /// Used by the REST transport for embedded sub-queries and NDJSON streaming
    /// where the query parameters are already resolved from HTTP request parameters.
    ///
    /// `act_on` makes this the read of the rows the server is about to write (#1581,
    /// #1597): bounded at that many rows on top of the page a `limit` argument asked for,
    /// without being held to `max_page_size`, and routed to the primary, which also
    /// bypasses the result cache, whatever the query's own `read_routing` says.
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
        request_budget: Option<&crate::security::RequestBudget>,
        act_on: Option<u32>,
    ) -> Result<serde_json::Value> {
        // A leaf selection of an object field is the whole object, read as its type.
        let expanded = super::query_nested::expand_leaf_objects(&self.ctx.schema, query_match)?;
        let query_match = &*expanded;
        let mut resolved = self.resolve_rows(
            query_match,
            variables,
            security_context,
            Door::Direct {
                gated:   GatedFieldHandling::RefuseAsUnsupported,
                cost:    DirectReadCost::Flat(request_budget),
                nesting: None,
            },
        )?;
        if let Some(bound) = act_on {
            resolved.limit = Some(resolved.limit.map_or(bound, |page| page.min(bound)));
        }
        let routing = if act_on.is_some() {
            crate::backend::types::ReadRouting::Primary
        } else {
            query_match.query_def.read_routing
        };
        // A nested level its type scopes is read with that type's predicate, as for
        // GraphQL: the same read, as the root of a composed statement carrying it.
        let session_vars = std::mem::take(&mut resolved.session_vars);
        let session_pairs: Vec<(&str, &str)> =
            session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let counted = resolved.nested.is_empty() && resolved.nearest_count.is_some();
        let results = if resolved.nested.is_empty() {
            self.ctx
                .adapter
                .execute_with_projection_arc_with_session(
                    &resolved.projection_request(),
                    &session_pairs,
                    routing,
                )
                .await?
        } else {
            self.execute_composed_document_read(resolved.composed_level(), &session_pairs, routing)
                .await?
        };
        let results = self.settle_nearest(query_match, resolved.nearest_k, counted, results)?;

        // The response-bytes ceiling, charged on what came back.
        //
        // `request_budget` is the caller's, and it is present when the caller is a
        // transport that issues **several** reads to answer one request: a REST
        // `?select=` embed resolves one sub-read per parent row per level, so a budget
        // built here would bound each sub-read and leave their aggregate — the quantity
        // that decides whether the response is too large — unbounded. It is borrowed
        // rather than owned for exactly that reason; see the rule on `ResponseBudget`.
        //
        // Absent one, this read *is* the whole response and owns its own budget, which
        // is the same ceiling over the same bytes. One charge site either way, so the
        // two cases cannot come to mean different things.
        let own_budget = if request_budget.is_none() {
            resolved.budget()
        } else {
            None
        };
        if let Some(budget) = request_budget
            .and_then(crate::security::RequestBudget::bytes)
            .or(own_budget.as_ref())
        {
            budget.charge_jsonb_rows(&results)?;
        }

        let projected = self.project_direct_rows(
            query_match,
            &resolved.selection,
            &results,
            query_match.query_def.returns_list,
        )?;

        // Wrap in GraphQL data envelope.
        Ok(ResultProjector::wrap_in_data_envelope(projected, query_match.response_key()))
    }

    /// Resolve a row-shaped read down to the strings the column-shaped adapter
    /// methods take (#1351).
    ///
    /// The row shape is the gRPC transport's: a `Vec<Vec<ColumnValue>>` projected
    /// through protobuf `ColumnSpec`s, not GraphQL-shaped JSON against a selection
    /// set. It resolves through [`resolve_root_read`](Self::resolve_root_read)
    /// like every other read — that is the whole point of the entry — and then
    /// lowers the resolved clause to the `Option<&str>` pair
    /// [`execute_row_query`](DatabaseAdapter::execute_row_query) accepts.
    ///
    /// Returns the resolved read and the columns **narrowed by field-level RBAC**.
    /// The caller must project with the returned columns rather than the ones it
    /// passed in: the adapter zips values to specs positionally, so reading a
    /// narrowed set and encoding with the original one misaligns every row.
    ///
    /// # Errors
    ///
    /// Everything [`resolve_root_read`](Self::resolve_root_read) returns, plus
    /// `FraiseQLError::Unsupported` for the two resolved shapes the row path cannot
    /// carry — a parameterised ordering (`?search=` relevance, #1284) and a SQL
    /// projection hint (a `nearest` vector distance, #959). Both are refused rather
    /// than dropped: a read that silently loses its ordering or its computed column
    /// answers a different question than the one asked.
    fn resolve_row_read(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<RowReadPlan> {
        let resolved = self.resolve_rows(
            query_match,
            variables,
            security_context,
            Door::Direct {
                gated:   GatedFieldHandling::AdjudicatePerRow,
                cost:    DirectReadCost::Flat(None),
                nesting: None,
            },
        )?;

        // The **row-shaped** view, not the query's own `sql_source`.
        //
        // A row read answers scalar columns; `sql_source` names the JSONB document
        // view, whose only column is `data`. Reading that here would not fail —
        // `SELECT *` succeeds and the column extractor simply finds none of the names
        // it is looking for — it would answer every field of every row as `NULL`. A
        // silent wrong answer, which is worse than an error.
        //
        // The convention lives here rather than in the transport because the engine
        // owns which object a read targets, exactly as `resolve_root_read` does for
        // the JSON path. A transport that names a database object is the leak this
        // work removes.
        let type_def =
            self.ctx.schema.find_type(&query_match.query_def.return_type).ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!(
                        "Type '{}' not found in schema",
                        query_match.query_def.return_type
                    ),
                    path:    None,
                }
            })?;
        let view = format!("vr_{}", type_def.sql_source);

        // A column is a leaf. An object or list field named as one would be read as the
        // value its view stores — a nested level with none of its type's gates, since a
        // row has no key beneath a column to classify, mask or row-gate. The transport
        // never names one (`column_specs_from_type` keeps scalars); this entry is public.
        if let Some(nested) = columns.iter().find(|c| {
            type_def.find_field(&c.name).is_some_and(|f| {
                f.field_type.is_list()
                    || matches!(
                        f.field_type,
                        FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_)
                    )
            })
        }) {
            return Err(FraiseQLError::Unsupported {
                message: format!(
                    "Field '{}.{}' is a nested level, which the row-shaped read cannot carry; \
                     it is not available on this transport",
                    type_def.name, nested.name
                ),
            });
        }

        if resolved.projection.is_some() {
            return Err(FraiseQLError::Unsupported {
                message: format!(
                    "Query '{}' resolves to a SQL projection the row-shaped read cannot \
                     carry; it is not available on this transport",
                    query_match.query_def.name
                ),
            });
        }

        // #886: read only what this principal may see. `allowed()` excludes the
        // masked fields, which on the JSON path are projected and then nulled —
        // a column-shaped row has no key to null, so the column is not read.
        let allowed = resolved.access().allowed();
        let narrowed: Vec<fraiseql_db::types::ColumnSpec> = columns
            .iter()
            .filter(|c| allowed.iter().any(|a| a == &c.name))
            .cloned()
            .collect();

        let where_sql = match resolved.composed_where {
            Some(ref clause) => {
                Some(fraiseql_db::where_sql_generator::WhereSqlGenerator::to_sql(clause)?)
            },
            None => None,
        };

        let order_sql = match fraiseql_db::order_by::render_order_by_columns(
            resolved.order_by.as_deref(),
            fraiseql_db::DatabaseType::PostgreSQL,
            1,
            fraiseql_db::order_by::Tiebreak::None,
        )? {
            Some(rendered) if rendered.params.is_empty() => Some(rendered.columns),
            Some(_) => {
                return Err(FraiseQLError::Unsupported {
                    message: format!(
                        "Query '{}' resolves to a parameterised ordering the row-shaped read \
                         cannot carry; it is not available on this transport",
                        query_match.query_def.name
                    ),
                });
            },
            None => None,
        };

        let gate = self.row_field_gate(query_match, security_context, &narrowed)?;
        let localizer = RowLocalizer::new(type_def, &narrowed, &self.ctx.schema);

        Ok(RowReadPlan {
            resolved,
            view,
            columns: narrowed,
            where_sql,
            order_sql,
            gate,
            localizer,
        })
    }

    /// Build the per-row #423 gate for a columnar read, or fail closed (#1351).
    ///
    /// The row path's replacement for `deny_if_gated_field_selected`, which refused a
    /// gated field for every principal — including one the authorizer accepts, which is
    /// the half of #1351's verification gate a refusal-only path cannot pass.
    ///
    /// The three refusals are the ones `apply_dynamic_field_authorizer` already makes on
    /// the JSON path, in the same order and with the same messages, because a deployment
    /// must not get a different answer per shape for the same misconfiguration:
    ///
    /// - a gated field is selected and **no authorizer is configured** — the operator declared a
    ///   gate and installed nothing to decide it;
    /// - a gated field sits **inside a sub-selection** — not supported on either shape;
    /// - a gated field is selected and there is **no principal** — a per-row policy decision needs
    ///   one, and the anonymous caller is exactly who must not be served.
    ///
    /// Returns `None` when the read is not gated, which is every read on a schema that
    /// declares no `authorize` field — the common case, and it costs one index lookup.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Authorization`] on any of the three refusals above.
    fn row_field_gate(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<Option<crate::security::field_authorizer::RowFieldGate>> {
        use crate::security::field_authorizer as authz;

        let return_type = &query_match.query_def.return_type;
        let root_fields: &[crate::graphql::FieldSelection] =
            query_match.selections.first().map_or(&[], |r| r.nested_fields.as_slice());

        if !authz::selection_set_selects_gated_field(&self.ctx.schema, return_type, root_fields) {
            return Ok(None);
        }

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
        if authz::selection_set_has_nested_gated_field(&self.ctx.schema, return_type, root_fields) {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Field-level authorization of nested fields on type '{return_type}' is not \
                     supported in this version"
                ),
                action:   Some("read".to_string()),
                resource: Some(return_type.clone()),
            });
        }
        let Some(principal) = security_context else {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Field-level authorization is required for a selected field on type \
                     '{return_type}' but the request carries no principal"
                ),
                action:   Some("read".to_string()),
                resource: Some(return_type.clone()),
            });
        };

        let gated = authz::collect_top_level_gated_fields(
            &self.ctx.schema,
            return_type,
            root_fields,
            &query_match.arguments,
        )?;

        Ok(authz::RowFieldGate::new(authorizer, principal, return_type, gated, columns))
    }

    /// Execute a row-shaped read through the direct-read chokepoint (#1351).
    ///
    /// The column-shaped twin of
    /// [`execute_query_direct`](Self::execute_query_direct). gRPC used to answer its
    /// reads by calling [`DatabaseAdapter::execute_row_query`] itself, building its
    /// own `WHERE` clause from the request message — a second read implementation,
    /// so the operation `Authorizer` (#422), the `requires_role` gate (#1122), the
    /// actor allow-list (#966), the field gate (#423) and the compiled page-size
    /// ceiling (#421) all applied to every transport except that one.
    ///
    /// # Errors
    ///
    /// Everything [`resolve_row_read`](Self::resolve_row_read) returns, and
    /// `FraiseQLError::Database` if the read fails.
    pub(in super::super) async fn execute_row_read(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<RowRead> {
        let plan = self.resolve_row_read(query_match, variables, security_context, columns)?;
        let session_pairs = plan.resolved.session_pairs();

        let mut rows = self
            .ctx
            .adapter
            .execute_row_query_with_session(
                &plan.view,
                &plan.columns,
                plan.where_sql.as_deref(),
                plan.order_sql.as_deref(),
                plan.resolved.limit,
                plan.resolved.offset,
                &session_pairs,
            )
            .await?;

        // #423 per row (#1351). A `Reject` or a policy error refuses the whole read, so
        // no row of it is encoded; a `Mask` nulls that field on that row only.
        if let Some(ref gate) = plan.gate {
            for row in &mut rows {
                gate.adjudicate_row(&plan.columns, row)?;
            }
        }
        if let Some(ref localizer) = plan.localizer {
            for row in &mut rows {
                localizer.apply(row);
            }
        }

        // The response-bytes ceiling, charged after adjudication: masking can only
        // shrink a row (`ColumnValue::Null`), so charging before it would bill the
        // caller for bytes the gate withheld.
        if let Some(budget) = plan.resolved.budget() {
            for row in &rows {
                budget.charge_column_row(row)?;
            }
        }

        Ok(RowRead {
            columns: plan.columns,
            rows,
        })
    }

    /// The same read as [`execute_row_read`](Self::execute_row_read), delivered one
    /// row at a time (#1351).
    ///
    /// The gRPC server-streaming arm's source. It resolves through the same
    /// function as the collecting arm for the reason
    /// [`resolve_root_read`](Self::resolve_root_read) exists at all: the two
    /// gRPC read arms had already drifted apart once (#1348 found one fail-closed
    /// and the other fail-open on the same RLS failure), and a streaming arm is
    /// where a missing ceiling costs the most.
    ///
    /// # Errors
    ///
    /// The same as [`execute_row_read`](Self::execute_row_read).
    pub(in super::super) async fn stream_row_read(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<StreamedRowRead> {
        let plan = self.resolve_row_read(query_match, variables, security_context, columns)?;
        let session_pairs = plan.resolved.session_pairs();

        let stream = self
            .ctx
            .adapter
            .stream_row_query_with_session(
                &plan.view,
                &plan.columns,
                plan.where_sql.as_deref(),
                plan.order_sql.as_deref(),
                plan.resolved.limit,
                plan.resolved.offset,
                &session_pairs,
            )
            .await?;

        // #423 per frame, wrapped **here** rather than left to the transport (#1351).
        //
        // The frames are adjudicated as they pass through the engine, so the transport
        // receives rows it may encode verbatim and never holds a policy decision of its
        // own. That is the property that stopped the two arms drifting: a streaming arm
        // that enforced in its own loop is a second enforcement site, and #1348 found
        // these exact two arms disagreeing about a failing RLS evaluation.
        //
        // A refusal mid-stream ends the stream with that error — the frames already sent
        // are the rows that were allowed, and the client sees the failure in the
        // trailers. There is no way to un-send a frame, which is the argument for the
        // ceilings that bound how many there can be (#421, and #379 as of this change).
        //
        // The response-bytes ceiling rides in the same wrap, for the same reason: one
        // budget for the whole stream, so the ceiling bounds the response and not each
        // frame on its own. This is the arm where it earns the most — a stream is the
        // only shape that can keep producing after the answer is already too big, and
        // cutting it at the frame that crosses the line is the difference between a
        // bounded refusal and an unbounded send.
        //
        // Charged after adjudication, because masking can only shrink a row and the
        // caller should not be billed for bytes the gate withheld.
        let budget = plan.resolved.budget();
        let stream = match (plan.gate, budget, plan.localizer) {
            (None, None, None) => stream,
            (gate, budget, localizer) => {
                let columns = plan.columns.clone();
                Box::pin(stream.map(move |row| {
                    let mut row = row?;
                    if let Some(ref gate) = gate {
                        gate.adjudicate_row(&columns, &mut row)?;
                    }
                    if let Some(ref localizer) = localizer {
                        localizer.apply(&mut row);
                    }
                    if let Some(ref budget) = budget {
                        budget.charge_column_row(&row)?;
                    }
                    Ok(row)
                })) as fraiseql_db::ColumnRowStream
            },
        };

        Ok(StreamedRowRead {
            columns: plan.columns,
            stream,
        })
    }

    /// The bound a `nearest` search of `k` counts its matches up to, in the same statement
    /// (#1314): only under `verify` / `refuse`.
    pub(super) fn nearest_count_bound(&self, nearest_k: Option<u32>) -> Option<u32> {
        nearest_k.filter(|_| {
            self.ctx.config.nearest_short_result
                != crate::runtime::notices::ShortResultPolicy::Signal
        })
    }

    /// A `nearest` page, settled (#1314): the count it carried stripped (when `counted`),
    /// and its short result signalled, verified or refused. Any other read passes through.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Unsupported` for a verified truncation under `refuse`.
    pub(super) fn settle_nearest(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        nearest_k: Option<u32>,
        counted: bool,
        results: Arc<Vec<crate::backend::JsonbValue>>,
    ) -> Result<Arc<Vec<crate::backend::JsonbValue>>> {
        let Some(k) = nearest_k else {
            return Ok(results);
        };
        let (results, matched) = if counted {
            let (rows, matched) = crate::backend::traits::take_matched(&results);
            (Arc::new(rows), matched)
        } else {
            (results, None)
        };
        crate::runtime::notices::settle_short_nearest(
            self.ctx.config.nearest_short_result,
            query_match.response_key(),
            k,
            results.len(),
            matched,
        )?;
        Ok(results)
    }

    /// Charge a direct read's projection against `[security.cost_budget]
    /// per_request_max` — the caller's request budget when it lends one, a budget of
    /// this read's own otherwise.
    ///
    /// One charge site for a flat read and a composed one, so the two cannot come to
    /// score the same projection differently.
    ///
    /// # Errors
    ///
    /// Returns the cost budget's refusal when this charge crosses the ceiling.
    pub(super) fn charge_direct_read_cost(
        &self,
        query_name: &str,
        projection: &crate::graphql::DirectReadProjection,
        request_budget: Option<&crate::security::RequestBudget>,
    ) -> Result<()> {
        let own_cost = match request_budget {
            Some(_) => None,
            None => crate::security::CostBudget::new(self.ctx.config.max_operation_cost),
        };
        if let Some(budget) = request_budget
            .and_then(crate::security::RequestBudget::cost)
            .or(own_cost.as_ref())
        {
            let cost = crate::graphql::estimate_direct_read_cost(
                query_name,
                &self.ctx.schema.operation_cost_weights,
                projection,
            ) as u64;
            budget.charge(cost)?;
        }
        Ok(())
    }

    /// The same read as [`execute_query_direct`](Self::execute_query_direct),
    /// delivered one projected row at a time (#958).
    ///
    /// Every row is resolved through [`resolve_root_read`](Self::resolve_root_read)
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
    /// Returns everything [`resolve_root_read`](Self::resolve_root_read) does,
    /// plus `FraiseQLError::Database` for a read that fails before its first row. A
    /// failure after that arrives as an `Err` item in the stream.
    pub(in super::super) async fn stream_query_direct(
        &self,
        query_match: crate::runtime::matcher::QueryMatch,
        variables: Option<serde_json::Value>,
        security_context: Option<SecurityContext>,
    ) -> Result<crate::runtime::JsonRowStream> {
        let query_match =
            super::query_nested::expand_leaf_objects(&self.ctx.schema, &query_match)?.into_owned();
        let resolved = self.resolve_rows(
            &query_match,
            variables.as_ref(),
            security_context.as_ref(),
            Door::Direct {
                gated:   GatedFieldHandling::RefuseAsUnsupported,
                cost:    DirectReadCost::Flat(None),
                nesting: None,
            },
        )?;
        // A function root is read as the rows of one call, which a stream does not
        // bind.
        if resolved.source_args.is_some() {
            return Err(super::root_read::refuse_function_root(
                &query_match.query_def.name,
                "a stream",
            ));
        }
        // A nested level its type scopes needs a composed statement, which a stream does
        // not read: refused rather than streamed ungated.
        if !resolved.nested.is_empty() {
            return Err(FraiseQLError::Unsupported {
                message: format!(
                    "a streamed read of '{}' selects a nested object whose type scopes its \
                     rows, which only a buffered read can gate; select fewer fields",
                    query_match.query_def.name
                ),
            });
        }
        let rows = {
            let session_pairs = resolved.session_pairs();
            self.ctx
                .adapter
                .stream_with_projection(
                    // A stream cannot carry a count: never asked for one (#1314).
                    &crate::backend::ProjectionRequest {
                        matched_up_to: None,
                        ..resolved.projection_request()
                    },
                    &session_pairs,
                    query_match.query_def.read_routing,
                )
                .await?
        };

        let runner = Self::new(Arc::clone(&self.ctx));
        // One budget for the whole export, kept across frames. An NDJSON/CSV/XLSX
        // export is the largest read this engine serves on purpose, so the ceiling
        // has to cut it where it crosses rather than let it run: `max_page_size`
        // does not bound a stream, which is the point of a stream.
        let budget = resolved.budget();
        let access = resolved.selection;
        Ok(Box::pin(rows.map(move |row| {
            let row = row?;
            if let Some(ref budget) = budget {
                budget.charge(crate::security::json_bytes(row.as_value()))?;
            }
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
    pub(super) fn project_direct_rows(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        selection: &super::query_nested::SelectionAccess,
        rows: &[crate::backend::types::JsonbValue],
        returns_list: bool,
    ) -> Result<serde_json::Value> {
        let access = &selection.root;
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
        // Masked fields stay in the projection, in their requested position, and are
        // nulled below — the response keeps the key and withholds only the value, as
        // both GraphQL paths do.
        let projector = ResultProjector::new(access.projected.clone())
            // #1192: a `String` field whose text parses as JSON must stay a string.
            .with_declared_scalars(&self.ctx.schema, &query_match.query_def.return_type)
            .configure_typename_from_selections(
                &query_match.selections,
                &query_match.query_def.return_type,
            );
        let mut projected = projector.project_results(rows, returns_list)?;
        // Every level below the root rebuilt from its selection, its masked fields null, as
        // for a GraphQL read. A root object field a leaf selection expanded arrives with its
        // stored keys, which the walk reads where its response keys are absent.
        crate::runtime::shape_projected(
            &mut projected,
            &query_match.query_def.return_type,
            root_fields,
            &self.ctx.schema,
            Some(selection.masked_set()),
        );
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
        // The same resolution as the rows it counts (#739, #938): the operation gate, the
        // row scope, the parent scoping, the client's filter and the explicit arguments. A
        // count is a read — it reports the cardinality of a set the caller may not select
        // from — so a door that counted on fewer conditions than its list is an oracle.
        let read = self.resolve_rows(query_match, variables, security_context, Door::Count)?;

        // `SELECT COUNT(*)`, pinning the session variables to the read's connection so a
        // policy counts the rows it filters (#329). The same method the GraphQL `<name>Count`
        // sibling uses: one count path, so the two surfaces cannot disagree.
        self.ctx
            .adapter
            .count_where_query(
                &read.sql_source,
                read.composed_where.as_ref(),
                &read.session_pairs(),
                query_match.query_def.read_routing,
            )
            .await
    }
}

#[cfg(test)]
#[path = "query_nested_gates_tests.rs"]
mod query_nested_gates_tests;

#[cfg(test)]
#[path = "query_nested_authz_tests.rs"]
mod query_nested_authz_tests;

#[cfg(test)]
#[path = "root_read_guard_tests.rs"]
mod root_read_guard_tests;

/// What a read's projection is built from that is fixed for its document: with the
/// document's fields and a [`ProjectionKey`](crate::graphql::selection_set::ProjectionKey),
/// everything [`DocumentProjection::build`] reads.
struct DocumentProjection<'a> {
    schema:          &'a crate::schema::CompiledSchema,
    return_type:     &'a str,
    database:        crate::db::DatabaseType,
    projected_count: usize,
    decimals:        &'a crate::runtime::DecimalIndex,
}

impl DocumentProjection<'_> {
    /// The projection of `root_fields`, plus `computed` fields, under `key`.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Internal` when the projection cannot be built. That is an error, not
    /// a licence to select every column: the Rust projector subsets top-level keys but
    /// returns nested objects whole, so the fallback leaked sub-blobs the client never
    /// asked for (#827's family).
    fn build(
        &self,
        root_fields: &[crate::graphql::FieldSelection],
        computed: Vec<fraiseql_db::projection_generator::ProjectionField>,
        key: &crate::graphql::selection_set::ProjectionKey,
    ) -> Result<KeptProjection> {
        // #1554: a projection of stored keys alone can be cut to its page and detoasted
        // before it is projected. A computed field (a vector distance) reads native columns
        // the paged subquery does not expose, so that read keeps the single-`SELECT` shape.
        let reads_only_document = computed.is_empty();
        let mut typed_fields = build_typed_projection_fields(
            root_fields,
            self.schema,
            self.return_type,
            key.request_chain.as_deref(),
        );
        merge_computed_fields(&mut typed_fields, computed);
        // Decided from the fields the SQL is generated from, so the two cannot disagree.
        let decimals_as_text =
            projects_decimals_as_text(&typed_fields, self.schema, self.return_type, self.decimals);
        let projection_sql = PostgresProjectionGenerator::new()
            .generate_typed_projection_sql(&typed_fields)
            .map_err(|e| FraiseQLError::Internal {
                message: format!(
                    "could not build a projection for type '{}': {e}",
                    self.return_type
                ),
                source:  None,
            })?;
        let hint = SqlProjectionHint::new(
            self.database,
            projection_sql,
            compute_projection_reduction(self.projected_count),
        );
        let hint = if reads_only_document {
            hint.reading_only_the_document()
        } else {
            hint
        };
        Ok(KeptProjection {
            hint: Arc::new(hint),
            decimals_as_text,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod document_projection_tests {
    use super::*;
    use crate::{
        graphql::{FieldSelection, selection_set::ProjectionKey},
        schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig, TypeDefinition},
    };

    fn leaf(name: &str) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: Vec::new(),
            directives:    Vec::new(),
        }
    }

    /// The projection reads what a request can vary only through its key: built under one
    /// key, it is the same SQL whatever locale the request around it runs in, so a kept
    /// projection keyed by that key cannot be another request's.
    #[tokio::test]
    async fn a_document_projection_reads_the_request_only_through_its_key() {
        let mut schema = CompiledSchema::default();
        schema.types.push({
            let mut t = TypeDefinition::new("User", "v_user");
            let mut name = FieldDefinition::nullable("name", FieldType::String);
            name.localized = true;
            t.fields = vec![FieldDefinition::new("id", FieldType::Int), name];
            t
        });
        schema.locale = Some(
            LocaleConfig::new(
                "en-US",
                ["en-US", "fr-FR", "de-DE"].map(String::from).to_vec(),
                std::collections::BTreeMap::new(),
                Vec::new(),
            )
            .unwrap(),
        );
        let projection = DocumentProjection {
            schema:          &schema,
            return_type:     "User",
            database:        crate::db::DatabaseType::PostgreSQL,
            projected_count: 2,
            decimals:        &crate::runtime::DecimalIndex::default(),
        };
        let key = ProjectionKey {
            request_chain: Some(vec!["fr-FR".to_string(), "en-US".to_string()]),
        };
        let fields = [leaf("id"), leaf("name")];
        let outside = projection.build(&fields, Vec::new(), &key).unwrap();
        for locale in ["de-DE", "en-US"] {
            let inside = crate::runtime::with_request_locale(locale, async {
                projection.build(&fields, Vec::new(), &key).unwrap()
            })
            .await;
            assert_eq!(
                inside.hint.projection_template, outside.hint.projection_template,
                "a projection built in {locale} read the request's locale around its key"
            );
        }
        assert!(
            outside.hint.projection_template.contains("'fr-FR'"),
            "{}",
            outside.hint.projection_template
        );
    }
}
