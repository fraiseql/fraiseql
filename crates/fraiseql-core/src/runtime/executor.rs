//! Query executor - main runtime execution engine.

use crate::db::traits::DatabaseAdapter;
use crate::db::types::{DatabaseType, PoolMetrics};
use crate::error::Result;
use crate::schema::CompiledSchema;
use super::{QueryMatcher, QueryPlanner, ResultProjector, RuntimeConfig};
use std::sync::Arc;

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
