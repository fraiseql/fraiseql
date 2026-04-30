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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    // ── tenant_schema_name ──────────────────────────────────────────────

    #[test]
    fn valid_key_produces_prefixed_name() {
        assert_eq!(tenant_schema_name("acme").unwrap(), "tenant_acme");
    }

    #[test]
    fn alphanumeric_key_accepted() {
        assert_eq!(tenant_schema_name("org123").unwrap(), "tenant_org123");
    }

    #[test]
    fn underscore_in_key_accepted() {
        assert_eq!(
            tenant_schema_name("my_org").unwrap(),
            "tenant_my_org"
        );
    }

    #[test]
    fn empty_key_rejected() {
        let err = tenant_schema_name("").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "expected Validation, got: {err:?}"
        );
    }

    #[test]
    fn key_with_hyphen_rejected() {
        let err = tenant_schema_name("my-org").unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn key_with_dot_rejected() {
        let err = tenant_schema_name("my.org").unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn key_with_space_rejected() {
        assert!(tenant_schema_name("my org").is_err());
    }

    #[test]
    fn key_with_semicolon_rejected() {
        assert!(tenant_schema_name("org; DROP TABLE").is_err());
    }

    #[test]
    fn key_exceeding_max_length_rejected() {
        // MAX_PG_IDENTIFIER_LEN = 63, prefix = "tenant_" (7 chars)
        // So key can be at most 56 chars
        let long_key = "a".repeat(57);
        let err = tenant_schema_name(&long_key).unwrap_err();
        assert!(err.to_string().contains("63-character"));
    }

    #[test]
    fn key_at_max_length_accepted() {
        let key = "a".repeat(56); // tenant_ (7) + 56 = 63 exactly
        let name = tenant_schema_name(&key).unwrap();
        assert_eq!(name.len(), 63);
    }

    // ── DDL generation ──────────────────────────────────────────────────

    #[test]
    fn create_schema_ddl_generates_correct_sql() {
        assert_eq!(
            create_schema_ddl("acme").unwrap(),
            "CREATE SCHEMA IF NOT EXISTS tenant_acme"
        );
    }

    #[test]
    fn drop_schema_ddl_generates_correct_sql() {
        assert_eq!(
            drop_schema_ddl("acme").unwrap(),
            "DROP SCHEMA IF EXISTS tenant_acme CASCADE"
        );
    }

    #[test]
    fn create_schema_idempotent() {
        // IF NOT EXISTS means calling twice produces the same SQL
        let ddl1 = create_schema_ddl("acme").unwrap();
        let ddl2 = create_schema_ddl("acme").unwrap();
        assert_eq!(ddl1, ddl2);
        assert!(ddl1.contains("IF NOT EXISTS"));
    }

    #[test]
    fn create_schema_ddl_rejects_invalid_key() {
        assert!(create_schema_ddl("").is_err());
        assert!(create_schema_ddl("org; DROP").is_err());
    }

    #[test]
    fn drop_schema_ddl_rejects_invalid_key() {
        assert!(drop_schema_ddl("").is_err());
    }

    // ── search_path ─────────────────────────────────────────────────────

    #[test]
    fn search_path_sql_generates_correct_statement() {
        assert_eq!(
            search_path_sql("acme").unwrap(),
            "SET search_path TO tenant_acme, public"
        );
    }

    #[test]
    fn search_path_sql_rejects_invalid_key() {
        assert!(search_path_sql("").is_err());
    }

    // ── Row mode skips DDL ──────────────────────────────────────────────
    // (Row mode never calls these functions — verified at the caller level)

    // ── Async adapter functions ────────────────────────────────────────

    use std::sync::Mutex;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
    };

    /// Spy adapter that records all SQL passed to `execute_raw_query`.
    #[derive(Debug)]
    struct SpyAdapter {
        queries: Mutex<Vec<String>>,
    }

    impl SpyAdapter {
        fn new() -> Self {
            Self {
                queries: Mutex::new(Vec::new()),
            }
        }

        fn recorded_queries(&self) -> Vec<String> {
            self.queries.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for SpyAdapter {
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
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            self.queries.lock().unwrap().push(sql.to_string());
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

    #[tokio::test]
    async fn provision_executes_create_schema_ddl() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "CREATE SCHEMA IF NOT EXISTS tenant_acme");
    }

    #[tokio::test]
    async fn drop_executes_drop_schema_ddl() {
        let adapter = SpyAdapter::new();
        drop_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "DROP SCHEMA IF EXISTS tenant_acme CASCADE");
    }

    #[tokio::test]
    async fn configure_search_path_executes_set_statement() {
        let adapter = SpyAdapter::new();
        configure_search_path("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "SET search_path TO tenant_acme, public");
    }

    #[tokio::test]
    async fn provision_is_idempotent() {
        let adapter = SpyAdapter::new();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        provision_tenant_schema("acme", &adapter).await.unwrap();
        let queries = adapter.recorded_queries();
        assert_eq!(queries.len(), 2);
        // Both should be IF NOT EXISTS — idempotent
        assert!(queries[0].contains("IF NOT EXISTS"));
        assert!(queries[1].contains("IF NOT EXISTS"));
    }

    #[tokio::test]
    async fn provision_rejects_invalid_key() {
        let adapter = SpyAdapter::new();
        let err = provision_tenant_schema("my-org", &adapter).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        // No SQL should have been executed
        assert!(adapter.recorded_queries().is_empty());
    }

    #[tokio::test]
    async fn drop_rejects_invalid_key() {
        let adapter = SpyAdapter::new();
        let err = drop_tenant_schema("", &adapter).await.unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        assert!(adapter.recorded_queries().is_empty());
    }
}
