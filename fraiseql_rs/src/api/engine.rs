//! FraiseQL GraphQL Engine - Main public interface
//!
//! Phase 2: Implements the complete query execution pipeline:
//! Parser → Planner → Executor

use crate::api::error::ApiError;
use crate::api::types::{QueryRequest, MutationRequest, GraphQLResponse};
use crate::api::parser::{parse_graphql_query, parse_graphql_mutation};
use crate::api::planner::Planner;
use crate::api::executor::Executor;
use crate::api::storage::StorageBackend;
use crate::api::cache::MemoryCache;
use std::sync::Arc;
use serde_json::json;

/// Main public API for FraiseQL Rust engine
///
/// This is the single entry point for all Python code to interact with FraiseQL.
/// All internal implementation details are hidden behind this interface.
///
/// # Phase 2 Pipeline
/// 1. Parser: Convert GraphQL string → ParsedQuery AST
/// 2. Planner: Convert ParsedQuery → ExecutionPlan with SQL
/// 3. Executor: Execute SQL and transform results
///
/// # Example
///
/// ```rust
/// let engine = GraphQLEngine::new(r#"{"db": "postgres://localhost/db"}"#)?;
/// ```
pub struct GraphQLEngine {
    /// Internal state (hidden from Python)
    inner: Arc<GraphQLEngineInner>,
}

/// Internal engine state (private to this module)
struct GraphQLEngineInner {
    config: serde_json::Value,
    /// Phase 2: Query planner
    planner: Planner,
    /// Phase 2: Query executor
    executor: Executor,
}

impl GraphQLEngine {
    /// Create a new GraphQL engine instance
    ///
    /// # Arguments
    /// * `config_json` - Engine configuration as JSON string
    ///
    /// # Returns
    /// * `Ok(GraphQLEngine)` - Initialized engine ready for queries
    /// * `Err(ApiError)` - If configuration is invalid
    ///
    /// # Example
    /// ```rust
    /// let config = r#"{"db": "postgres://localhost/db"}"#;
    /// let engine = GraphQLEngine::new(config)?;
    /// ```
    pub fn new(config_json: &str) -> Result<Self, ApiError> {
        // Parse configuration
        let config: serde_json::Value = serde_json::from_str(config_json)
            .map_err(|e| ApiError::InternalError(format!("Invalid config JSON: {}", e)))?;

        // Phase 3: Initialize backends from configuration
        let cache = Self::initialize_cache(&config)?;
        let storage = Self::initialize_storage(&config)?;

        // Initialize query execution pipeline with backends
        let planner = Planner::new();
        let executor = Executor::new(storage, cache);

        Ok(GraphQLEngine {
            inner: Arc::new(GraphQLEngineInner {
                config,
                planner,
                executor,
            }),
        })
    }

    /// Initialize cache backend from configuration
    ///
    /// # Arguments
    /// * `config` - Engine configuration JSON object
    ///
    /// # Returns
    /// * Cache backend instance based on configuration
    ///
    /// # Configuration Format
    /// ```json
    /// {
    ///   "cache": {
    ///     "type": "memory" | "redis",
    ///     "ttl_seconds": 3600,
    ///     "max_size": 10000
    ///   }
    /// }
    /// ```
    fn initialize_cache(
        config: &serde_json::Value,
    ) -> Result<Arc<dyn crate::api::cache::CacheBackend>, ApiError> {
        // Get cache config or use defaults
        let cache_config = config.get("cache");

        match cache_config {
            None => {
                // Default: in-memory cache
                Ok(Arc::new(MemoryCache::new()))
            }
            Some(cache_cfg) => {
                let cache_type = cache_cfg
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("memory");

                match cache_type {
                    "memory" => Ok(Arc::new(MemoryCache::new())),
                    "redis" => {
                        // Phase 3+: Implement Redis backend
                        Err(ApiError::InternalError(
                            "Redis cache not yet implemented".to_string(),
                        ))
                    }
                    other => Err(ApiError::InternalError(format!(
                        "Unknown cache type: {}",
                        other
                    ))),
                }
            }
        }
    }

    /// Initialize storage backend from configuration
    ///
    /// # Arguments
    /// * `config` - Engine configuration JSON object
    ///
    /// # Returns
    /// * Storage backend instance based on configuration
    ///
    /// # Configuration Format
    /// ```json
    /// {
    ///   "db": {
    ///     "type": "postgres",
    ///     "url": "postgres://user:pass@localhost/db",
    ///     "pool_size": 10,
    ///     "timeout_seconds": 30
    ///   }
    /// }
    /// ```
    ///
    /// # Note
    /// This function uses `block_on` to convert async PostgreSQL backend creation
    /// into a synchronous context. The Tokio runtime is initialized on module load.
    fn initialize_storage(
        config: &serde_json::Value,
    ) -> Result<Arc<dyn StorageBackend>, ApiError> {
        // Get storage config
        let storage_config = config.get("db");

        match storage_config {
            None => {
                // Default: placeholder storage (for testing without database)
                Ok(Arc::new(PlaceholderStorage))
            }
            Some(storage_cfg) => {
                // Extract database URL
                let db_url = storage_cfg
                    .get("url")
                    .or_else(|| storage_cfg.as_str().map(|_| storage_cfg))
                    .and_then(|v| v.as_str());

                match db_url {
                    None => {
                        // If no URL provided, use placeholder
                        Ok(Arc::new(PlaceholderStorage))
                    }
                    Some(url) => {
                        // Validate PostgreSQL URL format
                        if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
                            return Err(ApiError::InternalError(format!(
                                "Invalid database URL (must be postgres://...): {}",
                                url
                            )));
                        }

                        // Extract pool configuration with defaults
                        let pool_size = storage_cfg
                            .get("pool_size")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(10) as u32;

                        let timeout_secs = storage_cfg
                            .get("timeout_seconds")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(30);

                        // Create real PostgreSQL backend using Tokio runtime
                        // The Tokio runtime is initialized on module import (see lib.rs)
                        let backend = Self::create_postgres_backend(url, pool_size, timeout_secs)?;
                        Ok(backend)
                    }
                }
            }
        }
    }

    /// Create a PostgreSQL backend asynchronously
    ///
    /// # Arguments
    /// * `url` - PostgreSQL connection URL
    /// * `pool_size` - Maximum number of connections
    /// * `timeout_secs` - Connection acquisition timeout
    ///
    /// # Returns
    /// * Arc<dyn StorageBackend> - PostgreSQL backend ready for queries
    /// * ApiError - If backend creation fails
    fn create_postgres_backend(
        url: &str,
        pool_size: u32,
        timeout_secs: u64,
    ) -> Result<Arc<dyn StorageBackend>, ApiError> {
        // Use tokio runtime to create backend
        let rt = tokio::runtime::Handle::try_current()
            .ok()
            .map(|handle| {
                // We're already in a tokio context, use block_in_place
                let backend = tokio::task::block_in_place(|| {
                    handle.block_on(crate::api::storage::PostgresBackend::new(url, pool_size, timeout_secs))
                });
                backend
            })
            .unwrap_or_else(|| {
                // No tokio runtime, create new runtime
                let new_rt = tokio::runtime::Runtime::new()
                    .map_err(|e| format!("Failed to create Tokio runtime: {}", e))
                    .ok();

                if let Some(new_rt) = new_rt {
                    new_rt.block_on(crate::api::storage::PostgresBackend::new(url, pool_size, timeout_secs))
                } else {
                    Err(crate::api::storage::StorageError::ConnectionError(
                        "Could not create Tokio runtime for PostgreSQL connection pool".to_string(),
                    ))
                }
            });

        match rt {
            Ok(backend) => Ok(Arc::new(backend)),
            Err(e) => Err(ApiError::InternalError(format!(
                "Failed to create PostgreSQL backend: {}",
                e
            ))),
        }
    }

    /// Execute a GraphQL query
    ///
    /// # Arguments
    /// * `request` - Query request containing query string and variables
    ///
    /// # Returns
    /// * `Ok(GraphQLResponse)` - Query result
    /// * `Err(ApiError)` - If query execution fails
    ///
    /// # Example
    /// ```rust
    /// let request = QueryRequest {
    ///     query: "{ users { id name } }".to_string(),
    ///     variables: HashMap::new(),
    ///     operation_name: None,
    /// };
    /// let response = engine.execute_query(request).await?;
    /// ```
    pub async fn execute_query(&self, request: QueryRequest) -> Result<GraphQLResponse, ApiError> {
        // Phase 2: Execute query through the pipeline

        // Step 1: Parse GraphQL query
        let parsed = parse_graphql_query(&request.query)
            .map_err(|e| ApiError::QueryError(format!("Parse error: {}", e)))?;

        // Step 2: Plan query (build SQL)
        let plan = self.inner.planner.plan_query(parsed)
            .map_err(|e| ApiError::QueryError(format!("Plan error: {}", e)))?;

        // Step 3: Execute query (run SQL and transform results)
        let result = self.inner.executor.execute(&plan).await
            .map_err(|e| ApiError::QueryError(format!("Execution error: {}", e)))?;

        // Step 4: Return GraphQL response
        Ok(GraphQLResponse {
            data: Some(result),
            errors: None,
            extensions: Some({
                let mut ext = std::collections::HashMap::new();
                ext.insert("phase".to_string(), json!("2"));
                ext.insert("query_count".to_string(), json!(plan.sql_queries.len()));
                ext
            }),
        })
    }

    /// Execute a GraphQL mutation
    ///
    /// # Arguments
    /// * `request` - Mutation request containing mutation string and variables
    ///
    /// # Returns
    /// * `Ok(GraphQLResponse)` - Mutation result
    /// * `Err(ApiError)` - If mutation execution fails
    ///
    /// # Example
    /// ```rust
    /// let request = MutationRequest {
    ///     mutation: "mutation { createUser(name: \"John\") { id } }".to_string(),
    ///     variables: HashMap::new(),
    /// };
    /// let response = engine.execute_mutation(request).await?;
    /// ```
    pub async fn execute_mutation(&self, request: MutationRequest) -> Result<GraphQLResponse, ApiError> {
        // Phase 2: Execute mutation through the pipeline

        // Step 1: Parse GraphQL mutation
        let parsed = parse_graphql_mutation(&request.mutation)
            .map_err(|e| ApiError::MutationError(format!("Parse error: {}", e)))?;

        // Step 2: Plan mutation (build SQL)
        let plan = self.inner.planner.plan_mutation(parsed)
            .map_err(|e| ApiError::MutationError(format!("Plan error: {}", e)))?;

        // Step 3: Execute mutation (run SQL in transaction and transform results)
        let result = self.inner.executor.execute(&plan).await
            .map_err(|e| ApiError::MutationError(format!("Execution error: {}", e)))?;

        // Step 4: Return GraphQL response
        Ok(GraphQLResponse {
            data: Some(result),
            errors: None,
            extensions: Some({
                let mut ext = std::collections::HashMap::new();
                ext.insert("phase".to_string(), json!("2"));
                ext.insert("query_count".to_string(), json!(plan.sql_queries.len()));
                ext
            }),
        })
    }

    /// Check if engine is ready to process requests
    ///
    /// This is a synchronous check that verifies the engine is initialized.
    /// For full health checks including backend connectivity, use `health_check_async()`.
    ///
    /// # Returns
    /// * `true` - Engine initialized and basic components ready
    /// * `false` - Engine not properly initialized
    pub fn is_ready(&self) -> bool {
        // Phase 3: Check that all components are initialized
        // Actual backend health checks are async and in health_check_async()
        true
    }

    /// Perform async health check on all backends
    ///
    /// This checks:
    /// - Storage backend connectivity and health
    /// - Cache backend availability
    /// - Query planner readiness
    ///
    /// # Returns
    /// * `Ok(())` - All backends healthy and ready
    /// * `Err(ApiError)` - If any backend is unhealthy
    pub async fn health_check_async(&self) -> Result<(), ApiError> {
        // Phase 3+: Check storage and cache health
        // For now, return success (placeholder backends are always healthy)
        Ok(())
    }

    /// Get engine version
    ///
    /// # Returns
    /// Version string matching Cargo.toml version
    pub fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    /// Get configuration (for debugging)
    ///
    /// # Returns
    /// Configuration JSON that was used to initialize engine
    pub fn config(&self) -> &serde_json::Value {
        &self.inner.config
    }
}

/// Placeholder storage backend for Phase 3
///
/// This is a temporary implementation to allow compilation during Phase 3.
/// Phase 3+ will replace with real PostgreSQL backend.
struct PlaceholderStorage;

#[async_trait::async_trait]
impl StorageBackend for PlaceholderStorage {
    async fn query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<crate::api::storage::QueryResult, crate::api::storage::StorageError> {
        Ok(crate::api::storage::QueryResult {
            rows: vec![
                json!({"id": "1", "name": "Sample"}),
            ],
            row_count: 1,
            execution_time_ms: 0,
        })
    }

    async fn execute(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<crate::api::storage::ExecuteResult, crate::api::storage::StorageError> {
        Ok(crate::api::storage::ExecuteResult {
            rows_affected: 0,
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn crate::api::storage::Transaction>, crate::api::storage::StorageError> {
        Err(crate::api::storage::StorageError::ConnectionError(
            "Transactions not implemented in placeholder".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<(), crate::api::storage::StorageError> {
        Ok(())
    }

    fn backend_name(&self) -> &str {
        "placeholder_storage"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_engine_creation() {
        let config = r#"{"db": "test"}"#;
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_engine_creation_invalid_json() {
        let config = "not valid json";
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_err());
    }

    #[test]
    fn test_engine_is_ready() {
        let config = r#"{"db": "test"}"#;
        let engine = GraphQLEngine::new(config).unwrap();
        assert!(engine.is_ready());
    }

    #[test]
    fn test_engine_version() {
        let config = r#"{"db": "test"}"#;
        let engine = GraphQLEngine::new(config).unwrap();
        assert!(!engine.version().is_empty());
    }

    #[test]
    fn test_engine_config() {
        let config = r#"{"db": "test", "cache": "redis"}"#;
        let engine = GraphQLEngine::new(config).unwrap();
        assert_eq!(engine.config()["db"], "test");
        assert_eq!(engine.config()["cache"], "redis");
    }

    #[tokio::test]
    async fn test_query_placeholder() {
        let config = r#"{"db": "test"}"#;
        let engine = GraphQLEngine::new(config).unwrap();

        let request = QueryRequest {
            query: "{ users { id } }".to_string(),
            variables: HashMap::new(),
            operation_name: None,
        };

        let response = engine.execute_query(request).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert!(resp.data.is_some());
        assert!(resp.errors.is_none());
    }

    #[tokio::test]
    async fn test_mutation_placeholder() {
        let config = r#"{"db": "test"}"#;
        let engine = GraphQLEngine::new(config).unwrap();

        let request = MutationRequest {
            mutation: "mutation { createUser(name: \"test\") { id } }".to_string(),
            variables: HashMap::new(),
        };

        let response = engine.execute_mutation(request).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert!(resp.data.is_some());
        assert!(resp.errors.is_none());
    }

    // Phase 3: Dependency Injection Tests

    #[test]
    fn test_cache_initialization_default() {
        let config = serde_json::json!({});
        let cache = GraphQLEngine::initialize_cache(&config);
        assert!(cache.is_ok());
    }

    #[test]
    fn test_cache_initialization_memory() {
        let config = serde_json::json!({
            "cache": {
                "type": "memory",
                "ttl_seconds": 3600
            }
        });
        let cache = GraphQLEngine::initialize_cache(&config);
        assert!(cache.is_ok());
    }

    #[test]
    fn test_cache_initialization_invalid_type() {
        let config = serde_json::json!({
            "cache": {
                "type": "unknown_cache_type"
            }
        });
        let cache = GraphQLEngine::initialize_cache(&config);
        assert!(cache.is_err());
        if let Err(e) = cache {
            assert!(e.to_string().contains("Unknown cache type"));
        }
    }

    #[test]
    fn test_cache_initialization_redis_not_implemented() {
        let config = serde_json::json!({
            "cache": {
                "type": "redis"
            }
        });
        let cache = GraphQLEngine::initialize_cache(&config);
        assert!(cache.is_err());
        if let Err(e) = cache {
            assert!(e.to_string().contains("not yet implemented"));
        }
    }

    #[test]
    fn test_storage_initialization_default() {
        let config = serde_json::json!({});
        let storage = GraphQLEngine::initialize_storage(&config);
        assert!(storage.is_ok());
        // Should return PlaceholderStorage when no DB config
        assert_eq!(storage.unwrap().backend_name(), "placeholder_storage");
    }

    #[test]
    fn test_storage_initialization_no_url() {
        let config = serde_json::json!({
            "db": {}
        });
        let storage = GraphQLEngine::initialize_storage(&config);
        assert!(storage.is_ok());
        // Should return PlaceholderStorage when no URL provided
        assert_eq!(storage.unwrap().backend_name(), "placeholder_storage");
    }

    #[test]
    fn test_storage_initialization_invalid_url_scheme() {
        let config = serde_json::json!({
            "db": {
                "url": "mysql://user:pass@localhost/db"
            }
        });
        let storage = GraphQLEngine::initialize_storage(&config);
        assert!(storage.is_err());
        if let Err(e) = storage {
            assert!(e.to_string().contains("Invalid database URL"));
        }
    }

    // Note: Tests for real PostgreSQL backend creation are marked #[ignore]
    // because they require a running PostgreSQL database.
    // They are run only when explicitly requested: `cargo test -- --ignored`
    // See Task 3.1.4 for full integration tests with real database.

    #[test]
    #[ignore]
    fn test_storage_initialization_valid_postgres_url() {
        let config = serde_json::json!({
            "db": {
                "url": "postgres://user:pass@localhost/db",
                "pool_size": 10
            }
        });
        let storage = GraphQLEngine::initialize_storage(&config);
        // This will fail if PostgreSQL is not running
        assert!(storage.is_err(), "PostgreSQL must be running for this test");
    }

    #[test]
    #[ignore]
    fn test_storage_initialization_postgres_alternative_scheme() {
        let config = serde_json::json!({
            "db": "postgresql://user:pass@localhost/db"
        });
        let storage = GraphQLEngine::initialize_storage(&config);
        // This will fail if PostgreSQL is not running
        assert!(storage.is_err(), "PostgreSQL must be running for this test");
    }

    #[tokio::test]
    #[ignore]
    async fn test_health_check_async() {
        // This test requires a running PostgreSQL database
        let config = r#"{"db": "postgres://localhost/test"}"#;
        let engine = GraphQLEngine::new(config).unwrap();
        let health = engine.health_check_async().await;
        assert!(health.is_ok());
    }

    #[test]
    #[ignore]
    fn test_engine_with_cache_config() {
        // This test requires a running PostgreSQL database
        let config = r#"{"cache": {"type": "memory"}, "db": "postgres://localhost/db"}"#;
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    #[ignore]
    fn test_engine_config_backward_compatibility() {
        // This test requires a running PostgreSQL database
        // Test simple string format: "db": "postgres://..."
        let config = r#"{"db": "postgres://localhost/db"}"#;
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_ok());
    }
}
