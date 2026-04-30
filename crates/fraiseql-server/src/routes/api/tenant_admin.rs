//! Tenant management admin API endpoints.
//!
//! All endpoints require multi-tenant mode to be enabled (tenant registry present
//! in `AppState`). When disabled, they return 404 to avoid leaking the feature.
//!
//! Write endpoints (PUT, DELETE) require `admin_token`.
//! Read endpoints (GET, health) accept `admin_readonly_token` or `admin_token`.

use axum::{
    Json,
    extract::{Path, State},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    routes::{
        api::types::ApiError,
        graphql::{AppState, tenant_registry::TenantQuota},
    },
    tenancy::pool_factory::TenantPoolConfig,
};

// ── Request / Response types ─────────────────────────────────────────────

/// Body for `PUT /api/v1/admin/tenants/{key}`.
#[derive(Debug, Deserialize)]
pub struct TenantRegistrationRequest {
    /// Compiled schema JSON (the full `schema.compiled.json` contents).
    pub schema:               serde_json::Value,
    /// Database connection configuration for this tenant.
    pub connection:           TenantPoolConfig,
    /// Maximum requests per second (token bucket rate). `None` = unlimited.
    #[serde(default)]
    pub max_requests_per_sec: Option<u32>,
    /// Maximum concurrent in-flight requests. `None` = unlimited.
    #[serde(default)]
    pub max_concurrent:       Option<u32>,
    /// Maximum storage in bytes (soft limit). `None` = unlimited.
    #[serde(default)]
    pub max_storage_bytes:    Option<u64>,
}

/// Response for tenant write operations.
#[derive(Debug, Serialize)]
pub struct TenantResponse {
    /// The tenant key.
    pub key:    String,
    /// Whether this was `"created"`, `"updated"`, or `"removed"`.
    pub status: &'static str,
}

/// Response for `GET /api/v1/admin/tenants/{key}`.
#[derive(Debug, Serialize)]
pub struct TenantMetadata {
    /// The tenant key.
    pub key:            String,
    /// Tenant lifecycle status (`"active"` or `"suspended"`).
    pub status:         &'static str,
    /// Number of queries in the tenant's compiled schema.
    pub query_count:    usize,
    /// Number of mutations in the tenant's compiled schema.
    pub mutation_count: usize,
}

/// Response for `GET /api/v1/admin/tenants`.
#[derive(Debug, Serialize)]
pub struct TenantListResponse {
    /// All registered tenant keys.
    pub tenants: Vec<String>,
    /// Number of registered tenants.
    pub count:   usize,
}

/// Response for `GET /api/v1/admin/tenants/{key}/health`.
#[derive(Debug, Serialize)]
pub struct TenantHealthResponse {
    /// The tenant key.
    pub key:    String,
    /// Health status.
    pub status: &'static str,
}

/// Body for `PUT /api/v1/admin/domains/{domain}`.
#[derive(Debug, Deserialize)]
pub struct DomainRegistrationRequest {
    /// The tenant key to map this domain to.
    pub tenant_key: String,
}

/// Response for domain write operations.
#[derive(Debug, Serialize)]
pub struct DomainResponse {
    /// The domain name.
    pub domain:     String,
    /// Whether this was `"registered"` or `"removed"`.
    pub status:     &'static str,
    /// The tenant key the domain maps to (omitted on removal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_key: Option<String>,
}

/// Response for `GET /api/v1/admin/domains`.
#[derive(Debug, Serialize)]
pub struct DomainListResponse {
    /// All registered domain → tenant key mappings.
    pub domains: Vec<DomainMapping>,
    /// Number of registered domains.
    pub count:   usize,
}

/// A single domain → tenant key mapping.
#[derive(Debug, Serialize)]
pub struct DomainMapping {
    /// The custom domain.
    pub domain:     String,
    /// The tenant key it resolves to.
    pub tenant_key: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────

/// `PUT /api/v1/admin/tenants/{key}` — register or update a tenant.
///
/// Accepts compiled schema JSON and connection configuration in a single request.
/// Returns `"created"` or `"updated"` status.
///
/// Uses the `TenantExecutorFactory` stored in `AppState` to construct the
/// executor, avoiding the need for `A: FromPoolConfig` on the handler.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled, 400 for invalid
/// schema JSON, or 503 if the connection cannot be established.
pub async fn upsert_tenant_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
    Json(body): Json<TenantRegistrationRequest>,
) -> Result<Json<TenantResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    let factory = state
        .tenant_executor_factory()
        .ok_or_else(|| ApiError::internal_error("tenant executor factory not configured"))?;

    let schema_json = serde_json::to_string(&body.schema)
        .map_err(|e| ApiError::validation_error(format!("invalid schema JSON: {e}")))?;

    let executor = factory(key.clone(), schema_json, body.connection).await.map_err(|e| match &e {
        fraiseql_error::FraiseQLError::Parse { .. }
        | fraiseql_error::FraiseQLError::Validation { .. } => ApiError::validation_error(e),
        fraiseql_error::FraiseQLError::ConnectionPool { .. }
        | fraiseql_error::FraiseQLError::Database { .. } => {
            ApiError::new(format!("Connection failed: {e}"), "SERVICE_UNAVAILABLE")
        },
        _ => ApiError::internal_error(e),
    })?;

    let quota = TenantQuota {
        max_requests_per_sec: body.max_requests_per_sec,
        max_concurrent:       body.max_concurrent,
        max_storage_bytes:    body.max_storage_bytes,
    };

    let was_insert = registry.upsert_with_quota(&key, executor, quota);
    let status = if was_insert { "created" } else { "updated" };

    info!(tenant_key = %key, status, "tenant executor registered");

    Ok(Json(TenantResponse { key, status }))
}

/// `DELETE /api/v1/admin/tenants/{key}` — remove a tenant.
///
/// In-flight requests on the old executor complete via Arc semantics.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the tenant
/// key is not found.
pub async fn delete_tenant_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
) -> Result<Json<TenantResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    registry
        .remove(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    info!(tenant_key = %key, "tenant executor removed");

    Ok(Json(TenantResponse {
        key,
        status: "removed",
    }))
}

/// `POST /api/v1/admin/tenants/{key}/suspend` — suspend a tenant.
///
/// Suspended tenants' data requests return 503 with `Retry-After: 60`.
/// No executor teardown occurs — database connections remain open.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the tenant
/// key is not found.
pub async fn suspend_tenant_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
) -> Result<Json<TenantResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    registry
        .suspend(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    info!(tenant_key = %key, "tenant suspended");

    Ok(Json(TenantResponse {
        key,
        status: "suspended",
    }))
}

/// `POST /api/v1/admin/tenants/{key}/resume` — resume a suspended tenant.
///
/// Restores the tenant to active status so data requests are served normally.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the tenant
/// key is not found.
pub async fn resume_tenant_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
) -> Result<Json<TenantResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    registry
        .resume(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    info!(tenant_key = %key, "tenant resumed");

    Ok(Json(TenantResponse {
        key,
        status: "resumed",
    }))
}

/// `GET /api/v1/admin/tenants/{key}` — get tenant metadata.
///
/// Returns query/mutation counts. Never includes credentials.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the tenant
/// key is not found.
pub async fn get_tenant_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
) -> Result<Json<TenantMetadata>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    let status = registry
        .tenant_status(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    let executor = registry
        .executor_for_admin(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    Ok(Json(TenantMetadata {
        key,
        status: status.as_str(),
        query_count: executor.schema().queries.len(),
        mutation_count: executor.schema().mutations.len(),
    }))
}

/// `GET /api/v1/admin/tenants` — list all registered tenant keys.
///
/// Never includes credentials.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled.
pub async fn list_tenants_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> Result<Json<TenantListResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    let tenants = registry.tenant_keys();
    let count = tenants.len();

    Ok(Json(TenantListResponse { tenants, count }))
}

/// `GET /api/v1/admin/tenants/{key}/health` — health check a tenant's pool.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the tenant
/// key is not found. Returns 503 if the health check fails.
pub async fn tenant_health_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
) -> Result<Json<TenantHealthResponse>, ApiError> {
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    registry.health_check(&key).await.map_err(|e| match &e {
        fraiseql_error::FraiseQLError::NotFound { .. } => {
            ApiError::not_found(format!("tenant '{key}'"))
        },
        _ => ApiError::new(format!("Health check failed: {e}"), "SERVICE_UNAVAILABLE"),
    })?;

    Ok(Json(TenantHealthResponse {
        key,
        status: "healthy",
    }))
}

// ── Domain management handlers ──────────────────────────────────────────

/// `PUT /api/v1/admin/domains/{domain}` — register a domain → tenant mapping.
///
/// Validates that the referenced tenant key exists in the tenant registry
/// (when multi-tenant mode is enabled). Overwrites any existing mapping
/// for the same domain.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the
/// referenced tenant key is not registered.
pub async fn upsert_domain_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(domain): Path<String>,
    Json(body): Json<DomainRegistrationRequest>,
) -> Result<Json<DomainResponse>, ApiError> {
    // Multi-tenant mode must be enabled
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    // Verify the tenant key is actually registered
    registry
        .executor_for(Some(&body.tenant_key))
        .map_err(|_| ApiError::not_found(format!("tenant '{}'", body.tenant_key)))?;

    state.domain_registry().register(&domain, &body.tenant_key);

    info!(domain = %domain, tenant_key = %body.tenant_key, "domain mapping registered");

    Ok(Json(DomainResponse {
        domain,
        status: "registered",
        tenant_key: Some(body.tenant_key),
    }))
}

/// `DELETE /api/v1/admin/domains/{domain}` — remove a domain mapping.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled or the
/// domain is not registered.
pub async fn delete_domain_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(domain): Path<String>,
) -> Result<Json<DomainResponse>, ApiError> {
    state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    if !state.domain_registry().remove(&domain) {
        return Err(ApiError::not_found(format!("domain '{domain}'")));
    }

    info!(domain = %domain, "domain mapping removed");

    Ok(Json(DomainResponse {
        domain,
        status: "removed",
        tenant_key: None,
    }))
}

/// `GET /api/v1/admin/domains` — list all domain → tenant mappings.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled.
pub async fn list_domains_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> Result<Json<DomainListResponse>, ApiError> {
    state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    let mappings = state.domain_registry().domains();
    let count = mappings.len();

    Ok(Json(DomainListResponse {
        domains: mappings
            .into_iter()
            .map(|(domain, tenant_key)| DomainMapping { domain, tenant_key })
            .collect(),
        count,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };

    use super::*;
    use crate::routes::graphql::TenantExecutorRegistry;

    /// Stub adapter for tenant admin tests.
    #[derive(Debug, Clone)]
    struct StubAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
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

    fn make_multitenant_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        let state = AppState::new(executor);
        let registry = TenantExecutorRegistry::new(state.executor.clone());
        state.with_tenant_registry(Arc::new(registry))
    }

    fn make_single_tenant_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor)
    }

    // ── Unit tests for handler logic (via direct state manipulation) ─────

    #[test]
    fn test_single_tenant_mode_has_no_registry() {
        let state = make_single_tenant_state();
        assert!(state.tenant_registry().is_none());
    }

    #[test]
    fn test_multi_tenant_empty_registry() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.tenant_keys().len(), 0);
    }

    #[test]
    fn test_register_and_list_tenants() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.tenant_keys(), vec!["tenant-abc"]);
    }

    #[test]
    fn test_upsert_existing_returns_false() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        assert!(registry.upsert("tenant-abc", executor));

        let executor2 = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        assert!(!registry.upsert("tenant-abc", executor2));
    }

    #[test]
    fn test_delete_unknown_returns_error() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();
        assert!(registry.remove("unknown").is_err());
    }

    #[test]
    fn test_get_tenant_metadata_via_registry() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let mut schema = CompiledSchema::default();
        schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        let exec = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(exec.schema().queries.len(), 1);
        assert_eq!(exec.schema().mutations.len(), 0);
    }

    #[tokio::test]
    async fn test_health_check_registered_tenant() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        assert!(registry.health_check("tenant-abc").await.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_unknown_tenant() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        assert!(registry.health_check("unknown").await.is_err());
    }

    // ── Domain management tests ─────────────────────────────────────────

    #[test]
    fn test_domain_registry_register_and_list() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        // Register a tenant first
        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        // Register a domain mapping
        state.domain_registry().register("api.acme.com", "tenant-abc");

        let mappings = state.domain_registry().domains();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].0, "api.acme.com");
        assert_eq!(mappings[0].1, "tenant-abc");
    }

    #[test]
    fn test_domain_registry_remove() {
        let state = make_multitenant_state();

        state.domain_registry().register("api.acme.com", "tenant-abc");
        assert!(state.domain_registry().remove("api.acme.com"));
        assert!(!state.domain_registry().remove("api.acme.com"));
    }

    #[test]
    fn test_domain_registry_lookup_with_port() {
        let state = make_multitenant_state();
        state.domain_registry().register("api.acme.com", "tenant-abc");

        assert_eq!(
            state.domain_registry().lookup("api.acme.com:8080"),
            Some("tenant-abc".to_string())
        );
    }

    #[test]
    fn test_domain_empty_in_single_tenant_mode() {
        let state = make_single_tenant_state();
        assert!(state.domain_registry().is_empty());
    }
}
