//! Admin surface for the suppression list — append + query.
//!
//! The correlation step auto-suppresses on hard bounces and repeated challenges;
//! this router is the **operator** surface for the manual cases: a support removal,
//! a GDPR do-not-contact request, an unsubscribe processed out of band. Mounted
//! behind the same admin bearer gate as the RBAC / identity-cache APIs.
//!
//! The address is hashed **server-side** with the recipient address-hash key (the
//! server HMAC subkey) before it touches the store — the suppression list holds no
//! raw address, so a query or an append both go through here rather than direct
//! SQL, which could not compute the same keyed hash. Read of *send* status (keyed
//! by the non-secret send-id) needs no hashing and can be a direct RLS-scoped query
//! against `_fraiseql_send_status`.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;

use super::tracking::{PgSendTracker, SendCorrelator, SendTracker, SuppressionReason};

/// Shared state for the suppression admin router.
pub struct SuppressionAdminState {
    /// The delivery-feedback store (append via `SendCorrelator::suppress`, query
    /// via `SendTracker::suppression_reason`).
    tracker:          Arc<PgSendTracker>,
    /// The recipient address-hash key — the server HMAC subkey used to hash an
    /// address before it touches the store.
    address_hash_key: Arc<[u8]>,
}

impl SuppressionAdminState {
    /// Assemble the router state from the store and the address-hash key.
    #[must_use]
    pub const fn new(tracker: Arc<PgSendTracker>, address_hash_key: Arc<[u8]>) -> Self {
        Self {
            tracker,
            address_hash_key,
        }
    }
}

/// Build the suppression admin router.
///
/// - `POST /api/email/suppress` — add/refresh a suppression for an address.
/// - `GET /api/email/suppression?address=…[&tenant=…]` — whether an address is suppressed (and
///   why).
pub fn suppression_admin_router(state: Arc<SuppressionAdminState>) -> Router {
    Router::new()
        .route("/api/email/suppress", post(suppress))
        .route("/api/email/suppression", get(query))
        .with_state(state)
}

/// Body of `POST /api/email/suppress`.
#[derive(Debug, Deserialize)]
struct SuppressRequest {
    /// The recipient address to suppress (hashed server-side; never stored raw).
    address: String,
    /// The suppression reason (`hard_bounce` / `challenge_unanswered` /
    /// `unsubscribe`). An operator append is usually `unsubscribe`.
    reason:  String,
    /// Optional tenant to scope the suppression to.
    #[serde(default)]
    tenant:  Option<String>,
}

/// Query of `GET /api/email/suppression`.
#[derive(Debug, Deserialize)]
struct SuppressionQuery {
    /// The recipient address to check.
    address: String,
    /// Optional tenant scope.
    #[serde(default)]
    tenant:  Option<String>,
}

/// Append (or refresh) a suppression for an address.
async fn suppress(
    State(state): State<Arc<SuppressionAdminState>>,
    Json(request): Json<SuppressRequest>,
) -> impl IntoResponse {
    let Some(reason) = SuppressionReason::parse(&request.reason) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("unknown suppression reason {:?}", request.reason)
            })),
        );
    };
    let hash = fraiseql_observers::hash_address(&state.address_hash_key, &request.address);
    let ttl = reason.default_ttl(chrono::Utc::now());
    match state.tracker.suppress(request.tenant.as_deref(), &hash, reason, ttl).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "suppressed": true, "reason": reason.as_str() })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

/// Whether an address is currently suppressed (and why).
async fn query(
    State(state): State<Arc<SuppressionAdminState>>,
    Query(params): Query<SuppressionQuery>,
) -> impl IntoResponse {
    let hash = fraiseql_observers::hash_address(&state.address_hash_key, &params.address);
    match state.tracker.suppression_reason(params.tenant.as_deref(), &hash).await {
        Ok(reason) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "suppressed": reason.is_some(),
                "reason": reason.map(SuppressionReason::as_str),
            })),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        ),
    }
}

#[cfg(test)]
mod tests;
