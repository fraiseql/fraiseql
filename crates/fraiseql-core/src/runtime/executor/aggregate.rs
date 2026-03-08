//! Aggregate and window query execution.

use crate::{
    error::{FraiseQLError, Result},
    runtime::suggest_similar,
};

use super::Executor;
use crate::db::traits::DatabaseAdapter;

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute an aggregate query dispatch.
    pub(super) async fn execute_aggregate_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Extract table name from query name (e.g., "sales_aggregate" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_aggregate").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid aggregate query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata = self.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.schema.list_fact_tables();
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
        self.execute_aggregate_query(query_json, query_name, metadata).await
    }

    /// Execute a window query dispatch.
    pub(super) async fn execute_window_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Extract table name from query name (e.g., "sales_window" -> "tf_sales")
        let table_name =
            query_name.strip_suffix("_window").ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid window query name: {}", query_name),
                path:    None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata = self.schema.get_fact_table(&fact_table_name).ok_or_else(|| {
            let known: Vec<&str> = self.schema.list_fact_tables();
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
        self.execute_window_query(query_json, query_name, metadata).await
    }

    /// Execute an aggregate query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the aggregate query
    /// * `query_name` - GraphQL field name (e.g., "sales_aggregate")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
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
    pub async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse JSON query into AggregationRequest
        let request = super::super::AggregateQueryParser::parse(query_json, metadata)?;

        // 2. Generate execution plan
        let plan =
            crate::compiler::aggregation::AggregationPlanner::plan(request, metadata.clone())?;

        // 3. Generate SQL
        let sql_generator = super::super::AggregationSqlGenerator::new(self.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL
        let rows = self.adapter.execute_raw_query(&sql.raw_sql).await?;

        // 5. Project results
        let projected = super::super::AggregationProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response = super::super::AggregationProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    /// Execute a window query.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the window query
    /// * `query_name` - GraphQL field name (e.g., "sales_window")
    /// * `metadata` - Fact table metadata
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
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
    pub async fn execute_window_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse JSON query into WindowRequest
        let request = super::super::WindowQueryParser::parse(query_json, metadata)?;

        // 2. Generate execution plan (validates semantic names against metadata)
        let plan =
            crate::compiler::window_functions::WindowPlanner::plan(request, metadata.clone())?;

        // 3. Generate SQL
        let sql_generator = super::super::WindowSqlGenerator::new(self.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL
        let rows = self.adapter.execute_raw_query(&sql.raw_sql).await?;

        // 5. Project results
        let projected = super::super::WindowProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response = super::super::WindowProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }
}
