//! Tenant pool creation and executor construction.
//!
//! Provides [`TenantPoolConfig`] and [`create_tenant_executor`] to build a
//! fully-formed `Executor<A>` from a compiled schema JSON string and database
//! connection configuration. Used by the management API to register
//! tenants at runtime.

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{
        postgres::{PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig, SearchPath},
        traits::DatabaseAdapter,
    },
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
    /// Schema isolation for this tenant, derived from the compiled schema's
    /// tenancy mode and the tenant key — never supplied by the caller.
    ///
    /// `#[serde(skip)]` is load-bearing: this field is the isolation boundary, and
    /// the struct is deserialised straight from an admin-API request body. Letting
    /// a registration request name its own search path would let a caller point a
    /// tenant at another tenant's schema.
    ///
    /// [`create_tenant_executor`] populates it before the adapter is built, so an
    /// adapter cannot exist in a state where the isolation "has not been applied
    /// yet" — the defect the removed `configure_search_path` used to have (#809).
    #[serde(skip)]
    pub search_path:          Option<SearchPath>,
    /// Transport security for this tenant's connections, inherited from the
    /// server's `[database_tls]` settings — never supplied by the caller.
    ///
    /// `#[serde(skip)]` for the same reason as [`search_path`](Self::search_path):
    /// the struct is deserialised straight from an admin-API request body, so an
    /// attacker-controlled registration could otherwise send
    /// `{"tls": {"mode": "disable"}}` and downgrade its own tenant's database
    /// traffic to cleartext — turning a server-wide `verify-full` into a
    /// per-tenant opt-out. [`create_tenant_executor`] populates it from the
    /// server configuration before the adapter is built.
    #[serde(skip)]
    pub tls:                  PostgresTlsConfig,
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

#[async_trait::async_trait]
impl FromPoolConfig for PostgresAdapter {
    async fn from_pool_config(config: &TenantPoolConfig) -> Result<Self> {
        Self::with_pool_config(
            &config.connection_string,
            PoolPrewarmConfig {
                // Per-tenant pools are created on demand at registration; don't eagerly
                // pre-warm (min_size = 0). `with_pool_config` still opens one connection
                // for the startup health check, validating the connection string.
                min_size: 0,
                // Reason: `max_connections` is a small operator-set pool bound; usize is
                // at least 32-bit on every supported target, so the cast cannot truncate.
                #[allow(clippy::cast_possible_truncation)]
                max_size: config.max_connections as usize,
                timeout_secs: Some(config.connect_timeout_secs),
                search_path: config.search_path.clone(),
                tls: config.tls.clone(),
            },
        )
        .await
    }
}

/// The binary's `Server` wraps its adapter in a [`CachedDatabaseAdapter`], so the
/// per-tenant executor registry stores `Executor<CachedDatabaseAdapter<A>>` and the
/// factory must build that wrapped type. Each tenant gets its own fresh, isolated
/// [`QueryResultCache`]; on a schema update the whole executor is replaced
/// (`TenantExecutorRegistry::upsert`), so the cache is rebuilt rather than
/// version-invalidated — an empty `schema_version` namespace is sufficient and
/// collision-free. Per-tenant caches use `CacheConfig::default()` (they do not
/// inherit the server's tuned view-TTL / cacheable-view configuration).
#[async_trait::async_trait]
impl<A: FromPoolConfig> FromPoolConfig for CachedDatabaseAdapter<A> {
    async fn from_pool_config(config: &TenantPoolConfig) -> Result<Self> {
        let inner = A::from_pool_config(config).await?;
        let cache = QueryResultCache::new(CacheConfig::default());
        Ok(Self::new(inner, cache, String::new()))
    }
}

/// Creates a complete tenant executor from a compiled schema JSON string and
/// connection configuration.
///
/// This is the primary entry point for tenant registration: it parses the schema,
/// validates its format version, creates a database pool, and assembles an
/// `Executor<A>` with both baked in.
///
/// When the compiled schema specifies `tenancy.mode = "schema"`, the tenant's
/// search path is baked into the pool **before** it is built, so every connection
/// the pool ever opens carries it, and the tenant's PostgreSQL schema is then
/// provisioned (`CREATE SCHEMA IF NOT EXISTS tenant_{key}`).
///
/// The ordering matters and is the fix for #809: isolation is a property of the
/// pool, established at construction, not a statement issued afterwards against
/// one borrowed connection. Registration then *verifies* the isolation actually
/// took (see [`schema_isolation::verify_search_path`]) and refuses to hand back an
/// executor that would serve `public`.
///
/// # Arguments
///
/// * `tenant_key` - The tenant identifier used for schema naming
/// * `schema_json` - Compiled schema JSON string
/// * `pool_config` - Database connection configuration. Its `search_path` is ignored and recomputed
///   here; the tenant key is the only source of truth.
///
/// # Errors
///
/// Returns `FraiseQLError::Parse` if the schema JSON is invalid.
/// Returns `FraiseQLError::Validation` if the schema format version is unsupported
/// or the tenant key would produce an invalid PostgreSQL schema name.
/// Returns `FraiseQLError::ConnectionPool` / `FraiseQLError::Database` if the pool
/// cannot be created, schema DDL fails, or the pool's connections do not carry the
/// tenant search path.
#[doc(hidden)] // Internal-pub: tenant pool builder used by TenantExecutorRegistry; downstream wires tenants via TenancyConfig, not this fn directly.
pub async fn create_tenant_executor<A: FromPoolConfig>(
    tenant_key: &str,
    schema_json: &str,
    pool_config: &TenantPoolConfig,
) -> Result<Arc<Executor<A>>> {
    // 1. Parse and validate schema
    let schema =
        CompiledSchema::from_json(schema_json, false).map_err(|e| FraiseQLError::Parse {
            message:  format!("Invalid compiled schema JSON: {e}"),
            location: String::new(),
        })?;

    schema
        .validate_format_version()
        .map_err(|msg| FraiseQLError::validation(format!("Incompatible compiled schema: {msg}")))?;

    let tenancy_mode = schema.tenancy_mode();

    // 2. Derive the isolation the tenancy mode calls for, and put it in the config the pool is
    //    built from. Recomputed rather than trusted: `search_path` is `#[serde(skip)]`, but an
    //    in-process caller could still have set it.
    let pool_config = TenantPoolConfig {
        search_path: if tenancy_mode == TenancyMode::Schema {
            Some(schema_isolation::tenant_search_path(tenant_key)?)
        } else {
            None
        },
        ..pool_config.clone()
    };

    // 3. Create database adapter/pool — isolation applies from the first connection
    let adapter = A::from_pool_config(&pool_config).await?;

    // 4. Schema isolation: provision the schema, then prove the isolation is live
    if tenancy_mode == TenancyMode::Schema {
        info!(tenant_key, "provisioning schema for tenant (schema isolation mode)");
        schema_isolation::provision_tenant_schema(tenant_key, &adapter).await?;
        schema_isolation::verify_search_path(tenant_key, &adapter).await?;
    }

    // 5. Assemble executor
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
pub async fn destroy_tenant_schema(tenant_key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    schema_isolation::drop_tenant_schema(tenant_key, adapter).await
}
