//! Tenant pool creation and executor construction.
//!
//! Provides [`TenantPoolConfig`] and [`create_tenant_executor`] to build a
//! fully-formed `Executor<A>` from a compiled schema JSON string and database
//! connection configuration. Used by the management API to register
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
    let schema = CompiledSchema::from_json(schema_json, false).map_err(|e| FraiseQLError::Parse {
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
