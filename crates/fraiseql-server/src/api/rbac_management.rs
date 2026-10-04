//! Role and Permission Management API
//!
//! REST API endpoints for managing roles, permissions, and user-role associations.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use self::db_backend::{AuditFilter, RbacDbError};
use super::admin_principal::{AdminPrincipal, foreign_tenant_response};

/// Default page size for list endpoints.
///
/// The old handlers hard-coded `100` with no way to change it and no indication
/// when a result was cut off (#769). It survives as the *default*, but as one the
/// caller can override and whose truncation is reported.
const DEFAULT_LIMIT: u32 = 100;

/// Largest page a caller may request. A `limit` above this is refused rather than
/// silently reduced — silently reducing is the shape this phase exists to remove.
const MAX_LIMIT: u32 = 1000;

/// Role definition for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDto {
    /// Unique role identifier
    pub id:          String,
    /// Human-readable role name
    pub name:        String,
    /// Optional role description
    pub description: Option<String>,
    /// List of permission IDs assigned to this role
    pub permissions: Vec<String>,
    /// Tenant ID for multi-tenancy
    pub tenant_id:   Option<String>,
    /// Creation timestamp (ISO 8601)
    pub created_at:  String,
    /// Last update timestamp (ISO 8601)
    pub updated_at:  String,
}

/// Permission definition for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDto {
    /// Unique permission identifier
    pub id:          String,
    /// Permission resource and action (e.g., "query:read", "mutation:write")
    pub resource:    String,
    /// The action part of the permission (e.g., `"read"`, `"write"`, `"delete"`).
    pub action:      String,
    /// Optional permission description
    pub description: Option<String>,
    /// Creation timestamp (ISO 8601)
    pub created_at:  String,
}

/// User-Role association for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRoleDto {
    /// User ID
    pub user_id:     String,
    /// Role ID
    pub role_id:     String,
    /// Tenant ID for multi-tenancy
    pub tenant_id:   Option<String>,
    /// Assignment timestamp (ISO 8601)
    pub assigned_at: String,
}

/// Request to create a new role.
///
/// `deny_unknown_fields` is deliberate: the tenant scope arrives in this body, and a
/// misspelled `tenantId` that serde silently ignored would create a *global* role
/// while the caller believed it was tenant-scoped. That is the same silent-drop
/// class as #806/#757, applied to an authorization boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRoleRequest {
    /// Role name
    pub name:        String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Initial permissions to assign, as `"resource:action"` strings.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Tenant this role belongs to. Omit for a global role (deployment administrator) or
    /// for the administrator's own tenant (tenant administrator, #1089).
    #[serde(default)]
    pub tenant_id:   Option<String>,
}

/// Request to create a new permission
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePermissionRequest {
    /// Resource name
    pub resource:    String,
    /// Action name
    pub action:      String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
}

/// Request to assign a role to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignRoleRequest {
    /// User ID
    pub user_id:   String,
    /// Role ID to assign
    pub role_id:   String,
    /// Tenant the assignment is scoped to. Omit for a global assignment.
    #[serde(default)]
    pub tenant_id: Option<String>,
}

// =============================================================================
// Query parameters
// =============================================================================

/// Pagination and tenant scoping for the list endpoints.
///
/// `deny_unknown_fields` turns `?tenantid=…` into a `400` instead of an unscoped
/// read — a mistyped scope must not silently widen the result set.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// Restrict to one tenant. Omit for the operator's global view.
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Page size (default 100, maximum 1000).
    #[serde(default)]
    pub limit:     Option<u32>,
    /// Page offset (default 0).
    #[serde(default)]
    pub offset:    Option<u32>,
}

/// Query parameters for `GET /api/user-roles`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserRolesQuery {
    /// The user whose assignments to list. Required.
    pub user_id:   String,
    /// Restrict to one tenant.
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Page size (default 100, maximum 1000).
    #[serde(default)]
    pub limit:     Option<u32>,
    /// Page offset (default 0).
    #[serde(default)]
    pub offset:    Option<u32>,
}

/// Query parameters for `GET /api/audit/permissions`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    /// Restrict to events about this subject user.
    #[serde(default)]
    pub user_id:    Option<String>,
    /// Restrict to events about this role.
    #[serde(default)]
    pub role_id:    Option<String>,
    /// Restrict to one event type.
    #[serde(default)]
    pub event_type: Option<String>,
    /// Restrict to one tenant.
    #[serde(default)]
    pub tenant_id:  Option<String>,
    /// Inclusive lower bound, RFC 3339.
    #[serde(default)]
    pub start_time: Option<String>,
    /// Inclusive upper bound, RFC 3339.
    #[serde(default)]
    pub end_time:   Option<String>,
    /// Page size (default 100, maximum 1000).
    #[serde(default)]
    pub limit:      Option<u32>,
    /// Page offset (default 0).
    #[serde(default)]
    pub offset:     Option<u32>,
}

/// Resolve a caller-supplied page size, refusing an out-of-range one.
///
/// The error is the *message*, not a built `Response`: an axum `Response` is a large
/// `Err` variant (`clippy::result_large_err`), and a helper that returns one forces
/// every caller to carry it.
fn resolve_limit(limit: Option<u32>) -> Result<u32, String> {
    match limit {
        None => Ok(DEFAULT_LIMIT),
        Some(0) => Err("limit must be at least 1".to_string()),
        Some(n) if n > MAX_LIMIT => Err(format!("limit must not exceed {MAX_LIMIT}")),
        Some(n) => Ok(n),
    }
}

/// Parse an RFC 3339 timestamp filter, refusing a malformed one.
///
/// A silently-dropped time filter turns a narrow compliance query into an
/// unnoticed full read, so this fails loud rather than defaulting to "no bound".
fn parse_time(
    field: &str,
    raw: Option<&String>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
    raw.map(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| format!("Invalid {field} '{s}': {e}"))
    })
    .transpose()
}

/// A `400` carrying a machine-readable code and a human-readable message.
fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": "invalid_request", "message": message})),
    )
        .into_response()
}

// =============================================================================
// Error mapping
// =============================================================================

/// One HTTP mapping for every store error, used by every handler.
///
/// Before #769 each handler invented its own: `create_role` answered
/// `409 role_duplicate` for a malformed permission string *and* for an unreachable
/// database, so an operator debugging a dead database was told their role already
/// existed. Implementing `IntoResponse` on the error itself means a new handler
/// gets the right mapping by construction and a new variant is a compile error here.
impl IntoResponse for RbacDbError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::RoleNotFound => (StatusCode::NOT_FOUND, "role_not_found"),
            Self::PermissionNotFound => (StatusCode::NOT_FOUND, "permission_not_found"),
            Self::AssignmentNotFound => (StatusCode::NOT_FOUND, "assignment_not_found"),
            Self::RoleDuplicate => (StatusCode::CONFLICT, "role_duplicate"),
            Self::PermissionDuplicate => (StatusCode::CONFLICT, "permission_duplicate"),
            Self::AssignmentDuplicate => (StatusCode::CONFLICT, "assignment_duplicate"),
            Self::TenantMismatch => (StatusCode::BAD_REQUEST, "tenant_mismatch"),
            Self::PermissionInUse => (StatusCode::CONFLICT, "permission_in_use"),
            Self::ConnectionError(_) | Self::QueryError(_) | Self::TransactionError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            },
        };
        // Internal failures must not echo the database's message to an operator's
        // browser; the log is where the detail belongs.
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "RBAC management operation failed");
            "The RBAC store could not complete the operation".to_string()
        } else {
            self.to_string()
        };
        (status, Json(serde_json::json!({"error": code, "message": message}))).into_response()
    }
}

/// API state for role and permission management
#[derive(Clone)]
pub struct RbacManagementState {
    /// Database backend for RBAC operations
    pub db: Arc<db_backend::RbacDbBackend>,
}

/// Create RBAC management router
///
/// Routes:
/// - POST   /api/roles                           - Create role
/// - GET    /api/roles                           - List roles
/// - GET    `/api/roles/{role_id}`                 - Get role details
/// - PUT    `/api/roles/{role_id}`                 - Update role
/// - DELETE `/api/roles/{role_id}`                 - Delete role
/// - POST   /api/permissions                     - Create permission
/// - GET    /api/permissions                     - List permissions
/// - GET    `/api/permissions/{permission_id}`    - Get permission details
/// - DELETE `/api/permissions/{permission_id}`    - Delete permission
/// - POST   /api/user-roles                      - Assign role to user
/// - GET    /api/user-roles?user_id=…            - List a user's role assignments
/// - DELETE /api/user-roles/{user_id}/{role_id} - Revoke role from user
/// - GET    /api/audit/permissions               - Query recorded permission changes
///
/// Every list endpoint accepts `limit` (default 100, max 1000), `offset` and — where
/// the resource is tenant-scoped — `tenant_id`, and answers with a
/// [`Page`](db_backend::Page) carrying the unpaged `total` and a `has_more` flag.
/// Unknown query parameters are refused rather than ignored, so a mistyped
/// `tenant_id` cannot silently widen a read.
///
/// The router sits behind
/// [`admin_principal_middleware`](crate::middleware::admin_principal_middleware) (#1089). The
/// deployment administrator names the tenant on each request, as before; a tenant
/// administrator is confined to its own tenant (a request naming another is `403`, another
/// tenant's role or assignment is `404`), and may read but not change the permission
/// catalogue, which every tenant shares.
pub fn rbac_management_router(state: RbacManagementState) -> Router {
    Router::new()
        // Role endpoints
        .route("/api/roles", post(create_role).get(list_roles))
        .route("/api/roles/{role_id}", get(get_role).put(update_role).delete(delete_role))
        // Permission endpoints
        .route("/api/permissions", post(create_permission).get(list_permissions))
        .route(
            "/api/permissions/{permission_id}",
            get(get_permission).delete(delete_permission),
        )
        // User-role assignment endpoints
        .route("/api/user-roles", post(assign_role).get(list_user_roles))
        .route("/api/user-roles/{user_id}/{role_id}", delete(revoke_role))
        // Audit endpoints
        .route("/api/audit/permissions", get(query_permission_audit))
        .with_state(Arc::new(state))
}

// =============================================================================
// Tenant administrators (#1089)
// =============================================================================

/// Why a tenant administrator's request was refused before reaching the store.
enum Refusal {
    /// A malformed id, or a row the principal may not see (`404`, like a missing one).
    Rbac(RbacDbError),
    /// The request named a tenant the principal does not administer.
    ForeignTenant,
    /// A write to the permission catalogue, which only the platform changes.
    PlatformOnly,
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        match self {
            Self::Rbac(e) => e.into_response(),
            Self::ForeignTenant => foreign_tenant_response(),
            Self::PlatformOnly => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "platform_only",
                    "message": "the permission catalogue is shared by every tenant; only the \
                                deployment administrator changes it",
                })),
            )
                .into_response(),
        }
    }
}

/// The tenant a request may act on, as the string the backend takes.
///
/// A malformed tenant is a `400`; a tenant principal naming another tenant is a `403`; a
/// tenant principal naming none acts on its own.
fn scoped_tenant(principal: AdminPrincipal, raw: Option<&str>) -> Result<Option<String>, Refusal> {
    let requested = raw
        .map(|t| {
            Uuid::parse_str(t).map_err(|_| {
                Refusal::Rbac(RbacDbError::InvalidInput(format!(
                    "Invalid tenant ID: '{t}' is not a UUID"
                )))
            })
        })
        .transpose()?;
    principal
        .scope(requested)
        .map(|t| t.map(|u| u.to_string()))
        .map_err(|_| Refusal::ForeignTenant)
}

/// Whether `principal` may see a row whose stored tenant is `tenant` (`None` = global).
fn visible(principal: AdminPrincipal, tenant: Option<&str>) -> bool {
    match tenant {
        None => principal.may_see(None),
        // A stored tenant that is not a UUID is no tenant a principal administers.
        Some(t) => Uuid::parse_str(t).is_ok_and(|t| principal.may_see(Some(t))),
    }
}

/// Load a role the principal may see; another tenant's role is `RoleNotFound`, exactly
/// like a missing one.
async fn visible_role(
    state: &RbacManagementState,
    principal: AdminPrincipal,
    role_id: &str,
) -> Result<RoleDto, Refusal> {
    match state.db.get_role(role_id).await {
        Ok(role) if visible(principal, role.tenant_id.as_deref()) => Ok(role),
        Ok(_) => Err(Refusal::Rbac(RbacDbError::RoleNotFound)),
        Err(e) => Err(Refusal::Rbac(e)),
    }
}

/// The permission catalogue is global: only the platform writes it.
const fn platform_only(principal: AdminPrincipal) -> Result<(), Refusal> {
    if principal.is_platform() {
        Ok(())
    } else {
        Err(Refusal::PlatformOnly)
    }
}

// =============================================================================
// Role Management Endpoints
// =============================================================================

/// Create a new role
/// POST /api/roles
async fn create_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Json(payload): Json<CreateRoleRequest>,
) -> Response {
    let tenant = match scoped_tenant(principal, payload.tenant_id.as_deref()) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    match state
        .db
        .create_role(
            &payload.name,
            payload.description.as_deref(),
            payload.permissions,
            tenant.as_deref(),
        )
        .await
    {
        Ok(role) => (StatusCode::CREATED, Json(role)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// List roles
/// GET `/api/roles?tenant_id=…&limit=…&offset=…`
async fn list_roles(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(params): Query<ListQuery>,
) -> Response {
    let limit = match resolve_limit(params.limit) {
        Ok(limit) => limit,
        Err(message) => return bad_request(&message),
    };
    let tenant = match scoped_tenant(principal, params.tenant_id.as_deref()) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    match state.db.list_roles(tenant.as_deref(), limit, params.offset.unwrap_or(0)).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Get role details
/// GET `/api/roles/{role_id}`
async fn get_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(role_id): Path<String>,
) -> Response {
    match visible_role(&state, principal, &role_id).await {
        Ok(role) => (StatusCode::OK, Json(role)).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

/// Update role
/// PUT `/api/roles/{role_id}`
async fn update_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(role_id): Path<String>,
    Json(payload): Json<CreateRoleRequest>,
) -> Response {
    if let Err(refusal) = visible_role(&state, principal, &role_id).await {
        return refusal.into_response();
    }
    match state
        .db
        .update_role(&role_id, &payload.name, payload.description.as_deref(), payload.permissions)
        .await
    {
        Ok(role) => (StatusCode::OK, Json(role)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Delete role
/// DELETE `/api/roles/{role_id}`
async fn delete_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(role_id): Path<String>,
) -> Response {
    if let Err(refusal) = visible_role(&state, principal, &role_id).await {
        return refusal.into_response();
    }
    match state.db.delete_role(&role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

// =============================================================================
// Permission Management Endpoints
// =============================================================================

/// Create a new permission
/// POST /api/permissions
async fn create_permission(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Json(payload): Json<CreatePermissionRequest>,
) -> Response {
    if let Err(refusal) = platform_only(principal) {
        return refusal.into_response();
    }
    match state
        .db
        .create_permission(&payload.resource, &payload.action, payload.description.as_deref())
        .await
    {
        Ok(perm) => (StatusCode::CREATED, Json(perm)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// List permissions
/// GET `/api/permissions?limit=…&offset=…`
async fn list_permissions(
    State(state): State<Arc<RbacManagementState>>,
    Query(params): Query<ListQuery>,
) -> Response {
    let limit = match resolve_limit(params.limit) {
        Ok(limit) => limit,
        Err(message) => return bad_request(&message),
    };
    match state.db.list_permissions(limit, params.offset.unwrap_or(0)).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Get permission details
/// GET `/api/permissions/{permission_id}`
async fn get_permission(
    State(state): State<Arc<RbacManagementState>>,
    Path(permission_id): Path<String>,
) -> Response {
    match state.db.get_permission(&permission_id).await {
        Ok(perm) => (StatusCode::OK, Json(perm)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Delete permission
/// DELETE `/api/permissions/{permission_id}`
async fn delete_permission(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(permission_id): Path<String>,
) -> Response {
    if let Err(refusal) = platform_only(principal) {
        return refusal.into_response();
    }
    match state.db.delete_permission(&permission_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

// =============================================================================
// User-Role Assignment Endpoints
// =============================================================================

/// Assign a role to a user
/// POST /api/user-roles
async fn assign_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Json(payload): Json<AssignRoleRequest>,
) -> Response {
    let tenant = match scoped_tenant(principal, payload.tenant_id.as_deref()) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    if let Err(refusal) = visible_role(&state, principal, &payload.role_id).await {
        return refusal.into_response();
    }
    match state
        .db
        .assign_role_to_user(&payload.user_id, &payload.role_id, tenant.as_deref())
        .await
    {
        Ok(assignment) => (StatusCode::CREATED, Json(assignment)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// List a user's role assignments
/// GET `/api/user-roles?user_id=…&tenant_id=…&limit=…&offset=…`
///
/// `user_id` is required. It used to be optional, and omitting it answered
/// `200 []` — indistinguishable from "this user holds no roles" (#769).
async fn list_user_roles(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    params: Result<Query<UserRolesQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(params)) = params else {
        return bad_request("user_id is required: GET /api/user-roles?user_id=<subject>");
    };
    let limit = match resolve_limit(params.limit) {
        Ok(limit) => limit,
        Err(message) => return bad_request(&message),
    };
    let tenant = match scoped_tenant(principal, params.tenant_id.as_deref()) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    match state
        .db
        .list_user_roles(&params.user_id, tenant.as_deref(), limit, params.offset.unwrap_or(0))
        .await
    {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Revoke a role from a user
/// DELETE /api/user-roles/{user_id}/{role_id}
async fn revoke_role(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Path((user_id, role_id)): Path<(String, String)>,
) -> Response {
    // Another tenant's assignment is a 404, exactly like a missing one.
    match state.db.assignment_tenant(&user_id, &role_id).await {
        Ok(Some(tenant)) if principal.may_see(tenant) => {},
        Ok(_) => return RbacDbError::AssignmentNotFound.into_response(),
        Err(e) => return e.into_response(),
    }
    match state.db.revoke_role_from_user(&user_id, &role_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

// =============================================================================
// Audit Endpoints
// =============================================================================

/// Query recorded permission changes
/// GET `/api/audit/permissions?user_id=…&role_id=…&event_type=…&tenant_id=…&start_time=…&end_time=…
/// `
///
/// This used to be `Json(Vec::new())` — the parameters were not even extracted, and
/// no handler in this module recorded anything. A `200 []` under a compliance claim
/// is a positive assertion that no permission changes occurred, which is exactly the
/// answer a reviewer must not be given by accident (#768).
async fn query_permission_audit(
    State(state): State<Arc<RbacManagementState>>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(params): Query<AuditQuery>,
) -> Response {
    let tenant = match scoped_tenant(principal, params.tenant_id.as_deref()) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let limit = match resolve_limit(params.limit) {
        Ok(limit) => limit,
        Err(message) => return bad_request(&message),
    };
    let start_time = match parse_time("start_time", params.start_time.as_ref()) {
        Ok(t) => t,
        Err(message) => return bad_request(&message),
    };
    let end_time = match parse_time("end_time", params.end_time.as_ref()) {
        Ok(t) => t,
        Err(message) => return bad_request(&message),
    };

    let filter = AuditFilter {
        user_id: params.user_id.as_deref(),
        role_id: params.role_id.as_deref(),
        event_type: params.event_type.as_deref(),
        tenant_id: tenant.as_deref(),
        start_time,
        end_time,
        limit,
        offset: params.offset.unwrap_or(0),
    };

    match state.db.query_audit(&filter).await {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Database backend for RBAC operations
pub mod db_backend;

#[cfg(test)]
mod tests;
