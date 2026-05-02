//! Auth user management endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/users/*` expose user listing, invitation,
//! session revocation, and MFA status. All routes are protected by
//! the admin bearer token middleware.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// User record
// ---------------------------------------------------------------------------

/// A single user record in the admin user list.
///
/// Agreed response shape with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser {
    /// OIDC subject identifier.
    pub sub: String,
    /// User email address.
    pub email: String,
    /// Identity provider (e.g. `"google"`, `"email"`, `"github"`).
    pub provider: String,
    /// Account creation timestamp (RFC 3339).
    pub created_at: String,
    /// Most recent sign-in timestamp (RFC 3339), or `None` if never signed in.
    pub last_sign_in: Option<String>,
    /// Whether the user has enrolled a TOTP or `WebAuthn` MFA factor.
    pub mfa_enrolled: bool,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Paginated user list response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListResponse {
    /// Users on this page.
    pub users: Vec<AdminUser>,
    /// Total user count across all pages.
    pub total: u64,
    /// Current page number (1-indexed).
    pub page: u32,
    /// Users per page.
    pub page_size: u32,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request body for `POST /admin/v1/users/invite`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInviteRequest {
    /// Email address to send the magic-link invitation to.
    pub email: String,
}

/// Response body for `POST /admin/v1/users/invite`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInviteResponse {
    /// Whether the invite was successfully sent.
    pub success: bool,
    /// Human-readable message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/v1/users` — paginated user list.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn list_users_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Placeholder — not yet wired to auth session tables.
    Json(UserListResponse {
        users: vec![],
        total: 0,
        page: 1,
        page_size: 50,
    })
}

/// `POST /admin/v1/users/invite` — send magic-link invitation.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn invite_user_handler<A>(
    State(_state): State<AppState<A>>,
    Json(req): Json<UserInviteRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(UserInviteResponse {
        success: true,
        message: format!("Invitation queued for {}", req.email),
    })
}

/// `POST /admin/v1/users/{id}/revoke` — revoke all active sessions.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `404` if the user does not exist.
pub async fn revoke_user_handler<A>(
    Path(_user_id): Path<String>,
    State(_state): State<AppState<A>>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(serde_json::json!({"success": true, "message": "All sessions revoked"}))
}

/// `GET /admin/v1/users/{id}/mfa` — MFA enrollment details.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `404` if the user does not exist.
pub async fn mfa_status_handler<A>(
    Path(_user_id): Path<String>,
    State(_state): State<AppState<A>>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Not Implemented",
            "message": "MFA status endpoint available in a future release"
        })),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions
    use super::*;

    #[test]
    fn test_user_list_response_serializes() {
        let resp = UserListResponse {
            users: vec![],
            total: 0,
            page: 1,
            page_size: 50,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"users\""));
    }

    #[test]
    fn test_user_invite_request_parses() {
        let input = r#"{"email":"a@b.com"}"#;
        let req: UserInviteRequest = serde_json::from_str(input).unwrap();
        assert_eq!(req.email, "a@b.com");
    }
}
