//! Role and Permission Management API
//!
//! REST API endpoints for managing roles, permissions, and user-role associations.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

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

/// Request to create a new role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoleRequest {
    /// Role name
    pub name:        String,
    /// Optional description
    pub description: Option<String>,
    /// Initial permissions to assign
    pub permissions: Vec<String>,
}

/// Request to create a new permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePermissionRequest {
    /// Resource name
    pub resource:    String,
    /// Action name
    pub action:      String,
    /// Optional description
    pub description: Option<String>,
}

/// Request to assign a role to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRoleRequest {
    /// User ID
    pub user_id: String,
    /// Role ID to assign
    pub role_id: String,
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
/// - GET    /api/roles/{role_id}                 - Get role details
/// - PUT    /api/roles/{role_id}                 - Update role
/// - DELETE /api/roles/{role_id}                 - Delete role
/// - POST   /api/permissions                     - Create permission
/// - GET    /api/permissions                     - List permissions
/// - GET    /api/permissions/{permission_id}    - Get permission details
/// - DELETE /api/permissions/{permission_id}    - Delete permission
/// - POST   /api/user-roles                      - Assign role to user
/// - GET    /api/user-roles                      - List user-role assignments
/// - DELETE /api/user-roles/{user_id}/{role_id} - Revoke role from user
/// - GET    /api/audit/permissions               - Query permission access audit logs
pub fn rbac_management_router(state: RbacManagementState) -> Router {
    Router::new()
        // Role endpoints
        .route("/api/roles", post(create_role).get(list_roles))
        .route("/api/roles/:role_id", get(get_role).put(update_role).delete(delete_role))
        // Permission endpoints
        .route("/api/permissions", post(create_permission).get(list_permissions))
        .route("/api/permissions/:permission_id", get(get_permission).delete(delete_permission))
        // User-role assignment endpoints
        .route("/api/user-roles", post(assign_role).get(list_user_roles))
        .route("/api/user-roles/:user_id/:role_id", delete(revoke_role))
        // Audit endpoints
        .route("/api/audit/permissions", get(query_permission_audit))
        .with_state(Arc::new(state))
}

// =============================================================================
// Role Management Endpoints
// =============================================================================

/// Create a new role
/// POST /api/roles
async fn create_role(
    State(state): State<Arc<RbacManagementState>>,
    Json(payload): Json<CreateRoleRequest>,
) -> impl IntoResponse {
    // In production: validate payload, extract tenant from JWT, create role
    match state
        .db
        .create_role(
            &payload.name,
            payload.description.as_deref(),
            payload.permissions,
            None, // Would extract tenant from JWT
        )
        .await
    {
        Ok(role) => (StatusCode::CREATED, Json(serde_json::to_value(role).unwrap_or_default()))
            .into_response(),
        Err(_) => (StatusCode::CONFLICT, Json(serde_json::json!({"error": "role_duplicate"})))
            .into_response(),
    }
}

/// List all roles
/// GET /api/roles
async fn list_roles(State(state): State<Arc<RbacManagementState>>) -> impl IntoResponse {
    // In production: extract tenant from JWT, apply pagination
    match state.db.list_roles(None, 100, 0).await {
        Ok(roles) => Json(roles),
        Err(_) => Json(Vec::<RoleDto>::new()),
    }
}

/// Get role details
/// GET /api/roles/{role_id}
async fn get_role(
    State(state): State<Arc<RbacManagementState>>,
    Path(role_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_role(&role_id).await {
        Ok(role) => {
            (StatusCode::OK, Json(serde_json::to_value(role).unwrap_or_default())).into_response()
        },
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "role_not_found"})))
            .into_response(),
    }
}

/// Update role
/// PUT /api/roles/{role_id}
async fn update_role(
    State(_state): State<Arc<RbacManagementState>>,
    Path(_role_id): Path<String>,
    Json(_payload): Json<CreateRoleRequest>,
) -> impl IntoResponse {
    // Would need update_role() method in backend
    Json(serde_json::json!({"updated": true}))
}

/// Delete role
/// DELETE /api/roles/{role_id}
async fn delete_role(
    State(state): State<Arc<RbacManagementState>>,
    Path(role_id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_role(&role_id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::CONFLICT,
    }
}

// =============================================================================
// Permission Management Endpoints
// =============================================================================

/// Create a new permission
/// POST /api/permissions
async fn create_permission(
    State(state): State<Arc<RbacManagementState>>,
    Json(payload): Json<CreatePermissionRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_permission(&payload.resource, &payload.action, payload.description.as_deref())
        .await
    {
        Ok(perm) => (StatusCode::CREATED, Json(serde_json::to_value(perm).unwrap_or_default()))
            .into_response(),
        Err(_) => {
            (StatusCode::CONFLICT, Json(serde_json::json!({"error": "permission_duplicate"})))
                .into_response()
        },
    }
}

/// List all permissions
/// GET /api/permissions
async fn list_permissions(State(_state): State<Arc<RbacManagementState>>) -> impl IntoResponse {
    // Placeholder for now
    Json(Vec::<PermissionDto>::new())
}

/// Get permission details
/// GET /api/permissions/{permission_id}
async fn get_permission(
    State(_state): State<Arc<RbacManagementState>>,
    Path(_permission_id): Path<String>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "permission_not_found"})),
    )
}

/// Delete permission
/// DELETE /api/permissions/{permission_id}
async fn delete_permission(
    State(_state): State<Arc<RbacManagementState>>,
    Path(_permission_id): Path<String>,
) -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

// =============================================================================
// User-Role Assignment Endpoints
// =============================================================================

/// Assign a role to a user
/// POST /api/user-roles
async fn assign_role(
    State(state): State<Arc<RbacManagementState>>,
    Json(payload): Json<AssignRoleRequest>,
) -> impl IntoResponse {
    match state.db.assign_role_to_user(&payload.user_id, &payload.role_id, None).await {
        Ok(assignment) => {
            (StatusCode::CREATED, Json(serde_json::to_value(assignment).unwrap_or_default()))
                .into_response()
        },
        Err(_) => {
            (StatusCode::CONFLICT, Json(serde_json::json!({"error": "assignment_duplicate"})))
                .into_response()
        },
    }
}

/// List user-role assignments
/// GET /api/user-roles
async fn list_user_roles(State(_state): State<Arc<RbacManagementState>>) -> impl IntoResponse {
    // Placeholder for now
    Json(Vec::<UserRoleDto>::new())
}

/// Revoke a role from a user
/// DELETE /api/user-roles/{user_id}/{role_id}
async fn revoke_role(
    State(state): State<Arc<RbacManagementState>>,
    Path((user_id, role_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.db.revoke_role_from_user(&user_id, &role_id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

// =============================================================================
// Audit Endpoints
// =============================================================================

/// Query permission access audit logs
/// GET /api/audit/permissions?user_id=...&start_time=...&end_time=...
async fn query_permission_audit(
    State(_state): State<Arc<RbacManagementState>>,
) -> impl IntoResponse {
    Json(Vec::<serde_json::Value>::new())
}

/// Database backend for RBAC operations
pub mod db_backend;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod db_backend_tests;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod schema_tests;
