//! Schema-level tenant isolation utilities.
//!
//! When `tenancy.mode = "schema"`, each tenant gets a dedicated PostgreSQL
//! schema (`tenant_{key}`). This module provides:
//! - Schema name validation (valid Postgres identifiers, max 63 chars)
//! - DDL provisioning (`CREATE SCHEMA`) and teardown (`DROP SCHEMA CASCADE`)
//! - The [`SearchPath`] a tenant's pool is built with, and a post-registration check that it is
//!   genuinely in force on that pool's connections

use fraiseql_core::db::{postgres::SearchPath, traits::DatabaseAdapter};
use fraiseql_error::{FraiseQLError, Result};

/// Maximum length of a PostgreSQL identifier (schema name, table name, etc.).
///
/// Exposed `pub(crate)` so the `X-Tenant-ID` header validator can derive its own
/// length cap from the same source and avoid validator drift (#333).
pub(crate) const MAX_PG_IDENTIFIER_LEN: usize = 63;

/// Prefix prepended to tenant keys to form PostgreSQL schema names.
///
/// Exposed `pub(crate)` so the header validator's length cap can subtract it
/// from [`MAX_PG_IDENTIFIER_LEN`] (#333).
pub(crate) const TENANT_SCHEMA_PREFIX: &str = "tenant_";

/// Derive and validate a PostgreSQL schema name from a tenant key.
///
/// The resulting name is `tenant_{key}` and must be a valid PostgreSQL
/// identifier: alphanumeric + underscore only, max 63 characters.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is empty, contains
/// characters other than `[a-zA-Z0-9_]`, or would produce a schema name
/// exceeding 63 characters.
pub fn tenant_schema_name(key: &str) -> Result<String> {
    if key.is_empty() {
        return Err(FraiseQLError::validation("Tenant key must not be empty for schema isolation"));
    }

    // Only allow alphanumeric + underscore to prevent SQL injection
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(FraiseQLError::validation(format!(
            "Tenant key '{key}' contains invalid characters. \
             Only ASCII alphanumeric and underscore are allowed for schema isolation."
        )));
    }

    let schema_name = format!("{TENANT_SCHEMA_PREFIX}{key}");

    if schema_name.len() > MAX_PG_IDENTIFIER_LEN {
        return Err(FraiseQLError::validation(format!(
            "Tenant schema name '{schema_name}' exceeds PostgreSQL's \
             {MAX_PG_IDENTIFIER_LEN}-character identifier limit. \
             Use a shorter tenant key."
        )));
    }

    Ok(schema_name)
}

/// Build the [`SearchPath`] a tenant's connections must be established with.
///
/// Resolves unqualified relations against `tenant_{key}` first, then `public`.
///
/// This is deliberately *not* a `SET search_path TO …` statement. The path is a
/// property of the pool, applied by PostgreSQL while each connection is
/// established; issuing it as a statement configures only whichever connection is
/// checked out at the time and leaves every other connection in the pool on the
/// server default (#809).
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key produces an invalid schema name.
pub fn tenant_search_path(key: &str) -> Result<SearchPath> {
    let schema_name = tenant_schema_name(key)?;
    SearchPath::new([schema_name.as_str(), "public"])
}

/// Generate the `CREATE SCHEMA IF NOT EXISTS` DDL for a tenant.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key produces an invalid schema name.
pub fn create_schema_ddl(key: &str) -> Result<String> {
    let schema_name = tenant_schema_name(key)?;
    Ok(format!("CREATE SCHEMA IF NOT EXISTS {schema_name}"))
}

/// Generate the `DROP SCHEMA ... CASCADE` DDL for a tenant.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key produces an invalid schema name.
pub fn drop_schema_ddl(key: &str) -> Result<String> {
    let schema_name = tenant_schema_name(key)?;
    Ok(format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE"))
}

/// Provision a PostgreSQL schema for a tenant.
///
/// Executes `CREATE SCHEMA IF NOT EXISTS tenant_{key}` against the provided
/// adapter. Idempotent — calling multiple times for the same key is safe.
///
/// Because the DDL is `IF NOT EXISTS`, registering a tenant under a key whose
/// schema still holds a *previous* tenant's tables silently adopts them. That is
/// the second-order effect of #859: an operator who deletes a tenant without
/// `?purge=true` and later reuses the key gets the old rows served to the new
/// tenant, with nothing in the logs to say so. This function therefore counts the
/// relations it inherited and warns when it inherits any.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is invalid.
/// Returns `FraiseQLError::Database` if the DDL execution fails.
pub async fn provision_tenant_schema(key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    let schema_name = tenant_schema_name(key)?;
    let inherited = count_relations(&schema_name, adapter).await;

    let ddl = create_schema_ddl(key)?;
    adapter.execute_raw_query(&ddl).await.map_err(|e| {
        FraiseQLError::database(format!("Failed to provision schema for tenant '{key}': {e}"))
    })?;

    if inherited > 0 {
        tracing::warn!(
            tenant_key = key,
            schema = %schema_name,
            relations = inherited,
            "tenant registration adopted an existing schema that already contains \
             {inherited} relation(s). If this key was recycled, the new tenant will read \
             the previous tenant's rows — delete with ?purge=true to drop the schema first."
        );
    }
    Ok(())
}

/// Count relations in a schema, best-effort.
///
/// Returns 0 when the probe cannot run (non-PostgreSQL adapter, missing catalog
/// access). This only drives a warning, so a failure here must not block
/// registration — the provisioning DDL that follows is the operation that matters.
async fn count_relations(schema_name: &str, adapter: &dyn DatabaseAdapter) -> u64 {
    let sql = format!(
        "SELECT count(*)::text AS n FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = '{schema_name}'"
    );
    adapter
        .execute_raw_query(&sql)
        .await
        .ok()
        .and_then(|rows| {
            rows.first()
                .and_then(|r| r.get("n"))
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(0)
}

/// Drop a tenant's PostgreSQL schema and all its objects.
///
/// Executes `DROP SCHEMA IF EXISTS tenant_{key} CASCADE` against the provided
/// adapter. Idempotent — dropping a non-existent schema is a no-op.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is invalid.
/// Returns `FraiseQLError::Database` if the DDL execution fails.
pub async fn drop_tenant_schema(key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    let ddl = drop_schema_ddl(key)?;
    adapter.execute_raw_query(&ddl).await.map_err(|e| {
        FraiseQLError::database(format!("Failed to drop schema for tenant '{key}': {e}"))
    })?;
    Ok(())
}

/// Prove that a tenant adapter's connections really are established with the
/// tenant search path, and refuse the registration if they are not.
///
/// Reads `pg_settings.reset_val` for `search_path`, **not** `current_setting`.
/// That distinction is the whole point: `reset_val` is the value a connection was
/// *established* with, so it reflects the pool's startup options and is unaffected
/// by any `SET` a session may have issued. A guard written against
/// `current_setting` would pass for the broken session-`SET` mechanism this
/// replaces (#809) and so would prove nothing.
///
/// A `FromPoolConfig` implementation that ignores [`TenantPoolConfig::search_path`],
/// or a backend with no equivalent mechanism, therefore fails registration loudly
/// instead of silently serving `public`.
///
/// [`TenantPoolConfig::search_path`]: super::pool_factory::TenantPoolConfig::search_path
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is invalid.
/// Returns `FraiseQLError::Database` if the setting cannot be read, or
/// `FraiseQLError::Configuration` if it does not name the tenant's schema.
pub async fn verify_search_path(key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    let expected = tenant_search_path(key)?;
    let rows = adapter
        .execute_raw_query("SELECT reset_val FROM pg_settings WHERE name = 'search_path'")
        .await
        .map_err(|e| {
            FraiseQLError::database(format!(
                "Failed to verify schema isolation for tenant '{key}': {e}. Schema-per-tenant \
                 tenancy requires a PostgreSQL adapter whose pool applies the tenant search \
                 path at connection establishment."
            ))
        })?;

    let actual = rows
        .first()
        .and_then(|row| row.get("reset_val"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    // PostgreSQL normalises the stored value's spacing; compare on the schema list.
    let normalized: Vec<&str> = actual.split(',').map(str::trim).collect();
    let wanted: Vec<&str> = expected.as_str().split(',').map(str::trim).collect();
    if normalized == wanted {
        return Ok(());
    }

    Err(FraiseQLError::Configuration {
        message: format!(
            "Schema isolation for tenant '{key}' is not in force: connections in this \
             tenant's pool are established with search_path `{actual}`, expected \
             `{expected}`. Queries would resolve unqualified relations outside the \
             tenant's schema. Refusing to register the tenant."
        ),
    })
}
