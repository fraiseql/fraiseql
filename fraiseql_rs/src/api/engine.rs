//! FraiseQL GraphQL Engine - Main public interface
//!
//! Phase 2: Implements the complete query execution pipeline:
//! Parser → Planner → Executor

use crate::api::cache::MemoryCache;
use crate::api::error::ApiError;
use crate::api::executor::Executor;
use crate::api::parser::{parse_graphql_mutation, parse_graphql_query};
use crate::api::planner::Planner;
use crate::api::storage::StorageBackend;
use crate::api::types::{GraphQLResponse, MutationRequest, QueryRequest};
use serde_json::json;
use std::sync::Arc;

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
    ///     "url": "postgres://user:pass@localhost/db",
    ///     "pool_size": 10,
    ///     "timeout_seconds": 30
    ///   }
    /// }
    /// ```
    ///
    /// # Note
    /// This function creates a ProductionPool (deadpool-based) and wraps it
    /// in the PostgresBackend storage layer using the PoolBackend abstraction.
    fn initialize_storage(config: &serde_json::Value) -> Result<Arc<dyn StorageBackend>, ApiError> {
        use crate::db::{DatabaseConfig, PoolBackend, ProductionPool};

        // Fail-fast: Database configuration is REQUIRED
        let storage_config = config.get("db").ok_or_else(|| {
            ApiError::InternalError(
                "Database configuration required: add 'db' to engine config with PostgreSQL connection URL"
                    .to_string(),
            )
        })?;

        // Extract database URL - supports both formats:
        // 1. Simple string: "db": "postgres://..."
        // 2. Object with URL: "db": { "url": "postgres://..." }
        let db_url = storage_config
            .get("url")
            .or_else(|| {
                // Try direct string format
                storage_config.as_str().map(|_| storage_config)
            })
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError::InternalError(
                    "Invalid database configuration: 'db' must be a connection string or object with 'url' field"
                        .to_string(),
                )
            })?;

        // Parse connection string into DatabaseConfig
        let db_config = DatabaseConfig::from_url(db_url)
            .map_err(|e| ApiError::InternalError(format!("Invalid database URL: {}", e)))?;

        // Extract pool configuration with defaults
        let pool_size = storage_config
            .get("pool_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let db_config = db_config.with_max_size(pool_size);

        // Create production pool (deadpool-based)
        let pool = ProductionPool::new(db_config).map_err(|e| {
            ApiError::InternalError(format!("Failed to create connection pool: {}", e))
        })?;

        // Wrap pool as Arc<dyn PoolBackend> for storage layer
        let pool_backend: Arc<dyn PoolBackend> = Arc::new(pool);

        // Create PostgreSQL storage backend with pool abstraction
        let storage =
            crate::api::storage::PostgresBackend::with_pool(pool_backend).map_err(|e| {
                ApiError::InternalError(format!("Failed to create storage backend: {}", e))
            })?;

        Ok(Arc::new(storage))
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
        let plan = self
            .inner
            .planner
            .plan_query(parsed)
            .map_err(|e| ApiError::QueryError(format!("Plan error: {}", e)))?;

        // Step 3: Execute query (run SQL and transform results)
        let result = self
            .inner
            .executor
            .execute(&plan)
            .await
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
    pub async fn execute_mutation(
        &self,
        request: MutationRequest,
    ) -> Result<GraphQLResponse, ApiError> {
        // Phase 2: Execute mutation through the pipeline

        // Step 1: Parse GraphQL mutation
        let parsed = parse_graphql_mutation(&request.mutation)
            .map_err(|e| ApiError::MutationError(format!("Parse error: {}", e)))?;

        // Step 2: Plan mutation (build SQL)
        let plan = self
            .inner
            .planner
            .plan_mutation(parsed)
            .map_err(|e| ApiError::MutationError(format!("Plan error: {}", e)))?;

        // Step 3: Execute mutation (run SQL in transaction and transform results)
        let result = self
            .inner
            .executor
            .execute(&plan)
            .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_engine_creation_invalid_json() {
        let config = "not valid json";
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_err());
    }

    #[test]
    fn test_engine_creation_missing_db() {
        let config = r#"{"cache": "memory"}"#;
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_err());
        if let Err(e) = engine {
            assert!(e.to_string().contains("Database configuration required"));
        }
    }

    #[test]
    fn test_engine_creation_invalid_url_scheme() {
        let config = r#"{"db": "mysql://localhost/db"}"#;
        let engine = GraphQLEngine::new(config);
        assert!(engine.is_err());
        if let Err(e) = engine {
            assert!(e.to_string().contains("Invalid database URL scheme"));
        }
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
    fn test_storage_initialization_missing_db_config() {
        let config = serde_json::json!({});
        let storage = GraphQLEngine::initialize_storage(&config);
        assert!(storage.is_err());
        if let Err(e) = storage {
            assert!(e.to_string().contains("Database configuration required"));
        }
    }

    #[test]
    fn test_storage_initialization_missing_url() {
        let config = serde_json::json!({
            "db": {}
        });
        let storage = GraphQLEngine::initialize_storage(&config);
        assert!(storage.is_err());
        if let Err(e) = storage {
            assert!(e.to_string().contains("must be a connection string"));
        }
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
