//! API-key management endpoints (#627).
//!
//! Mounted behind the admin bearer token when `[security.api_keys] storage =
//! "postgres"` is configured. Operates on the **same**
//! [`PgApiKeyStore`](crate::api_key::postgres::PgApiKeyStore) the
//! authenticator reads, so management and authentication cannot disagree about
//! which keys exist.
//!
//! Routes:
//! - `GET    /api/v1/admin/api-keys`                      — list (metadata only)
//! - `POST   /api/v1/admin/api-keys`                      — create; returns the full key ONCE
//! - `POST   /api/v1/admin/api-keys/{selector}/revoke`    — revoke (idempotent)
//! - `POST   /api/v1/admin/api-keys/{selector}/rotate`    — replace the secret; returns the new
//!   full key ONCE

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::api_key::postgres::{ApiKeyStoreError, PgApiKeyStore};

/// Shared state: the store the authenticator also reads.
#[derive(Clone)]
pub struct ApiKeyManagementState {
    /// Postgres-backed key store.
    pub store: Arc<PgApiKeyStore>,
}

/// Build the management router.
pub fn api_key_management_router(state: ApiKeyManagementState) -> Router {
    Router::new()
        .route("/api/v1/admin/api-keys", get(list_keys).post(create_key))
        .route("/api/v1/admin/api-keys/{selector}/revoke", post(revoke_key))
        .route("/api/v1/admin/api-keys/{selector}/rotate", post(rotate_key))
        .with_state(state)
}

/// Map store errors onto HTTP statuses at one place, so a new handler cannot
/// conflate "caller's fault" with "database down" (the #769 lesson).
impl IntoResponse for ApiKeyStoreError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            Self::NotFound => (StatusCode::NOT_FOUND, "api_key_not_found"),
            // Database errors are the server's fault; do not leak the SQL text.
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
        };
        let message = match &self {
            Self::InvalidInput(m) => m.clone(),
            Self::NotFound => "API key not found".to_string(),
            _ => "database error".to_string(),
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}

/// Request body for key creation. Unknown fields are refused so a misspelled
/// `expires_in_secs` cannot silently create a never-expiring key.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateKeyRequest {
    name:            String,
    #[serde(default)]
    scopes:          Vec<String>,
    /// Optional TTL in seconds from now.
    #[serde(default)]
    expires_in_secs: Option<i64>,
}

async fn list_keys(State(state): State<ApiKeyManagementState>) -> Response {
    match state.store.list_keys().await {
        Ok(keys) => (StatusCode::OK, Json(json!({ "keys": keys }))).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn create_key(
    State(state): State<ApiKeyManagementState>,
    Json(req): Json<CreateKeyRequest>,
) -> Response {
    let expires_at: Option<DateTime<Utc>> = match req.expires_in_secs {
        Some(secs) if secs <= 0 => {
            return ApiKeyStoreError::InvalidInput("expires_in_secs must be positive".to_string())
                .into_response();
        },
        Some(secs) => Some(Utc::now() + Duration::seconds(secs)),
        None => None,
    };
    match state.store.create_key(&req.name, &req.scopes, expires_at).await {
        Ok((full_key, record)) => (
            StatusCode::CREATED,
            // The full key appears exactly once, here. Only the selector is
            // ever shown again.
            Json(json!({ "key": full_key, "record": record })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn revoke_key(
    State(state): State<ApiKeyManagementState>,
    Path(selector): Path<String>,
) -> Response {
    match state.store.revoke(&selector).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "revoked": selector }))).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn rotate_key(
    State(state): State<ApiKeyManagementState>,
    Path(selector): Path<String>,
) -> Response {
    match state.store.rotate(&selector).await {
        Ok(full_key) => {
            (StatusCode::OK, Json(json!({ "key": full_key, "selector": selector }))).into_response()
        },
        Err(e) => e.into_response(),
    }
}

#[cfg(test)]
#[path = "api_key_management_tests.rs"]
mod api_key_management_tests;
