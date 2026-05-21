//! Aggregate and window query execution runner.

use std::sync::Arc;

use super::super::context::ExecutorContext;
use crate::{
    db::{WhereClause, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    runtime::suggest_similar,
    security::{RlsWhereClause, SecurityContext},
};

/// Runner for aggregate and window analytics queries.
pub(in super::super) struct AggregateRunner<A: DatabaseAdapter> {
    ctx: Arc<ExecutorContext<A>>,
}

impl<A: DatabaseAdapter> AggregateRunner<A> {
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext<A>>) -> Self {
        Self { ctx }
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
    pub(in super::super) async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // 1. Parse JSON query into AggregationRequest. Build native_columns from
        //    denormalized_filters so the parser can emit direct column references instead of JSONB
        //    extraction for native columns.
        let native_columns = crate::runtime::native_columns::filter_columns_to_native_map(
            &metadata.denormalized_filters,
        );
        let mut request =
            crate::runtime::AggregateQueryParser::parse(query_json, metadata, &native_columns)?;

        // 1b. Evaluate RLS policy and compose with user-supplied WHERE.
        //     RLS WHERE is always AND-composed first so it cannot be bypassed.
        if let Some(ctx) = security_context {
            let rls_where: Option<RlsWhereClause> =
                if let Some(ref policy) = self.ctx.config.rls_policy {
                    policy.evaluate(ctx, &request.table_name)?
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
                )
                .await;
        }

        // 3. Standard path: generate execution plan
        let plan =
            crate::compiler::aggregation::AggregationPlanner::plan(request, metadata.clone())?;

        // 4. Generate parameterized SQL
        let sql_generator =
            crate::runtime::AggregationSqlGenerator::new(self.ctx.adapter.database_type());
        let parameterized = sql_generator.generate_parameterized(&plan)?;

        // 5. Execute with bind parameters (eliminates escape-based injection risk)
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate(&parameterized.sql, &parameterized.params)
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
        let plan = crate::compiler::aggregation::AggregationPlanner::plan(
            request.clone(),
            metadata.clone(),
        )?;

        // Generate UNION ALL SQL
        let sql_generator =
            crate::runtime::AggregationSqlGenerator::new(self.ctx.adapter.database_type());
        let union_sql = sql_generator.generate_partial_period(
            &plan,
            config,
            &branch_plan,
            extra_where.as_ref(),
        )?;

        // Execute
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate(&union_sql.sql, &union_sql.params)
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
        // 1. Parse JSON query into WindowRequest
        let mut request = crate::runtime::WindowQueryParser::parse(query_json, metadata)?;

        // 1b. Evaluate RLS policy and compose with user-supplied WHERE.
        //     RLS WHERE is always AND-composed first so it cannot be bypassed.
        if let Some(ctx) = security_context {
            let rls_where: Option<RlsWhereClause> =
                if let Some(ref policy) = self.ctx.config.rls_policy {
                    policy.evaluate(ctx, &request.table_name)?
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

        // 2. Generate execution plan (validates semantic names against metadata)
        let plan = crate::compiler::window_functions::WindowPlanner::plan(request, metadata)?;

        // 3. Generate SQL
        let sql_generator =
            crate::runtime::WindowSqlGenerator::new(self.ctx.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL — bind parameters via execute_parameterized_aggregate so WHERE clause
        //    values are passed as prepared-statement parameters, not inlined.
        let rows = self
            .ctx
            .adapter
            .execute_parameterized_aggregate(&sql.raw_sql, &sql.parameters)
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
