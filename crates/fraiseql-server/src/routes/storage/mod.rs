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
    pub backend:          Arc<dyn StorageBackend>,
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
    pub tenant_prefix:    Option<String>,
}

impl StorageRouteState {
    /// Create state with the given backend and the default 100 `MiB` upload limit.
    #[must_use]
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend, max_upload_bytes: DEFAULT_MAX_UPLOAD_BYTES, tenant_prefix: None }
    }

    /// Override the maximum upload size.
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
    url:        String,
    /// How long the URL remains valid, in seconds.
    expires_in: u64,
}

/// Body returned for all error responses.
#[derive(Serialize)]
struct ErrorBody {
    /// Human-readable error message.
    error: String,
    /// Stable machine-readable error code.
    code:  &'static str,
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
        code:  err.error_code(),
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
            max:  state.max_upload_bytes,
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
        Ok(data) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, "application/octet-stream")], data)
                .into_response()
        },
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
            axum::Json(PresignedUrlResponse { url, expires_in: params.expiry_secs }),
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
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::storage::LocalStorageBackend;

    /// Build a test router backed by a local filesystem backend in a temp dir.
    fn make_test_router() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend);
        let router = storage_router(state);
        (router, dir)
    }

    // ── upload ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn upload_returns_200_with_key() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/hello.txt")
            .header("content-type", "text/plain")
            .body(Body::from("hello world"))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["key"], "hello.txt");
    }

    #[tokio::test]
    async fn upload_nested_key_creates_directories() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/a/b/c/deep.txt")
            .body(Body::from("deep"))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn upload_rejects_path_traversal() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/..%2Fescape.txt")
            .body(Body::from("bad"))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        // validate_key rejects ".." → Storage error → 500
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn upload_enforces_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend).with_max_upload_bytes(10);
        let router = storage_router(state);

        let req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/big.bin")
            .body(Body::from(b"x".repeat(11).as_slice().to_owned()))
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "file_too_large");
    }

    // ── download ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn upload_then_download_round_trip() {
        let (router, _dir) = make_test_router();

        // Upload
        let upload_req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/greet.txt")
            .header("content-type", "text/plain")
            .body(Body::from("hello storage"))
            .unwrap();
        router.clone().oneshot(upload_req).await.unwrap();

        // Download
        let download_req = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/greet.txt")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(download_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert_eq!(content_type, "application/octet-stream");

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"hello storage");
    }

    #[tokio::test]
    async fn download_missing_file_returns_404() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/nonexistent.txt")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "file_not_found");
    }

    // ── delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_existing_file_returns_204() {
        let (router, _dir) = make_test_router();

        // Upload first
        let upload_req = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/todelete.txt")
            .body(Body::from("bye"))
            .unwrap();
        router.clone().oneshot(upload_req).await.unwrap();

        // Delete
        let delete_req = Request::builder()
            .method(Method::DELETE)
            .uri("/storage/v1/object/todelete.txt")
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(delete_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify gone
        let get_req = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/todelete.txt")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(get_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_missing_file_returns_404() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/storage/v1/object/ghost.txt")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── presigned URL ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn presigned_url_not_supported_by_local_backend() {
        let (router, _dir) = make_test_router();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/sign/file.txt?expiry_secs=300")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        // Local backend does not support presigned URLs → 500 with code
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "file_storage_error");
    }

    // ── state ─────────────────────────────────────────────────────────────────

    #[test]
    fn state_default_max_upload_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend);
        assert_eq!(state.max_upload_bytes, DEFAULT_MAX_UPLOAD_BYTES);
    }

    #[test]
    fn state_with_max_upload_bytes_overrides_default() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend).with_max_upload_bytes(512);
        assert_eq!(state.max_upload_bytes, 512);
    }

    // ── tenant isolation ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn tenant_prefix_isolates_keys() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));

        // Tenant A router
        let state_a = StorageRouteState::new(backend.clone())
            .with_tenant_prefix("tenant-a");
        let router_a = storage_router(state_a);

        // Tenant B router
        let state_b = StorageRouteState::new(backend.clone())
            .with_tenant_prefix("tenant-b");
        let router_b = storage_router(state_b);

        // Tenant A uploads shared.txt
        let upload = Request::builder()
            .method(Method::POST)
            .uri("/storage/v1/object/shared.txt")
            .body(Body::from("tenant-a content"))
            .unwrap();
        let resp = router_a.clone().oneshot(upload).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Tenant B cannot see tenant A's file (different prefixed key)
        let download = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/shared.txt")
            .body(Body::empty())
            .unwrap();
        let resp = router_b.oneshot(download).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // Tenant A can read its own file
        let download = Request::builder()
            .method(Method::GET)
            .uri("/storage/v1/object/shared.txt")
            .body(Body::empty())
            .unwrap();
        let resp = router_a.oneshot(download).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn state_no_prefix_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend);
        assert!(state.tenant_prefix.is_none());
    }

    #[test]
    fn state_with_tenant_prefix_sets_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
        let state = StorageRouteState::new(backend).with_tenant_prefix("myorg");
        assert_eq!(state.tenant_prefix.as_deref(), Some("myorg"));
    }
}
