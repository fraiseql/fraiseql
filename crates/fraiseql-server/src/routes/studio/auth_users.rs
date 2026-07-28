//! Auth user management endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/users/*` expose user listing, invitation,
//! session revocation, and MFA status. All routes are protected by
//! the admin bearer token middleware.

use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::{graphql::app_state::AppState, studio::not_implemented};

// ---------------------------------------------------------------------------
// User record
// ---------------------------------------------------------------------------

/// A single user record in the admin user list.
///
/// Agreed response shape with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser {
    /// OIDC subject identifier.
    pub sub:          String,
    /// User email address.
    pub email:        String,
    /// Identity provider (e.g. `"google"`, `"email"`, `"github"`).
    pub provider:     String,
    /// Account creation timestamp (RFC 3339).
    pub created_at:   String,
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
    pub users:     Vec<AdminUser>,
    /// Total user count across all pages.
    pub total:     u64,
    /// Current page number (1-indexed).
    pub page:      u32,
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
/// Answers `501`: FraiseQL has no user directory of its own — identities come from
/// the configured identity provider — so there is nothing to enumerate. This used to answer
/// `{"users": [], "total": 0}`, which reads as "this deployment has no users".
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `501` — see above.
pub async fn list_users_handler<A>(State(_state): State<AppState<A>>) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.users.list",
        "FraiseQL does not maintain a user directory; identities are owned by the \
         configured identity provider. Enumerate users there.",
    )
}

/// `POST /admin/v1/users/invite` — send magic-link invitation.
///
/// Answers `501`: there is no invitation subsystem. It used to answer
/// `{"success": true, "message": "Invitation queued for …"}` with nothing queued.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `501` — see above.
pub async fn invite_user_handler<A>(
    State(_state): State<AppState<A>>,
    Json(_req): Json<UserInviteRequest>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.users.invite",
        "No invitation subsystem is wired: FraiseQL does not send magic links. Invite \
         users through the configured identity provider.",
    )
}

/// `POST /admin/v1/users/{id}/revoke` — revoke all of a user's active sessions.
///
/// This is the sharpest case in #749: an operator responding to an account
/// compromise received `{"success": true, "message": "All sessions revoked"}` while
/// no revocation store was touched and the attacker's tokens kept validating. The
/// server has always had a real [`TokenRevocationManager`]; this handler simply
/// never called it.
///
/// When revocation is not configured the answer is `501`, not success — a deployment
/// with no revocation store genuinely cannot revoke anything, and saying so is the
/// only useful response.
///
/// [`TokenRevocationManager`]: crate::token_revocation::TokenRevocationManager
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `501` when no revocation store is configured.
/// Returns `500` when the revocation store rejects the write.
pub async fn revoke_user_handler<A>(
    Path(user_id): Path<String>,
    State(state): State<AppState<A>>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let Some(manager) = state.revocation_manager.as_ref() else {
        return not_implemented(
            "studio.users.revoke",
            "Token revocation is not configured, so no session can be revoked. Enable \
             [security.token_revocation] in fraiseql.toml.",
        );
    };

    match manager.revoke_all_for_user(&user_id).await {
        Ok(()) => Json(serde_json::json!({
            "success": true,
            "user_id": user_id,
            "message": "All sessions revoked",
        }))
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, user = %user_id, "revoke-all failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error":   "revocation_failed",
                    "message": "The revocation store rejected the write; sessions are NOT revoked.",
                })),
            )
                .into_response()
        },
    }
}

/// `GET /admin/v1/users/{id}/mfa` — MFA enrollment details.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `501` — MFA enrollment state is not exposed through the admin API.
pub async fn mfa_status_handler<A>(
    Path(_user_id): Path<String>,
    State(_state): State<AppState<A>>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    not_implemented(
        "studio.users.mfa",
        "MFA enrollment state is not exposed through the admin API.",
    )
}
