//! Schema-level tenant isolation utilities.
//!
//! When `tenancy.mode = "schema"`, each tenant gets a dedicated PostgreSQL
//! schema (`tenant_{key}`). This module provides:
//! - Schema name validation (valid Postgres identifiers, max 63 chars)
//! - DDL provisioning (`CREATE SCHEMA`) and teardown (`DROP SCHEMA CASCADE`)
//! - Search path configuration for per-connection schema routing

use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_error::{FraiseQLError, Result};

/// Maximum length of a PostgreSQL identifier (schema name, table name, etc.).
const MAX_PG_IDENTIFIER_LEN: usize = 63;

/// Prefix prepended to tenant keys to form PostgreSQL schema names.
const TENANT_SCHEMA_PREFIX: &str = "tenant_";

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
        return Err(FraiseQLError::validation(
            "Tenant key must not be empty for schema isolation",
        ));
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

/// Generate the `SET search_path` SQL statement for a tenant schema.
///
/// Returns `SET search_path TO tenant_{key}, public` which routes all
/// unqualified table references to the tenant's schema first, then `public`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key produces an invalid schema name.
pub fn search_path_sql(key: &str) -> Result<String> {
    let schema_name = tenant_schema_name(key)?;
    Ok(format!("SET search_path TO {schema_name}, public"))
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
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is invalid.
/// Returns `FraiseQLError::Database` if the DDL execution fails.
pub async fn provision_tenant_schema(key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    let ddl = create_schema_ddl(key)?;
    adapter.execute_raw_query(&ddl).await.map_err(|e| {
        FraiseQLError::database(format!(
            "Failed to provision schema for tenant '{key}': {e}"
        ))
    })?;
    Ok(())
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
        FraiseQLError::database(format!(
            "Failed to drop schema for tenant '{key}': {e}"
        ))
    })?;
    Ok(())
}

/// Configure the search path for a tenant's schema on a database adapter.
///
/// Executes `SET search_path TO tenant_{key}, public` which routes all
/// unqualified table references to the tenant's schema first.
///
/// This should be called after creating a tenant adapter to ensure all
/// subsequent queries use the correct schema.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is invalid.
/// Returns `FraiseQLError::Database` if the SET statement fails.
pub async fn configure_search_path(key: &str, adapter: &dyn DatabaseAdapter) -> Result<()> {
    let sql = search_path_sql(key)?;
    adapter.execute_raw_query(&sql).await.map_err(|e| {
        FraiseQLError::database(format!(
            "Failed to set search_path for tenant '{key}': {e}"
        ))
    })?;
    Ok(())
}
