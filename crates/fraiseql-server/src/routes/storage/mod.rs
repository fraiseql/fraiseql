//! Storage REST API routes.
//!
//! Provides object storage endpoints mounted at `/storage/v1/`:
//!
//! | Method | Path | Operation |
//! |--------|------|-----------|
//! | `POST`   | `/storage/v1/object/*key` | Upload object |
//! | `GET`    | `/storage/v1/object/*key` | Download object |
//! | `DELETE` | `/storage/v1/object/*key` | Delete object |
//! | `GET`    | `/storage/v1/object/sign/*key` | Generate presigned URL |
//!
//! Routes are only mounted when a storage backend has been attached via
//! [`Server::with_storage`](crate::server::Server::with_storage).

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use fraiseql_error::FileError;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::storage::{StorageBackend, validate_key};

/// Default maximum upload size: 100 `MiB`.
pub const DEFAULT_MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

/// Shared state for all storage route handlers.
#[derive(Clone)]
pub struct StorageRouteState {
    /// The configured storage backend (local, S3, GCS, Azure, …).
    pub backend: Arc<dyn StorageBackend>,
    /// Maximum allowed upload body size in bytes.
    ///
    /// Requests that exceed this limit are rejected with HTTP 413 before the
    /// body is forwarded to the backend, preventing memory exhaustion on the
    /// server when large files are sent.
    pub max_upload_bytes: usize,
    /// Optional key prefix prepended to every storage key.
    ///
    /// Used for per-tenant isolation: set this to the tenant's ID so that
    /// tenant A's keys (`"tenantA/file.txt"`) are disjoint from tenant B's
    /// (`"tenantB/file.txt"`).  When `None`, keys are used as-is.
    pub tenant_prefix: Option<String>,
}

impl StorageRouteState {
    /// Create state with the given backend and the default 100 `MiB` upload limit.
    #[must_use]
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            backend,
            max_upload_bytes: DEFAULT_MAX_UPLOAD_BYTES,
            tenant_prefix: None,
        }
    }

    /// Override the maximum upload size.
    #[must_use]
    pub const fn with_max_upload_bytes(mut self, bytes: usize) -> Self {
        self.max_upload_bytes = bytes;
        self
    }

    /// Set a tenant key prefix for per-tenant object isolation.
    ///
    /// Every storage key is prefixed with `{prefix}/` before being forwarded
    /// to the backend, ensuring that tenants cannot access each other's objects
    /// even if they share the same bucket.
    #[must_use]
    pub fn with_tenant_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.tenant_prefix = Some(prefix.into());
        self
    }
}

// ── Response types ────────────────────────────────────────────────────────────

/// Body returned by a successful upload.
#[derive(Serialize)]
struct UploadResponse {
    /// The key under which the object was stored.
    key: String,
}

/// Body returned by a successful presigned-URL request.
#[derive(Serialize)]
struct PresignedUrlResponse {
    /// Time-limited URL that grants direct access to the object.
    url: String,
    /// How long the URL remains valid, in seconds.
    expires_in: u64,
}

/// Body returned for all error responses.
#[derive(Serialize)]
struct ErrorBody {
    /// Human-readable error message.
    error: String,
    /// Stable machine-readable error code.
    code: &'static str,
}

// ── Error mapping ─────────────────────────────────────────────────────────────

/// Convert a [`FileError`] into an HTTP error response.
fn file_error_response(err: &FileError) -> Response {
    let status = match err {
        FileError::NotFound { .. } => StatusCode::NOT_FOUND,
        FileError::TooLarge { .. } | FileError::QuotaExceeded => StatusCode::PAYLOAD_TOO_LARGE,
        FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        },
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = serde_json::to_string(&ErrorBody {
        error: err.to_string(),
        code: err.error_code(),
    })
    .unwrap_or_default();
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

// ── Key helpers ───────────────────────────────────────────────────────────────

/// Combine an optional tenant prefix with a raw key.
///
/// When `prefix` is `Some("tenantA")` and `key` is `"file.txt"`, the result
/// is `"tenantA/file.txt"`.  When `prefix` is `None`, the key is returned
/// unchanged.
fn prefixed_key(prefix: Option<&str>, key: &str) -> String {
    match prefix {
        Some(p) => format!("{p}/{key}"),
        None => key.to_owned(),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /storage/v1/object/*key` — upload an object.
///
/// Reads the entire request body and stores it at `key` in the configured
/// backend. Rejects bodies larger than [`StorageRouteState::max_upload_bytes`]
/// with HTTP 413.
///
/// The `Content-Type` header is forwarded to the backend and stored as the
/// object's MIME type (falls back to `application/octet-stream` when absent).
pub async fn upload_handler(
    State(state): State<StorageRouteState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(e) = validate_key(&key) {
        return file_error_response(&e);
    }

    if body.len() > state.max_upload_bytes {
        return file_error_response(&FileError::TooLarge {
            size: body.len(),
            max: state.max_upload_bytes,
        });
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let full_key = prefixed_key(state.tenant_prefix.as_deref(), &key);

    match state.backend.upload(&full_key, &body, &content_type).await {
        Ok(stored_key) => {
            (StatusCode::OK, axum::Json(UploadResponse { key: stored_key })).into_response()
        },
        Err(e) => file_error_response(&e),
    }
}

/// `GET /storage/v1/object/*key` — download an object.
///
/// Returns the object bytes with `Content-Type: application/octet-stream`.
pub async fn download_handler(
    State(state): State<StorageRouteState>,
    Path(key): Path<String>,
) -> Response {
    if let Err(e) = validate_key(&key) {
        return file_error_response(&e);
    }

    let full_key = prefixed_key(state.tenant_prefix.as_deref(), &key);

    match state.backend.download(&full_key).await {
        Ok(data) => (StatusCode::OK, [(header::CONTENT_TYPE, "application/octet-stream")], data)
            .into_response(),
        Err(e) => file_error_response(&e),
    }
}

/// `DELETE /storage/v1/object/*key` — delete an object.
///
/// Returns HTTP 204 on success.
pub async fn delete_handler(
    State(state): State<StorageRouteState>,
    Path(key): Path<String>,
) -> Response {
    if let Err(e) = validate_key(&key) {
        return file_error_response(&e);
    }

    let full_key = prefixed_key(state.tenant_prefix.as_deref(), &key);

    match state.backend.delete(&full_key).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => file_error_response(&e),
    }
}

/// Query parameters for the presigned-URL endpoint.
#[derive(Deserialize)]
pub struct SignQuery {
    /// URL expiry in seconds (default: 3 600 s = 1 hour).
    #[serde(default = "default_expiry_secs")]
    expiry_secs: u64,
}

const fn default_expiry_secs() -> u64 {
    3600
}

/// `GET /storage/v1/object/sign/*key` — generate a presigned URL.
///
/// Returns a time-limited URL granting direct access to the object without
/// requiring credentials.  Not all backends support presigned URLs; those that
/// do not return HTTP 500 with `code: "file_storage_error"`.
pub async fn presigned_url_handler(
    State(state): State<StorageRouteState>,
    Path(key): Path<String>,
    Query(params): Query<SignQuery>,
) -> Response {
    if let Err(e) = validate_key(&key) {
        return file_error_response(&e);
    }

    let expiry = Duration::from_secs(params.expiry_secs);
    let full_key = prefixed_key(state.tenant_prefix.as_deref(), &key);

    match state.backend.presigned_url(&full_key, expiry).await {
        Ok(url) => (
            StatusCode::OK,
            axum::Json(PresignedUrlResponse {
                url,
                expires_in: params.expiry_secs,
            }),
        )
            .into_response(),
        Err(e) => {
            warn!(key = %key, error = %e, "Presigned URL generation failed");
            file_error_response(&e)
        },
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the storage sub-router and attach `state` to all routes.
///
/// Register this router with [`Router::merge`] after the main application
/// router is built.  The routes use the `/storage/v1/` prefix.
///
/// **Route registration order matters for axum wildcard matching**: the sign
/// route (`/sign/{*key}`) is registered before the generic object route
/// (`/{*key}`) so that axum's static-segment-wins rule resolves correctly.
pub fn storage_router(state: StorageRouteState) -> Router {
    Router::new()
        // Sign route must come before the generic wildcard route.
        .route("/storage/v1/object/sign/{*key}", get(presigned_url_handler))
        .route(
            "/storage/v1/object/{*key}",
            post(upload_handler).get(download_handler).delete(delete_handler),
        )
        .with_state(state)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
