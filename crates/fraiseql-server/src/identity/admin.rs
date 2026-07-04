//! Admin surface for the identity cache — `flush(sub)` / `flush_all`.
//!
//! Lets an operator propagate a revocation or a fresh provision **immediately**
//! rather than waiting out `cache_ttl_secs` (DESIGN §6, §6.1). Mounted under the
//! admin bearer token alongside the RBAC-management API, and only when an
//! enrichment resolver exists (`[identity.enrichment]` enabled).

use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use serde::Deserialize;

use super::resolver::IdentityResolver;

/// Body of `POST /api/identity/flush`.
#[derive(Debug, Deserialize)]
struct FlushRequest {
    /// The subject whose cached identity — across every bound-parameter tuple —
    /// is evicted.
    sub: String,
}

/// Build the identity-cache admin router over a resolver instance.
pub fn identity_admin_router(resolver: Arc<IdentityResolver>) -> Router {
    Router::new()
        .route("/api/identity/flush", post(flush_subject))
        .route("/api/identity/flush-all", post(flush_all))
        .with_state(resolver)
}

/// Evict every cache entry for one subject — propagates a revoke/provision now.
async fn flush_subject(
    State(resolver): State<Arc<IdentityResolver>>,
    Json(request): Json<FlushRequest>,
) -> impl IntoResponse {
    resolver.flush(&request.sub);
    (StatusCode::OK, Json(serde_json::json!({ "flushed": request.sub })))
}

/// Evict the entire identity cache.
async fn flush_all(State(resolver): State<Arc<IdentityResolver>>) -> impl IntoResponse {
    resolver.flush_all();
    (StatusCode::OK, Json(serde_json::json!({ "flushed_all": true })))
}
