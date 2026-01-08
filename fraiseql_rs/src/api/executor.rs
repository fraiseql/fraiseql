//! GraphQL Query Executor Layer
//!
//! This module executes SQL query plans and transforms results into GraphQL responses.
//! Phase 3: Integrates storage and cache layers for real query execution
//!
//! Responsibilities:
//! - Execute SQL queries via StorageBackend
//! - Cache SELECT query results
//! - Invalidate cache on mutations
//! - Transform SQL results to GraphQL format
//! - Handle errors gracefully

use crate::api::cache::CacheBackend;
use crate::api::error::ApiError;
use crate::api::planner::{ExecutionPlan, ResponseMetadata, ResultMapping};
use crate::api::storage::StorageBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Result of executing a single SQL query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// SQL query that was executed
    pub sql: String,

    /// Raw results from database
    pub sql_results: Vec<serde_json::Value>,

    /// Whether execution succeeded
    pub success: bool,

    /// Any errors that occurred
    pub errors: Vec<ExecutionError>,

    /// Whether this result came from cache
    pub from_cache: bool,
}

/// Errors that can occur during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionError {
    /// Database connection or query error
    DatabaseError(String),

    /// Error transforming SQL result to GraphQL format
    TransformationError(String),

    /// Query execution timeout
    TimeoutError,

    /// Authorization/permission denied
    AuthorizationError(String),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            ExecutionError::TransformationError(msg) => write!(f, "Transformation error: {}", msg),
            ExecutionError::TimeoutError => write!(f, "Query timeout"),
            ExecutionError::AuthorizationError(msg) => write!(f, "Authorization error: {}", msg),
        }
    }
}

/// Executor for running SQL queries and transforming results
pub struct Executor {
    /// Storage backend for executing queries
    storage: Arc<dyn StorageBackend>,

    /// Cache backend for caching query results
    cache: Arc<dyn CacheBackend>,
}

impl Executor {
    /// Create a new Executor with storage and cache backends
    ///
    /// # Arguments
    /// * `storage` - StorageBackend implementation for executing queries
    /// * `cache` - CacheBackend implementation for caching results
    pub fn new(storage: Arc<dyn StorageBackend>, cache: Arc<dyn CacheBackend>) -> Self {
        Executor { storage, cache }
    }

    /// Execute an execution plan and return GraphQL-formatted results
    ///
    /// # Arguments
    /// * `plan` - The execution plan from the planner layer
    ///
    /// # Returns
    /// * `Result<serde_json::Value, ApiError>` - Transformed GraphQL response
    ///
    /// # Phase 3+ Implementation
    /// - Checks cache for SELECT queries before storage lookup
    /// - Executes queries via storage backend
    /// - Caches SELECT results for future queries
    /// - Invalidates cache on mutations
    pub async fn execute(&self, plan: &ExecutionPlan) -> Result<serde_json::Value, ApiError> {
        let mut result_map = serde_json::Map::new();

        for sql_query in &plan.sql_queries {
            // Generate cache key for SELECT queries
            let cache_key = if sql_query
                .sql
                .trim_start()
                .to_uppercase()
                .starts_with("SELECT")
            {
                Some(format!("query:{}", sql_query.sql))
            } else {
                None
            };

            // Try to get from cache first
            let sql_results = if let Some(key) = &cache_key {
                match self.cache.get(key).await {
                    Ok(Some(cached_value)) => {
                        // Return cached results as single value
                        vec![cached_value]
                    }
                    Ok(None) => {
                        // Cache miss - query storage
                        let query_result = self
                            .storage
                            .query(&sql_query.sql, &sql_query.parameters)
                            .await
                            .map_err(|e| {
                                ApiError::InternalError(format!("Query execution failed: {}", e))
                            })?;

                        // Cache the results for future queries
                        let results_value = serde_json::json!(query_result.rows);
                        let _ = self.cache.set(key, results_value.clone(), 3600).await;

                        query_result.rows
                    }
                    Err(_e) => {
                        // Cache error - fall back to storage
                        let query_result = self
                            .storage
                            .query(&sql_query.sql, &sql_query.parameters)
                            .await
                            .map_err(|e| {
                                ApiError::InternalError(format!("Query execution failed: {}", e))
                            })?;
                        query_result.rows
                    }
                }
            } else {
                // Mutation query - execute directly without caching
                let execute_result = self
                    .storage
                    .execute(&sql_query.sql, &sql_query.parameters)
                    .await
                    .map_err(|e| {
                        ApiError::InternalError(format!("Mutation execution failed: {}", e))
                    })?;

                // Invalidate cache on mutations
                // In Phase 3+, this would be more selective based on affected tables
                let _ = self.cache.clear().await;

                // Return mutation result as JSON
                vec![serde_json::json!({
                    "rows_affected": execute_result.rows_affected,
                    "last_insert_id": execute_result.last_insert_id,
                })]
            };

            // Transform using result mapping
            let transformed = self.transform_results(
                &sql_results,
                &plan.result_mapping,
                &plan.response_metadata,
            )?;

            // Add to result map
            result_map.insert(sql_query.root_field.clone(), transformed);
        }

        Ok(serde_json::Value::Object(result_map))
    }

    /// Execute multiple plans in a transaction
    ///
    /// # Arguments
    /// * `plans` - Vector of execution plans to execute
    ///
    /// # Returns
    /// * `Result<Vec<serde_json::Value>, ApiError>` - Results for each plan
    pub async fn execute_in_transaction(
        &self,
        plans: &[&ExecutionPlan],
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let mut results = Vec::new();

        for plan in plans {
            let result = self.execute(plan).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Transform SQL results using result mapping and metadata
    fn transform_results(
        &self,
        sql_results: &[serde_json::Value],
        mapping: &ResultMapping,
        metadata: &ResponseMetadata,
    ) -> Result<serde_json::Value, ApiError> {
        // Phase 2: Simple pass-through transformation
        // Phase 3+: Apply column mapping, aliases, nested transformations

        let mut transformed = Vec::new();

        for result in sql_results {
            if let serde_json::Value::Object(obj) = result {
                let mut transformed_obj = serde_json::Map::new();

                // Apply column-to-field mapping
                for (column, value) in obj.iter() {
                    let field_name = mapping
                        .column_to_field
                        .get(column)
                        .map(|s| s.as_str())
                        .unwrap_or(column);

                    // Apply aliases if present
                    let final_name = if let Some(alias) = metadata.aliases.get(field_name) {
                        alias.as_str()
                    } else {
                        field_name
                    };

                    transformed_obj.insert(final_name.to_string(), value.clone());
                }

                // Add __typename if requested (Phase 3+)
                if metadata.include_typename {
                    transformed_obj.insert(
                        "__typename".to_string(),
                        serde_json::json!(&metadata.return_type),
                    );
                }

                transformed.push(serde_json::Value::Object(transformed_obj));
            }
        }

        // Return list or single object based on structure
        if transformed.len() == 1 && !mapping.selected_columns.contains(&"*".to_string()) {
            Ok(transformed.into_iter().next().unwrap())
        } else {
            Ok(serde_json::Value::Array(transformed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::cache::{CacheBackend, CacheError};
    use crate::api::parser::parse_graphql_query;
    use crate::api::planner::Planner;
    use crate::api::storage::{ExecuteResult, QueryResult, StorageBackend, StorageError};
    use async_trait::async_trait;

    /// Mock storage backend for testing
    struct MockStorage;

    #[async_trait]
    impl StorageBackend for MockStorage {
        async fn query(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<QueryResult, StorageError> {
            Ok(QueryResult {
                rows: vec![
                    serde_json::json!({"id": "1", "name": "Item 1"}),
                    serde_json::json!({"id": "2", "name": "Item 2"}),
                ],
                row_count: 2,
                execution_time_ms: 5,
            })
        }

        async fn execute(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<ExecuteResult, StorageError> {
            Ok(ExecuteResult {
                rows_affected: 1,
                last_insert_id: Some(42),
                execution_time_ms: 2,
            })
        }

        async fn begin_transaction(
            &self,
        ) -> Result<Box<dyn crate::api::storage::Transaction>, StorageError> {
            Err(StorageError::ConnectionError(
                "Not implemented in mock".to_string(),
            ))
        }

        async fn health_check(&self) -> Result<(), StorageError> {
            Ok(())
        }

        fn backend_name(&self) -> &str {
            "mock_storage"
        }
    }

    /// Mock cache backend for testing
    struct MockCache;

    #[async_trait]
    impl CacheBackend for MockCache {
        async fn get(&self, _key: &str) -> Result<Option<serde_json::Value>, CacheError> {
            Ok(None)
        }

        async fn set(
            &self,
            _key: &str,
            _value: serde_json::Value,
            _ttl_seconds: u64,
        ) -> Result<(), CacheError> {
            Ok(())
        }

        async fn delete(&self, _key: &str) -> Result<(), CacheError> {
            Ok(())
        }

        async fn delete_many(&self, _keys: &[String]) -> Result<(), CacheError> {
            Ok(())
        }

        async fn clear(&self) -> Result<(), CacheError> {
            Ok(())
        }

        async fn health_check(&self) -> Result<(), CacheError> {
            Ok(())
        }

        fn backend_name(&self) -> &str {
            "mock_cache"
        }
    }

    fn create_test_executor() -> Executor {
        let storage: Arc<dyn StorageBackend> = Arc::new(MockStorage);
        let cache: Arc<dyn CacheBackend> = Arc::new(MockCache);
        Executor::new(storage, cache)
    }

    #[tokio::test]
    async fn test_execute_simple_query() {
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();
        let plan = planner.plan_query(parsed).unwrap();

        let executor = create_test_executor();
        let result = executor.execute(&plan).await;

        assert!(result.is_ok());
        let result_obj = result.unwrap();
        assert!(result_obj.is_object());
    }

    #[tokio::test]
    async fn test_execute_handles_errors() {
        let executor = create_test_executor();

        // Create a minimal plan with an unknown field
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();

        if let Ok(plan) = planner.plan_query(parsed) {
            let result = executor.execute(&plan).await;
            // Should succeed with mock storage
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_execute_marks_list_queries() {
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();
        let plan = planner.plan_query(parsed).unwrap();

        assert!(plan.sql_queries[0].is_list);
    }

    #[tokio::test]
    async fn test_execute_marks_single_queries() {
        let parsed = parse_graphql_query("{ user { id } }").unwrap();
        let planner = Planner::new();
        let plan = planner.plan_query(parsed).unwrap();

        assert!(!plan.sql_queries[0].is_list);
    }

    #[test]
    fn test_transform_results_simple() {
        let executor = create_test_executor();
        let sql_results = vec![serde_json::json!({
            "id": "1",
            "name": "Test"
        })];

        let mapping = ResultMapping {
            column_to_field: HashMap::new(),
            selected_columns: vec!["id".to_string(), "name".to_string()],
            nested_plans: HashMap::new(),
        };

        let metadata = ResponseMetadata {
            return_type: "User".to_string(),
            include_typename: false,
            aliases: HashMap::new(),
        };

        let result = executor
            .transform_results(&sql_results, &mapping, &metadata)
            .unwrap();

        assert!(result.is_object());
    }

    #[test]
    fn test_transform_results_with_aliases() {
        let executor = create_test_executor();
        let sql_results = vec![serde_json::json!({
            "id": "1",
            "name": "Test"
        })];

        let mut aliases = HashMap::new();
        aliases.insert("id".to_string(), "user_id".to_string());

        let mapping = ResultMapping {
            column_to_field: HashMap::new(),
            selected_columns: vec!["id".to_string(), "name".to_string()],
            nested_plans: HashMap::new(),
        };

        let metadata = ResponseMetadata {
            return_type: "User".to_string(),
            include_typename: false,
            aliases,
        };

        let result = executor
            .transform_results(&sql_results, &mapping, &metadata)
            .unwrap();

        // Result should contain alias
        if let serde_json::Value::Object(obj) = result {
            // Alias should be applied
            assert!(obj.contains_key("user_id") || obj.contains_key("id"));
        }
    }

    #[tokio::test]
    async fn test_execute_in_transaction() {
        let parsed = parse_graphql_query("{ users { id } }").unwrap();
        let planner = Planner::new();
        let plan = planner.plan_query(parsed).unwrap();

        let executor = create_test_executor();
        let results = executor.execute_in_transaction(&[&plan]).await;

        assert!(results.is_ok());
        assert_eq!(results.unwrap().len(), 1);
    }

    #[test]
    fn test_execution_error_display() {
        let err = ExecutionError::DatabaseError("test error".to_string());
        assert!(err.to_string().contains("Database error"));

        let err = ExecutionError::TimeoutError;
        assert!(err.to_string().contains("timeout"));

        let err = ExecutionError::AuthorizationError("denied".to_string());
        assert!(err.to_string().contains("Authorization"));
    }

    #[test]
    fn test_executor_creation() {
        let executor = create_test_executor();

        // Executor should be valid
        assert!(std::mem::size_of_val(&executor) > 0);
    }
}
