//! Query executor - main runtime execution engine.

use crate::db::traits::DatabaseAdapter;
use crate::error::{FraiseQLError, Result};
use crate::schema::CompiledSchema;
use super::{QueryMatcher, QueryPlanner, ResultProjector, RuntimeConfig};
use std::sync::Arc;

#[cfg(test)]
use crate::db::types::{DatabaseType, PoolMetrics};

/// Query type classification for routing.
#[derive(Debug, Clone, PartialEq)]
enum QueryType {
    /// Regular GraphQL query (non-analytics).
    Regular,

    /// Aggregate analytics query (ends with _aggregate).
    /// Contains the full query name (e.g., "sales_aggregate").
    Aggregate(String),

    /// Window function query (ends with _window).
    /// Contains the full query name (e.g., "sales_window").
    Window(String),
}

/// Query executor - executes compiled GraphQL queries.
///
/// This is the main entry point for runtime query execution.
/// It coordinates matching, planning, execution, and projection.
pub struct Executor<A: DatabaseAdapter> {
    /// Compiled schema.
    schema: CompiledSchema,

    /// Database adapter.
    adapter: Arc<A>,

    /// Query matcher.
    matcher: QueryMatcher,

    /// Query planner.
    planner: QueryPlanner,

    /// Runtime configuration.
    config: RuntimeConfig,
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Create new executor.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new(schema, Arc::new(adapter));
    /// ```
    #[must_use]
    pub fn new(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config(schema, adapter, RuntimeConfig::default())
    }

    /// Create new executor with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    /// * `config` - Runtime configuration
    #[must_use]
    pub fn with_config(schema: CompiledSchema, adapter: Arc<A>, config: RuntimeConfig) -> Self {
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);

        Self {
            schema,
            adapter,
            matcher,
            planner,
            config,
        }
    }

    /// Execute a GraphQL query.
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Query variables (optional)
    ///
    /// # Returns
    ///
    /// GraphQL response as JSON string
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query is malformed
    /// - Query references undefined operations
    /// - Database execution fails
    /// - Result projection fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query = r#"query { users { id name } }"#;
    /// let result = executor.execute(query, None).await?;
    /// println!("{}", result);
    /// ```
    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Classify query type
        let query_type = self.classify_query(query)?;

        // 2. Route to appropriate handler
        match query_type {
            QueryType::Regular => self.execute_regular_query(query, variables).await,
            QueryType::Aggregate(query_name) => {
                self.execute_aggregate_dispatch(&query_name, variables).await
            }
            QueryType::Window(query_name) => {
                self.execute_window_dispatch(&query_name, variables).await
            }
        }
    }

    /// Execute a regular (non-analytics) GraphQL query.
    async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // 2. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 3. Execute SQL query
        let sql_source = query_match
            .query_def
            .sql_source
            .as_ref()
            .ok_or_else(|| crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path: None,
            })?;

        let results = self
            .adapter
            .execute_where_query(sql_source, None, None, None)
            .await?;

        // 4. Project results
        let projector = ResultProjector::new(plan.projection_fields);
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 5. Wrap in GraphQL data envelope
        let response = ResultProjector::wrap_in_data_envelope(
            projected,
            &query_match.query_def.name,
        );

        // 6. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    /// Classify query type based on operation name.
    fn classify_query(&self, query: &str) -> Result<QueryType> {
        // Extract operation name from GraphQL query
        let operation_name = self.extract_operation_name(query)?;

        // Check if it's an aggregate query (ends with _aggregate)
        if operation_name.ends_with("_aggregate") {
            return Ok(QueryType::Aggregate(operation_name));
        }

        // Check if it's a window query (ends with _window)
        if operation_name.ends_with("_window") {
            return Ok(QueryType::Window(operation_name));
        }

        // Otherwise, it's a regular query
        Ok(QueryType::Regular)
    }

    /// Extract operation name from GraphQL query.
    fn extract_operation_name(&self, query: &str) -> Result<String> {
        let query_trimmed = query.trim();

        // Try to match "query operationName {"
        if let Some(start) = query_trimmed.find("query") {
            let after_query = &query_trimmed[start + 5..].trim_start();
            if let Some(brace_pos) = after_query.find('{') {
                let op_name = after_query[..brace_pos].trim();
                if !op_name.is_empty() {
                    return Ok(op_name.to_string());
                }
            }
        }

        // Try to match "{ operationName"
        if let Some(brace_pos) = query_trimmed.find('{') {
            let after_brace = &query_trimmed[brace_pos + 1..].trim_start();
            if let Some(space_or_brace) = after_brace.find(|c: char| c.is_whitespace() || c == '{' || c == '(') {
                let op_name = after_brace[..space_or_brace].trim();
                if !op_name.is_empty() {
                    return Ok(op_name.to_string());
                }
            }
        }

        Err(FraiseQLError::Parse {
            message: "Could not extract operation name from query".to_string(),
            location: "query".to_string(),
        })
    }

    /// Execute an aggregate query dispatch.
    async fn execute_aggregate_dispatch(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Extract table name from query name (e.g., "sales_aggregate" -> "tf_sales")
        let table_name = query_name
            .strip_suffix("_aggregate")
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!("Invalid aggregate query name: {}", query_name),
                path: None,
            })?;

        let fact_table_name = format!("tf_{}", table_name);

        // Get fact table metadata from schema
        let metadata_json = self
            .schema
            .get_fact_table(&fact_table_name)
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!("Fact table '{}' not found in schema", fact_table_name),
                path: Some(format!("fact_tables.{}", fact_table_name)),
            })?;

        // Parse metadata into FactTableMetadata
        let metadata: crate::compiler::fact_table::FactTableMetadata =
            serde_json::from_value(metadata_json.clone())?;

        // Parse query variables into aggregate query JSON
        let empty_json = serde_json::json!({});
        let query_json = variables.unwrap_or(&empty_json);

        // Execute aggregate query
        self.execute_aggregate_query(query_json, query_name, &metadata)
            .await
    }

    /// Execute a window query dispatch.
    async fn execute_window_dispatch(
        &self,
        _query_name: &str,
        _variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // TODO: Implement window query execution
        // This will be implemented when Phase 7 (Window Functions) is integrated
        Err(FraiseQLError::Validation {
            message: "Window queries not yet implemented".to_string(),
            path: None,
        })
    }

    /// Execute a query and return parsed JSON.
    ///
    /// Same as `execute()` but returns parsed `serde_json::Value` instead of string.
    pub async fn execute_json(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let result_str = self.execute(query, variables).await?;
        Ok(serde_json::from_str(&result_str)?)
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
    /// ```rust,ignore
    /// let query_json = json!({
    ///     "table": "tf_sales",
    ///     "groupBy": { "category": true },
    ///     "aggregates": [{"count": {}}]
    /// });
    ///
    /// let metadata = /* fact table metadata */;
    /// let result = executor.execute_aggregate_query(&query_json, "sales_aggregate", &metadata).await?;
    /// ```
    pub async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse JSON query into AggregationRequest
        let request = super::AggregateQueryParser::parse(query_json, metadata)?;

        // 2. Generate execution plan
        let plan = crate::compiler::aggregation::AggregationPlanner::plan(request, metadata.clone())?;

        // 3. Generate SQL
        let sql_generator = super::AggregationSqlGenerator::new(self.adapter.database_type());
        let sql = sql_generator.generate(&plan)?;

        // 4. Execute SQL
        let rows = self.adapter.execute_raw_query(&sql.complete_sql).await?;

        // 5. Project results
        let projected = super::AggregationProjector::project(rows, &plan)?;

        // 6. Wrap in GraphQL data envelope
        let response = super::AggregationProjector::wrap_in_data_envelope(projected, query_name);

        // 7. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }

    /// Get runtime configuration.
    #[must_use]
    pub const fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get database adapter reference.
    #[must_use]
    pub fn adapter(&self) -> &Arc<A> {
        &self.adapter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::JsonbValue;
    use crate::db::where_clause::WhereClause;
    use crate::schema::{CompiledSchema, QueryDefinition, AutoParams};
    use async_trait::async_trait;

    /// Mock database adapter for testing.
    struct MockAdapter {
        mock_results: Vec<JsonbValue>,
    }

    impl MockAdapter {
        fn new(mock_results: Vec<JsonbValue>) -> Self {
            Self { mock_results }
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MockAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.mock_results.clone())
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections: 1,
                active_connections: 0,
                idle_connections: 1,
                waiting_requests: 0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            // Mock implementation: return empty results
            Ok(vec![])
        }
    }

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params: AutoParams::default(),
        });
        schema
    }

    fn mock_user_results() -> Vec<JsonbValue> {
        vec![
            JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"})),
            JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob"})),
        ]
    }

    #[tokio::test]
    async fn test_executor_new() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        assert_eq!(executor.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.contains("\"data\""));
        assert!(result.contains("\"users\""));
        assert!(result.contains("\"id\""));
        assert!(result.contains("\"name\""));
    }

    #[tokio::test]
    async fn test_execute_json() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute_json(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
    }

    #[tokio::test]
    async fn test_executor_with_config() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let config = RuntimeConfig {
            cache_query_plans: false,
            max_query_depth: 5,
            max_query_complexity: 500,
            enable_tracing: true,
        };

        let executor = Executor::with_config(schema, adapter, config);

        assert!(!executor.config().cache_query_plans);
        assert_eq!(executor.config().max_query_depth, 5);
        assert!(executor.config().enable_tracing);
    }
}
