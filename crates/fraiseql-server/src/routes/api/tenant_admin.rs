//! Tenant management admin API endpoints.
//!
//! All endpoints require multi-tenant mode to be enabled (tenant registry present
//! in `AppState`). When disabled, they return 404 to avoid leaking the feature.
//!
//! Write endpoints (PUT, DELETE) require `admin_token`.
//! Read endpoints (GET, health) accept `admin_readonly_token` or `admin_token`.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    routes::{
        api::types::ApiError,
        graphql::{AppState, tenant_registry::TenantQuota},
    },
    tenancy::{audit::TenantEventKind, pool_factory::TenantPoolConfig},
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

/// Query parameters for `GET /api/v1/admin/tenants/{key}/events`.
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Maximum number of events to return (default: 50, max: 200).
    #[serde(default = "default_events_limit")]
    pub limit:  usize,
    /// Offset for pagination (default: 0).
    #[serde(default)]
    pub offset: usize,
}

const fn default_events_limit() -> usize {
    50
}

/// Response for `GET /api/v1/admin/tenants/{key}/events`.
#[derive(Debug, Serialize)]
pub struct TenantEventsResponse {
    /// The tenant key.
    pub key:    String,
    /// The events, newest first.
    pub events: Vec<crate::tenancy::audit::TenantEvent>,
    /// Total number of events returned.
    pub count:  usize,
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
    // Reject keys that the header validator would accept but schema-mode
    // provisioning would later reject, so the drift surfaces at registration
    // time rather than at the first schema-mode DDL job (#333).
    crate::routes::graphql::tenant_key::validate_tenant_key(&key)
        .map_err(|e| ApiError::validation_error(e.to_string()))?;

    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    let factory = state
        .tenant_executor_factory()
        .ok_or_else(|| ApiError::internal_error("tenant executor factory not configured"))?;

    let schema_json = serde_json::to_string(&body.schema)
        .map_err(|e| ApiError::validation_error(format!("invalid schema JSON: {e}")))?;

    let executor =
        factory(key.clone(), schema_json, body.connection).await.map_err(|e| match &e {
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

    // Record audit event (fire-and-forget — audit failure must not block the operation)
    if let Some(audit_log) = state.tenant_audit_log() {
        let event = if was_insert {
            TenantEventKind::Created
        } else {
            TenantEventKind::ConfigChanged
        };
        if let Err(e) = audit_log.record(&key, event, None, None).await {
            tracing::warn!(tenant_key = %key, error = %e, "failed to record audit event");
        }
    }

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

    if let Some(audit_log) = state.tenant_audit_log() {
        if let Err(e) = audit_log.record(&key, TenantEventKind::Deleted, None, None).await {
            tracing::warn!(tenant_key = %key, error = %e, "failed to record audit event");
        }
    }

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

    if let Some(audit_log) = state.tenant_audit_log() {
        if let Err(e) = audit_log.record(&key, TenantEventKind::Suspended, None, None).await {
            tracing::warn!(tenant_key = %key, error = %e, "failed to record audit event");
        }
    }

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

    if let Some(audit_log) = state.tenant_audit_log() {
        if let Err(e) = audit_log.record(&key, TenantEventKind::Resumed, None, None).await {
            tracing::warn!(tenant_key = %key, error = %e, "failed to record audit event");
        }
    }

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

/// Maximum events per page to prevent abuse.
const MAX_EVENTS_LIMIT: usize = 200;

/// `GET /api/v1/admin/tenants/{key}/events` — query tenant audit trail.
///
/// Returns lifecycle events for a specific tenant, newest first.
/// Supports pagination via `limit` and `offset` query parameters.
///
/// # Errors
///
/// Returns `ApiError` with 404 if multi-tenant mode is disabled, the tenant
/// key is not found, or no audit log is configured.
pub async fn tenant_events_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Path(key): Path<String>,
    Query(params): Query<EventsQuery>,
) -> Result<Json<TenantEventsResponse>, ApiError> {
    // Multi-tenant mode must be enabled + verify tenant exists
    let registry = state
        .tenant_registry()
        .ok_or_else(|| ApiError::not_found("multi-tenant mode not enabled"))?;

    registry
        .executor_for_admin(&key)
        .map_err(|_| ApiError::not_found(format!("tenant '{key}'")))?;

    let audit_log = state
        .tenant_audit_log()
        .ok_or_else(|| ApiError::not_found("audit log not configured"))?;

    let limit = params.limit.min(MAX_EVENTS_LIMIT);
    let events = audit_log
        .events_for(&key, limit, params.offset)
        .await
        .map_err(|e| ApiError::internal_error(format!("failed to query audit events: {e}")))?;

    let count = events.len();

    Ok(Json(TenantEventsResponse { key, events, count }))
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
