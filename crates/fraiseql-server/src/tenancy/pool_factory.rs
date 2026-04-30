//! Tenant pool creation and executor construction.
//!
//! Provides [`TenantPoolConfig`] and [`create_tenant_executor`] to build a
//! fully-formed `Executor<A>` from a compiled schema JSON string and database
//! connection configuration. Used by the management API (Phase 03) to register
//! tenants at runtime.

use std::sync::Arc;

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::{CompiledSchema, TenancyMode},
};
use fraiseql_error::{FraiseQLError, Result};
use serde::Deserialize;
use tracing::info;

use super::schema_isolation;

/// Connection configuration for a tenant database pool.
#[derive(Debug, Clone, Deserialize)]
pub struct TenantPoolConfig {
    /// Database connection string (e.g. `postgres://user:pass@host:5432/db`).
    pub connection_string:    String,
    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections:      u32,
    /// Connection timeout in seconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Idle connection timeout in seconds.
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs:    u64,
}

const fn default_max_connections() -> u32 {
    10
}
const fn default_connect_timeout() -> u64 {
    5
}
const fn default_idle_timeout() -> u64 {
    300
}

/// Trait for database adapters that can be created from a connection string.
///
/// Implemented by adapters that support dynamic pool creation at runtime
/// (as opposed to static initialization at server startup).
#[async_trait::async_trait]
pub trait FromPoolConfig: DatabaseAdapter + Sized {
    /// Create a new adapter from connection configuration.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` or `FraiseQLError::Database`
    /// if the connection cannot be established.
    async fn from_pool_config(config: &TenantPoolConfig) -> Result<Self>;
}

/// Creates a complete tenant executor from a compiled schema JSON string and
/// connection configuration.
///
/// This is the primary entry point for tenant registration: it parses the schema,
/// validates its format version, creates a database pool, and assembles an
/// `Executor<A>` with both baked in.
///
/// When the compiled schema specifies `tenancy.mode = "schema"`, this function
/// also provisions the tenant's PostgreSQL schema (`CREATE SCHEMA IF NOT EXISTS
/// tenant_{key}`) and configures the adapter's search path.
///
/// # Arguments
///
/// * `tenant_key` - The tenant identifier used for schema naming
/// * `schema_json` - Compiled schema JSON string
/// * `pool_config` - Database connection configuration
///
/// # Errors
///
/// Returns `FraiseQLError::Parse` if the schema JSON is invalid.
/// Returns `FraiseQLError::Validation` if the schema format version is unsupported
/// or the tenant key would produce an invalid PostgreSQL schema name.
/// Returns `FraiseQLError::ConnectionPool` / `FraiseQLError::Database` if the pool
/// cannot be created or schema DDL fails.
pub async fn create_tenant_executor<A: FromPoolConfig>(
    tenant_key: &str,
    schema_json: &str,
    pool_config: &TenantPoolConfig,
) -> Result<Arc<Executor<A>>> {
    // 1. Parse and validate schema
    let schema = CompiledSchema::from_json(schema_json).map_err(|e| FraiseQLError::Parse {
        message:  format!("Invalid compiled schema JSON: {e}"),
        location: String::new(),
    })?;

    schema
        .validate_format_version()
        .map_err(|msg| FraiseQLError::validation(format!("Incompatible compiled schema: {msg}")))?;

    let tenancy_mode = schema.tenancy_mode();

    // 2. Create database adapter/pool
    let adapter = A::from_pool_config(pool_config).await?;

    // 3. Schema isolation: provision schema + configure search_path
    if tenancy_mode == TenancyMode::Schema {
        info!(tenant_key, "provisioning schema for tenant (schema isolation mode)");
        schema_isolation::provision_tenant_schema(tenant_key, &adapter).await?;
        schema_isolation::configure_search_path(tenant_key, &adapter).await?;
    }

    // 4. Assemble executor
    Ok(Arc::new(Executor::new(schema, Arc::new(adapter))))
}

/// Drop a tenant's PostgreSQL schema if schema isolation mode is active.
///
/// Executes `DROP SCHEMA IF EXISTS tenant_{key} CASCADE` against the provided
/// adapter. This is a no-op if the tenant key does not correspond to an existing
/// schema. Called from the delete tenant handler when `tenancy.mode = "schema"`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the tenant key is invalid.
/// Returns `FraiseQLError::Database` if the DDL execution fails.
pub async fn destroy_tenant_schema(
    tenant_key: &str,
    adapter: &dyn DatabaseAdapter,
) -> Result<()> {
    schema_isolation::drop_tenant_schema(tenant_key, adapter).await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        schema::CompiledSchema,
    };

    use super::*;

    /// Stub adapter that implements `FromPoolConfig` for testing.
    #[derive(Debug, Clone)]
    struct StubPoolAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubPoolAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl FromPoolConfig for StubPoolAdapter {
        async fn from_pool_config(_config: &TenantPoolConfig) -> FraiseQLResult<Self> {
            Ok(Self)
        }
    }

    fn test_pool_config() -> TenantPoolConfig {
        TenantPoolConfig {
            connection_string:    "stub://localhost/test".to_string(),
            max_connections:      5,
            connect_timeout_secs: 5,
            idle_timeout_secs:    300,
        }
    }

    #[tokio::test]
    async fn test_create_tenant_executor_success() {
        let schema = CompiledSchema::default();
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let executor =
            create_tenant_executor::<StubPoolAdapter>("acme", &schema_json, &config)
                .await
                .unwrap();
        assert_eq!(executor.schema().types.len(), 0);
    }

    #[tokio::test]
    async fn test_create_tenant_executor_invalid_json() {
        let config = test_pool_config();
        let Err(err) =
            create_tenant_executor::<StubPoolAdapter>("acme", "not valid json", &config).await
        else {
            panic!("expected Err for invalid JSON");
        };
        assert!(matches!(err, FraiseQLError::Parse { .. }), "Expected Parse error, got: {err:?}");
    }

    #[tokio::test]
    async fn test_create_tenant_executor_bad_format_version() {
        let schema = CompiledSchema {
            schema_format_version: Some(999),
            ..CompiledSchema::default()
        };
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let Err(err) =
            create_tenant_executor::<StubPoolAdapter>("acme", &schema_json, &config).await
        else {
            panic!("expected Err for bad format version");
        };
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }

    /// Adapter that always fails to connect — simulates unreachable DB.
    #[derive(Debug, Clone)]
    struct FailingAdapter;

    #[async_trait]
    impl DatabaseAdapter for FailingAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Err(FraiseQLError::database("connection refused"))
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl FromPoolConfig for FailingAdapter {
        async fn from_pool_config(_config: &TenantPoolConfig) -> FraiseQLResult<Self> {
            Err(FraiseQLError::ConnectionPool {
                message: "connection refused".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_create_tenant_executor_unreachable_db() {
        let schema = CompiledSchema::default();
        let schema_json = serde_json::to_string(&schema).unwrap();
        let config = test_pool_config();

        let Err(err) =
            create_tenant_executor::<FailingAdapter>("acme", &schema_json, &config).await
        else {
            panic!("expected Err for unreachable DB");
        };
        assert!(
            matches!(err, FraiseQLError::ConnectionPool { .. }),
            "Expected ConnectionPool error, got: {err:?}"
        );
    }

    #[test]
    fn test_pool_config_defaults() {
        let json = r#"{"connection_string": "postgres://localhost/test"}"#;
        let config: TenantPoolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.idle_timeout_secs, 300);
    }
}
