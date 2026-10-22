//! Session revocation for the Studio dashboard.
//!
//! `POST /admin/v1/users/{id}/revoke` revokes every active session of a user
//! through the server's token revocation store. It is protected by the admin
//! bearer token middleware.

use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse, Response},
};

use crate::routes::{graphql::app_state::AppState, studio::not_implemented};

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
pub async fn revoke_user_handler(
    Path(user_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
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
