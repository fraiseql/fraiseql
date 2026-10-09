//! Aggregate and window query execution runner.

use std::sync::Arc;

use super::super::context::ExecutorContext;
use crate::{
    backend::WhereClause,
    error::{FraiseQLError, Result},
    runtime::suggest_similar,
    security::{RlsWhereClause, SecurityContext, rls_policy::RlsTarget},
};

/// Give every `ORDER BY` key that names a **text** group-by output `collation` (#1512): a
/// JSONB or calendar dimension (both `->>` extractions), or a native dimension whose cast
/// is text. Temporal buckets, tree levels and aggregates sort by their own type.
fn collate_text_group_keys(
    request: &mut crate::compiler::aggregation::AggregationRequest,
    collation: &str,
) {
    use crate::compiler::aggregation::GroupBySelection;

    let text_aliases: Vec<&str> = request
        .group_by
        .iter()
        .filter_map(|selection| match selection {
            GroupBySelection::Dimension { alias, .. }
            | GroupBySelection::CalendarDimension { alias, .. } => Some(alias.as_str()),
            GroupBySelection::NativeDimension { alias, pg_cast, .. }
                if super::query_projection::sorts_as_text(pg_cast) =>
            {
                Some(alias.as_str())
            },
            _ => None,
        })
        .collect();
    for clause in &mut request.order_by {
        if text_aliases.contains(&clause.field.as_str()) {
            clause.collation = Some(collation.to_string());
        }
    }
}

/// The stored keys of the localized fields of `metadata`'s type, as dimension keys (#1524).
fn localized_dimension_keys(
    schema: &crate::schema::CompiledSchema,
    metadata: &crate::compiler::fact_table::FactTableMetadata,
) -> Vec<String> {
    use crate::compiler::fact_table::dimension_key;
    metadata
        .type_name
        .as_deref()
        .and_then(|name| schema.find_type(name))
        .map_or_else(Vec::new, |t| {
            t.fields
                .iter()
                .filter(|f| f.localized)
                .map(|f| dimension_key(f.name.as_str()))
                .collect()
        })
}

/// Whether a dimension `path` reads one of `keys` (a localized field), whole.
fn reads_localized(path: &[String], keys: &[String]) -> bool {
    matches!(path, [key] if keys.contains(&crate::compiler::fact_table::dimension_key(key)))
}

/// Group a localized dimension by its label through `chain` (#1524): the stored locale map
/// would make every distinct map a group, and report the map as the key.
fn localize_group_by(
    plan: &mut crate::compiler::aggregation::AggregationPlan,
    keys: &[String],
    chain: &[String],
) {
    use crate::compiler::aggregation::GroupByExpression;
    for expr in &mut plan.group_by_expressions {
        if let GroupByExpression::JsonbPath {
            path, localized, ..
        } = expr
        {
            if reads_localized(path, keys) {
                *localized = Some(chain.to_vec());
            }
        }
    }
}

/// Read a filter on a localized dimension through `chain`, under `collation` (#1524): the
/// label is compared, as on any read of the field, never the stored map.
fn localize_where(
    clause: WhereClause,
    keys: &[String],
    chain: &[String],
    collation: Option<&str>,
) -> WhereClause {
    match clause {
        WhereClause::Field { ref path, .. } if reads_localized(path, keys) => {
            WhereClause::Localized {
                chain:     chain.to_vec(),
                collation: collation.map(str::to_string),
                inner:     Box::new(clause),
            }
        },
        WhereClause::And(clauses) => WhereClause::And(
            clauses.into_iter().map(|c| localize_where(c, keys, chain, collation)).collect(),
        ),
        WhereClause::Or(clauses) => WhereClause::Or(
            clauses.into_iter().map(|c| localize_where(c, keys, chain, collation)).collect(),
        ),
        WhereClause::Not(inner) => {
            WhereClause::Not(Box::new(localize_where(*inner, keys, chain, collation)))
        },
        other => other,
    }
}

/// Runner for aggregate and window analytics queries.
pub(in super::super) struct AggregateRunner {
    ctx: Arc<ExecutorContext>,
}

impl AggregateRunner {
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext>) -> Self {
        Self { ctx }
    }

    /// Resolve configured session variables for `security_context` into owned
    /// `(name, value)` pairs, for passing to the connection-affine
    /// `*_with_session` adapter methods so `current_setting()`-backed RLS on
    /// aggregate views is effective (#329).
    fn resolve_session_vars(
        &self,
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<(String, String)>> {
        crate::runtime::executor::support::security::read_session_variables(
            &self.ctx.schema,
            security_context,
        )
    }

    /// The collation a text key sorts under in this request's locale, when the schema
    /// declares `[locale]` (#1512).
    fn request_collation(&self) -> Option<String> {
        let schema = &self.ctx.schema;
        schema
            .locale
            .as_ref()
            .zip(crate::runtime::request_locale(schema))
            .and_then(|(config, locale)| config.collation(&locale))
    }

    /// Refuse a read with no principal when a row policy is configured (ruling AB 3).
    ///
    /// The policy cannot be evaluated without a principal, and the composition below applies
    /// it only when there is one: an anonymous aggregate or window read every row. The regular
    /// read has refused this since #784; this is the same refusal, worded the same way (the
    /// read is not advertised to a caller who cannot make it). It sits in the two executors,
    /// so the GraphQL dispatch and the public embedder entries — which pass no principal —
    /// both meet it.
    fn refuse_anonymous_under_a_row_policy(
        &self,
        query_name: &str,
        security_context: Option<&SecurityContext>,
    ) -> Result<()> {
        if security_context.is_none() && self.ctx.config.rls_policy.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!("Query '{query_name}' not found in schema"),
                path:    None,
            });
        }
        Ok(())
    }

    /// Execute an aggregate query dispatch.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — the query name does not end with `_aggregate`, or the
    ///   derived fact table is not found in the compiled schema.
    /// * Propagates errors from [`execute_aggregate_query`](Self::execute_aggregate_query).
    pub(in super::super) async fn execute_aggregate_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // Extract table name from query name (e.g., "sales_aggregate" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_aggregate").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid aggregate query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata = self.ctx.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.ctx.schema.list_fact_tables();
            let suggestion = suggest_similar(&fact_table_name, &known);
            let base = format!("Fact table '{}' not found in schema", fact_table_name);
            let message = match suggestion.as_slice() {
                [s] => format!("{base}. Did you mean '{s}'?"),
                _ => base,
            };
            FraiseQLError::Validation {
                message,
                path: Some(format!("fact_tables.{}", fact_table_name)),
            }
        })?;

        // Parse query variables into aggregate query JSON
        let empty_json = serde_json::json!({});
        let query_json = variables.unwrap_or(&empty_json);

        // Execute aggregate query
        self.execute_aggregate_query(query_json, query_name, metadata, security_context)
            .await
    }

    /// Execute a window query dispatch.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — the query name does not end with `_window`, or the derived
    ///   fact table is not found in the compiled schema.
    /// * Propagates errors from [`execute_window_query`](Self::execute_window_query).
    pub(in super::super) async fn execute_window_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // Extract table name from query name (e.g., "sales_window" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_window").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid window query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata = self.ctx.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.ctx.schema.list_fact_tables();
            let suggestion = suggest_similar(&fact_table_name, &known);
            let base = format!("Fact table '{}' not found in schema", fact_table_name);
            let message = match suggestion.as_slice() {
                [s] => format!("{base}. Did you mean '{s}'?"),
                _ => base,
            };
            FraiseQLError::Validation {
                message,
                path: Some(format!("fact_tables.{}", fact_table_name)),
            }
        })?;

        // Parse query variables into window query JSON
        let empty_json = serde_json::json!({});
        let query_json = variables.unwrap_or(&empty_json);

        // Execute window query
        self.execute_window_query(query_json, query_name, metadata, security_context)
            .await
    }

    /// Execute an aggregate query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the aggregate query
    /// * `query_name` - GraphQL field name (e.g., "`sales_aggregate`")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// When `security_context` is `Some`, evaluates the configured RLS policy and
    /// AND-composes the resulting WHERE clause with the user-supplied WHERE before
    /// planning. RLS conditions are always placed first so they cannot be bypassed.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - RLS policy evaluation fails
    /// - Query parsing fails
    /// - Execution plan generation fails
    /// - SQL generation fails
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and compiled fact table metadata.
    /// // See: tests/integration/ for runnable examples.
    /// # use serde_json::json;
    /// let query_json = json!({
    ///     "table": "tf_sales",
    ///     "groupBy": { "category": true },
    ///     "aggregates": [{"count": {}}]
    /// });
    /// // let result = executor.execute_aggregate_query(&query_json, "sales_aggregate", &metadata).await?;
    /// ```
    /// This aggregate's compiled read routing (#957), or the server's policy when
    /// no compiled query carries the name.
    ///
    /// Aggregates dispatch by *name* against fact-table metadata rather than
    /// through a `QueryDefinition`, so the annotation has to be looked back up.
    /// Falling back to `Any` is the same answer every query gave before the field
    /// existed — an aggregate nobody annotated keeps following server policy.
    fn aggregate_read_routing(&self, query_name: &str) -> crate::backend::types::ReadRouting {
        self.ctx
            .schema
            .queries
            .iter()
            .find(|q| q.name == query_name)
            .map_or(crate::backend::types::ReadRouting::Any, |q| q.read_routing)
    }

    pub(in super::super) async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        self.refuse_anonymous_under_a_row_policy(query_name, security_context)?;

        // 1. Parse JSON query into AggregationRequest. Build native_columns from
        //    denormalized_filters so the parser can emit direct column references instead of JSONB
        //    extraction for native columns.
        let native_columns = crate::runtime::native_columns::filter_columns_to_native_map(
            &metadata.denormalized_filters,
        );
        let mut request =
            crate::runtime::AggregateQueryParser::parse(query_json, metadata, &native_columns)?;
        // #1306: an offset over groups is held to `[validation] max_offset` as a list's is.
        request.offset = super::query_params::enforce_max_offset(
            request.offset,
            self.ctx.config.max_offset,
            "offset",
        )?;
        // #1512: an ORDER BY on a text group-by key sorts under the request locale's collation.
        if let Some(collation) = self.request_collation() {
            collate_text_group_keys(&mut request, &collation);
        }

        // 1a. A linked fact table is read as its type (ruling AB 2): every name the request
        //     references must be a field the caller may read. Before the policy is composed,
        //     so its own predicate is never classified.
        super::aggregate_gates::refuse_unreadable_aggregate(
            &self.ctx.schema,
            metadata,
            &request,
            security_context,
        )?;

        // 1a''. A localized dimension is read as its label in the request locale (#1524): in the
        //       caller's filter here, in the grouping once planned. The policy's own clause is
        //       composed below and never names one.
        let localized_keys = localized_dimension_keys(&self.ctx.schema, metadata);
        let chain = crate::runtime::localization_chain(&self.ctx.schema).unwrap_or_default();
        if !localized_keys.is_empty() {
            if let Some(clause) = request.where_clause.take() {
                let collation = self.request_collation();
                request.where_clause =
                    Some(localize_where(clause, &localized_keys, &chain, collation.as_deref()));
            }
        }

        // 1a'. Node-id filters resolve through the hierarchy their column declares (#1498).
        //      The caller's clause only: the policy below never carries one.
        if let Some(clause) = request.where_clause.take() {
            request.where_clause = Some(super::aggregate_hierarchy::attach_hierarchies(
                clause,
                metadata,
                self.ctx.schema.hierarchies_config.as_ref(),
            )?);
        }

        // 1b. Evaluate RLS policy and compose with user-supplied WHERE.
        //     RLS WHERE is always AND-composed first so it cannot be bypassed.
        if let Some(ctx) = security_context {
            let rls_where: Option<RlsWhereClause> = if let Some(ref policy) =
                self.ctx.config.rls_policy
            {
                // SECURITY (#795): look the policy up by the fact table the root field
                // resolved, never the client's `table` key. This runs *before* the
                // planner's reconciliation check, so it must be correct on its own:
                // an unpolicied name returns `None`, which composed no WHERE clause at
                // all and silently dropped the tenant filter.
                policy.evaluate(ctx, &RlsTarget::fact_table(query_name, &metadata.table_name))?
            } else {
                None
            };
            request.where_clause = match (
                rls_where.map(RlsWhereClause::into_where_clause),
                request.where_clause.take(),
            ) {
                (Some(rls), Some(user)) => Some(WhereClause::And(vec![rls, user])),
                (Some(rls), None) => Some(rls),
                (None, user) => user,
            };
        }

        // 2. Check partial-period dispatch — if conditions are met, generate UNION ALL SQL instead
        //    of a single SELECT.
        let today = chrono::Utc::now().date_naive();
        if let Some((lower_bound, pp_config)) =
            crate::runtime::partial_period::should_use_partial_period(
                metadata,
                request.where_clause.as_ref(),
                today,
            )
        {
            return self
                .execute_partial_period_aggregate(
                    &request,
                    metadata,
                    pp_config,
                    lower_bound,
                    today,
                    query_name,
                    security_context,
                )
                .await;
        }

        // 3. Standard path: generate execution plan
        let mut plan =
            crate::compiler::aggregation::AggregationPlanner::plan(request, metadata.clone())?;
        localize_group_by(&mut plan, &localized_keys, &chain);

        // 4. Generate parameterized SQL
        let sql_generator =
            crate::runtime::AggregationSqlGenerator::new(self.ctx.adapter.database_type());
        let parameterized = sql_generator.generate_parameterized(&plan)?;

        // 5. Execute with bind parameters (eliminates escape-based injection risk), pinning session
        //    variables to the connection for current_setting() RLS (#329).
        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let routing = self.aggregate_read_routing(query_name);
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate_with_session(
                &parameterized.sql,
                &parameterized.params,
                &session_pairs,
                routing,
            )
            .await?;

        // 6. Project results
        let projected = crate::runtime::AggregationProjector::project(rows, &plan)?;

        // 7. Wrap in GraphQL data envelope
        let response =
            crate::runtime::AggregationProjector::wrap_in_data_envelope(projected, query_name);

        // 8. Serialize to JSON string
        Ok(response)
    }

    /// Execute an aggregate query via partial-period UNION ALL.
    ///
    /// Generates a UNION ALL query combining fine-grain and coarse-grain branches,
    /// then executes and projects the result identically to the standard path.
    ///
    /// # Errors
    ///
    /// Returns error if plan generation, SQL generation, or database execution fails.
    #[allow(clippy::too_many_arguments)] // Reason: all arguments are semantically required
    async fn execute_partial_period_aggregate(
        &self,
        request: &crate::compiler::aggregation::AggregationRequest,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
        config: &crate::compiler::fact_table::PartialPeriodConfig,
        lower_bound: chrono::NaiveDate,
        today: chrono::NaiveDate,
        query_name: &str,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        let branch_plan = crate::runtime::partial_period::determine_branches(
            lower_bound,
            config.time_grain_trunc,
            today,
        );

        // Split the WHERE clause to separate the date condition from the rest
        let extra_where = request
            .where_clause
            .as_ref()
            .and_then(|wc| {
                crate::runtime::partial_period::split_where_clause(wc, &config.time_grain_column)
            })
            .and_then(|split| split.remaining);

        // Generate execution plan (for GROUP BY / aggregate expression resolution)
        let mut plan = crate::compiler::aggregation::AggregationPlanner::plan(
            request.clone(),
            metadata.clone(),
        )?;
        // A localized dimension groups by its label (#1524), as on the standard path.
        localize_group_by(
            &mut plan,
            &localized_dimension_keys(&self.ctx.schema, metadata),
            &crate::runtime::localization_chain(&self.ctx.schema).unwrap_or_default(),
        );

        // Generate UNION ALL SQL
        let sql_generator =
            crate::runtime::AggregationSqlGenerator::new(self.ctx.adapter.database_type());
        let union_sql = sql_generator.generate_partial_period(
            &plan,
            config,
            &branch_plan,
            extra_where.as_ref(),
        )?;

        // Execute, pinning session variables to the connection so a
        // `current_setting()`-backed RLS policy constrains the partial-period branch the
        // same way it constrains the standard aggregate and window paths (#610).
        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let routing = self.aggregate_read_routing(query_name);
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate_with_session(
                &union_sql.sql,
                &union_sql.params,
                &session_pairs,
                routing,
            )
            .await?;

        // Project and wrap (same as standard path)
        let projected = crate::runtime::AggregationProjector::project(rows, &plan)?;
        let response =
            crate::runtime::AggregationProjector::wrap_in_data_envelope(projected, query_name);

        Ok(response)
    }

    /// Execute a window query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the window query
    /// * `query_name` - GraphQL field name (e.g., "`sales_window`")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// When `security_context` is `Some`, evaluates the configured RLS policy and
    /// AND-composes the resulting WHERE clause with the user-supplied WHERE before
    /// planning. RLS conditions are always placed first so they cannot be bypassed.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - RLS policy evaluation fails
    /// - Query parsing fails
    /// - Execution plan generation fails
    /// - SQL generation fails
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live database adapter and compiled fact table metadata.
    /// // See: tests/integration/ for runnable examples.
    /// # use serde_json::json;
    /// let query_json = json!({
    ///     "table": "tf_sales",
    ///     "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
    ///     "windows": [{
    ///         "function": {"type": "row_number"},
    ///         "alias": "rank",
    ///         "partitionBy": [{"type": "dimension", "path": "category"}],
    ///         "orderBy": [{"field": "revenue", "direction": "DESC"}]
    ///     }]
    /// });
    /// // let result = executor.execute_window_query(&query_json, "sales_window", &metadata).await?;
    /// ```
    pub(in super::super) async fn execute_window_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        self.refuse_anonymous_under_a_row_policy(query_name, security_context)?;

        // 1. Parse JSON query into WindowRequest
        let mut request = crate::runtime::WindowQueryParser::parse(query_json, metadata)?;
        // #1306: as an aggregate's.
        request.offset = super::query_params::enforce_max_offset(
            request.offset,
            self.ctx.config.max_offset,
            "offset",
        )?;

        // 1a. A linked fact table is read as its type (ruling AB 2), as for an aggregate.
        super::aggregate_gates::refuse_unreadable_window(
            &self.ctx.schema,
            metadata,
            &request,
            security_context,
        )?;

        // 1a'. A localized dimension is read as its label (#1524): in the caller's filter here,
        //      and wherever the plan selects, partitions or orders by it.
        let localized_keys = localized_dimension_keys(&self.ctx.schema, metadata);
        let chain = crate::runtime::localization_chain(&self.ctx.schema).unwrap_or_default();
        let collation = self.request_collation();
        if !localized_keys.is_empty() {
            if let Some(clause) = request.where_clause.take() {
                request.where_clause =
                    Some(localize_where(clause, &localized_keys, &chain, collation.as_deref()));
            }
        }

        // 1b. Evaluate RLS policy and compose with user-supplied WHERE.
        //     RLS WHERE is always AND-composed first so it cannot be bypassed.
        if let Some(ctx) = security_context {
            let rls_where: Option<RlsWhereClause> = if let Some(ref policy) =
                self.ctx.config.rls_policy
            {
                // SECURITY (#795): look the policy up by the fact table the root field
                // resolved, never the client's `table` key. This runs *before* the
                // planner's reconciliation check, so it must be correct on its own:
                // an unpolicied name returns `None`, which composed no WHERE clause at
                // all and silently dropped the tenant filter.
                policy.evaluate(ctx, &RlsTarget::fact_table(query_name, &metadata.table_name))?
            } else {
                None
            };
            request.where_clause = match (
                rls_where.map(RlsWhereClause::into_where_clause),
                request.where_clause.take(),
            ) {
                (Some(rls), Some(user)) => Some(WhereClause::And(vec![rls, user])),
                (Some(rls), None) => Some(rls),
                (None, user) => user,
            };
        }

        // 2. Generate execution plan (validates semantic names against metadata) Text keys sort
        //    under the request locale's collation (#1512).
        let plan = crate::compiler::window_functions::WindowPlanner::plan_in_locale(
            request,
            metadata,
            crate::compiler::window_functions::WindowLocale {
                collation: collation.as_deref(),
                localized: &localized_keys,
                chain:     &chain,
            },
        )?;

        // 3. Generate SQL
        let sql_generator =
            crate::runtime::WindowSqlGenerator::new(self.ctx.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL — bind parameters via execute_parameterized_aggregate so WHERE clause
        //    values are passed as prepared-statement parameters, not inlined. Session variables are
        //    pinned to the connection for current_setting() RLS (#329).
        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let routing = self.aggregate_read_routing(query_name);
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate_with_session(
                &sql.raw_sql,
                &sql.parameters,
                &session_pairs,
                routing,
            )
            .await?;

        // 5. Project results
        let projected = crate::runtime::WindowProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response =
            crate::runtime::WindowProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(response)
    }
}

#[cfg(test)]
#[path = "aggregate_tests.rs"]
mod aggregate_rls_tests;
