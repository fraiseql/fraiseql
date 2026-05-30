//! HTTP route handlers for storage operations.
//!
//! Provides a complete `axum::Router` for object storage:
//! - `PUT /storage/v1/object/{bucket}/{*key}` — upload
//! - `GET /storage/v1/object/{bucket}/{*key}` — download
//! - `DELETE /storage/v1/object/{bucket}/{*key}` — delete
//! - `GET /storage/v1/list/{bucket}` — list
//! - `POST /storage/v1/presign/{bucket}/{*key}` — presigned URL
//! - `GET /storage/v1/render/{bucket}/{*key}` — image transform

#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Arc};

use axum::{
    Extension, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bytes::Bytes;
use fraiseql_error::{FileError, FraiseQLError};
use serde::{Deserialize, Serialize};

#[cfg(feature = "aws-s3")]
use crate::PresignedUrl;
use crate::{
    backend::StorageBackend,
    config::BucketConfig,
    metadata::{NewStorageObject, StorageMetadataRepo, StorageMetadataRow},
    rls::StorageRlsEvaluator,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for all storage route handlers.
#[derive(Clone)]
pub struct StorageState {
    /// Storage backend (shared across all buckets).
    pub backend:  Arc<StorageBackend>,
    /// Metadata repository for object tracking.
    pub metadata: Arc<StorageMetadataRepo>,
    /// RLS evaluator for access control.
    pub rls:      StorageRlsEvaluator,
    /// Bucket configurations keyed by bucket name.
    pub buckets:  Arc<HashMap<String, BucketConfig>>,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for presigned URL generation.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Operation: "upload" (PUT) or "download" (GET).
    pub operation:       String,
    /// MIME type (required for uploads, optional for downloads).
    #[serde(default)]
    pub content_type:    Option<String>,
    /// URL validity duration in seconds (default: 3600, max: 86400).
    #[serde(default = "default_expiry_secs")]
    pub expires_in_secs: u64,
}

const fn default_expiry_secs() -> u64 {
    3600
}

/// Response body for presigned URL generation.
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    /// The presigned URL.
    pub url:        String,
    /// When the URL expires (RFC3339 format).
    pub expires_at: String,
    /// HTTP method this URL is valid for.
    pub method:     String,
}

#[cfg(feature = "aws-s3")]
impl From<PresignedUrl> for PresignResponse {
    fn from(url: PresignedUrl) -> Self {
        Self {
            url:        url.url,
            expires_at: url.expires_at.to_rfc3339(),
            method:     url.method,
        }
    }
}

/// Query parameters for list endpoint.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Filter by key prefix.
    pub prefix: Option<String>,
    /// Maximum results (default: 100, max: 1000).
    pub limit:  Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}

/// User identity extracted from request (populated by auth middleware).
#[derive(Debug, Clone, Default)]
pub struct StorageUser {
    /// User identifier (sub claim from JWT).
    pub user_id: Option<String>,
    /// User roles.
    pub roles:   Vec<String>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the storage HTTP router.
///
/// Returns an `axum::Router` that handles all storage endpoints.
/// The caller is responsible for applying authentication middleware
/// that populates `StorageUser` in request extensions.
pub fn storage_router(state: StorageState) -> Router {
    Router::new()
        .route(
            "/storage/v1/object/{bucket}/{*key}",
            put(put_handler).get(get_handler).delete(delete_handler),
        )
        .route("/storage/v1/list/{bucket}", get(list_handler))
        .route("/storage/v1/presign/{bucket}/{*key}", post(presign_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Upload an object.
#[tracing::instrument(skip(state, user, headers, body), fields(bucket = %bucket_name, key = %key))]
async fn put_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // RLS: check write permission
    if !state.rls.can_write(user.user_id.as_deref(), &user.roles, bucket) {
        tracing::warn!(
            bucket = %bucket_name,
            user_id = ?user.user_id,
            "Storage upload denied: authentication required"
        );
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required");
    }

    // Validate size
    if let Some(max_bytes) = bucket.max_object_bytes {
        if body.len() as u64 > max_bytes {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                size = body.len(),
                max_bytes = max_bytes,
                "Storage upload rejected: payload too large"
            );
            return error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Object exceeds maximum size",
            );
        }
    }

    // Determine content type
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    // Validate MIME type
    if let Some(ref allowed) = bucket.allowed_mime_types {
        if !allowed.is_empty() && !allowed.iter().any(|m| mime_matches(m, content_type)) {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                content_type = %content_type,
                "Storage upload rejected: MIME type not allowed"
            );
            return error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "mime_type_rejected",
                "Content type not allowed for this bucket",
            );
        }
    }

    // Upload to backend
    let etag = match state.backend.upload(&key, &body, content_type).await {
        Ok(etag) => etag,
        Err(e) => return storage_error_response(&e),
    };

    // Record metadata
    let new_obj = NewStorageObject {
        bucket: bucket_name,
        key,
        content_type: content_type.to_string(),
        // Reason: body length is bounded by max_object_bytes config (set elsewhere); i64
        // capacity is 9.2 EB so wrap is unreachable.
        #[allow(clippy::cast_possible_wrap)]
        size_bytes: body.len() as i64,
        etag: Some(etag.clone()),
        owner_id: user.user_id,
    };
    if let Err(e) = state.metadata.upsert(&new_obj).await {
        return storage_error_response(&e);
    }

    let mut headers = HeaderMap::new();
    if let Ok(val) = etag.parse() {
        headers.insert(header::ETAG, val);
    }
    (StatusCode::OK, headers).into_response()
}

/// Download an object.
#[tracing::instrument(skip(state, user), fields(bucket = %bucket_name, key = %key))]
async fn get_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
) -> Response {
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    // Look up metadata for RLS check
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found"),
        Err(e) => return storage_error_response(&e),
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_read(user.user_id.as_deref(), &user.roles, bucket, &row) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            "Storage download denied: access forbidden"
        );
        return error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied");
    }

    // Download from backend
    match state.backend.download(&key).await {
        Ok(data) => {
            let mut headers = HeaderMap::new();
            if let Ok(ct) = row.content_type.parse() {
                headers.insert(header::CONTENT_TYPE, ct);
            }
            if let Some(ref etag) = row.etag {
                if let Ok(val) = etag.parse() {
                    headers.insert(header::ETAG, val);
                }
            }
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=3600"
                    .parse()
                    .expect("static ASCII header value parses as HeaderValue"),
            );
            (StatusCode::OK, headers, Body::from(data)).into_response()
        },
        Err(e) => storage_error_response(&e),
    }
}

/// Delete an object.
#[tracing::instrument(skip(state, user), fields(bucket = %bucket_name, key = %key))]
async fn delete_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
) -> Response {
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    // Look up metadata for RLS check
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found"),
        Err(e) => return storage_error_response(&e),
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_delete(user.user_id.as_deref(), &user.roles, bucket, &row) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            "Storage delete denied: access forbidden"
        );
        return error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied");
    }

    // Delete from backend
    if let Err(e) = state.backend.delete(&key).await {
        return storage_error_response(&e);
    }

    // Remove metadata
    if let Err(e) = state.metadata.delete(&bucket_name, &key).await {
        return storage_error_response(&e);
    }

    StatusCode::NO_CONTENT.into_response()
}

/// List objects in a bucket.
#[tracing::instrument(skip(state, user, query), fields(bucket = %bucket_name))]
async fn list_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path(bucket_name): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_write(user.user_id.as_deref(), &user.roles, bucket) {
        // For listing, we require at least authenticated access
        // Public bucket reads are handled via filter_visible
        if matches!(bucket.access, crate::config::BucketAccess::Private) {
            return error_response(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required",
            );
        }
    }

    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let rows = match state.metadata.list(&bucket_name, query.prefix.as_deref(), limit, offset).await
    {
        Ok(rows) => rows,
        Err(e) => return storage_error_response(&e),
    };

    // Apply RLS filtering
    let visible = state.rls.filter_visible(user.user_id.as_deref(), &user.roles, bucket, rows);

    let items: Vec<ListItem> = visible.iter().map(ListItem::from).collect();
    axum::Json(items).into_response()
}

/// Generate a presigned URL.
///
/// Pre-v2.4.0 this handler bypassed [`StorageRlsEvaluator`] entirely: any
/// anonymous client could presign GET / PUT against any bucket+key,
/// returning a 24-hour-valid URL for objects in `BucketAccess::Private`
/// buckets owned by other users (#335).  The handler now mirrors the
/// access-control shape of [`put_handler`] / [`get_handler`]:
///
/// - For `operation = "download"`: the metadata row is loaded and `state.rls.can_read` is consulted
///   before signing. Missing objects yield `404`; objects the caller may not read yield `403`.
/// - For `operation = "upload"`: `state.rls.can_write(bucket)` is consulted before signing. No
///   metadata lookup happens because the object may not yet exist; the bucket-level write
///   permission is sufficient.
///
/// # Caveat — bucket constraints are NOT enforced via S3 presigned PUT.
///
/// The S3 presigned PUT URL gives the holder the same effective
/// authority as the FraiseQL server for the bucket+key window: any
/// `Content-Type` and any body size accepted by S3 itself goes through.
/// FraiseQL's bucket-level `max_object_bytes` and `allowed_mime_types`
/// checks live in [`put_handler`] and cannot be encoded in a vanilla S3
/// presigned PUT.  Operators who need those constraints enforced for
/// presigned uploads must (a) restrict presigned uploads to trusted
/// users via RLS, (b) re-validate after the upload via metadata
/// inspection + cleanup, or (c) route uploads through `PUT /storage/v1/{bucket}/{*key}`
/// instead.  This is documented as a known limitation in CHANGELOG.
#[tracing::instrument(skip(state, user, request), fields(bucket = %bucket_name, key = %key))]
async fn presign_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    axum::Json(request): axum::Json<PresignRequest>,
) -> Response {
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Validate operation
    let operation = request.operation.to_lowercase();
    if operation != "upload" && operation != "download" {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_operation",
            "operation must be 'upload' or 'download'",
        );
    }

    if request.expires_in_secs == 0 || request.expires_in_secs > 86400 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_expiry",
            "expires_in_secs must be between 1 and 86400",
        );
    }

    // RLS gate.  Mirrors put_handler / get_handler.  Done before any S3 work
    // so unauthorised callers cannot observe whether the object exists.
    if operation == "upload" {
        if !state.rls.can_write(user.user_id.as_deref(), &user.roles, bucket) {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                user_id = ?user.user_id,
                "Storage presign(upload) denied: authentication required"
            );
            return error_response(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required",
            );
        }
    } else {
        // download: look up metadata so can_read can apply per-row policy.
        let row = match state.metadata.get(&bucket_name, &key).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found");
            },
            Err(e) => return storage_error_response(&e),
        };
        if !state.rls.can_read(user.user_id.as_deref(), &user.roles, bucket, &row) {
            tracing::warn!(
                bucket = %bucket_name,
                key = %key,
                user_id = ?user.user_id,
                "Storage presign(download) denied by RLS"
            );
            return error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied");
        }
    }

    #[cfg(feature = "aws-s3")]
    {
        use std::time::Duration;
        let expires_in = Duration::from_secs(request.expires_in_secs);

        let result = if operation == "upload" {
            let Some(content_type) = request.content_type else {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "missing_content_type",
                    "content_type required for upload",
                );
            };
            state.backend.presign_put(&key, &content_type, expires_in).await
        } else {
            state.backend.presign_get(&key, expires_in).await
        };

        match result {
            Ok(url) => axum::Json(PresignResponse::from(url)).into_response(),
            Err(e) => storage_error_response(&e),
        }
    }

    #[cfg(not(feature = "aws-s3"))]
    {
        let _ = (key, operation, request);
        error_response(
            StatusCode::NOT_IMPLEMENTED,
            "not_supported",
            "Presigned URLs require S3 backend",
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// List item returned in JSON array from list endpoint.
#[derive(Debug, Serialize)]
struct ListItem {
    key:          String,
    size:         i64,
    content_type: String,
    etag:         Option<String>,
    created_at:   String,
    updated_at:   String,
}

impl From<&StorageMetadataRow> for ListItem {
    fn from(row: &StorageMetadataRow) -> Self {
        Self {
            key:          row.key.clone(),
            size:         row.size_bytes,
            content_type: row.content_type.clone(),
            etag:         row.etag.clone(),
            created_at:   row.created_at.to_rfc3339(),
            updated_at:   row.updated_at.to_rfc3339(),
        }
    }
}

/// Build a JSON error response.
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
        }
    });
    (status, axum::Json(body)).into_response()
}

/// Convert a `FraiseQLError` to an appropriate HTTP response.
///
/// After F050 (typed `FileError` migration), backend storage failures arrive
/// as `FraiseQLError::File(FileError::*)` rather than `FraiseQLError::Storage`.
/// The routing here matches the previous behaviour of
/// `Storage { code: Some("...") }`:
///
/// - `FileError::NotFound` → 404
/// - `FileError::PermissionDenied` → 403
/// - other backend variants (`IoError`, `Backend`, `NotImplemented`, `Unsupported`,
///   `SizeLimitExceeded`, `MimeTypeNotAllowed`) → 500
/// - `FileError::InvalidKey` → 400
fn storage_error_response(err: &FraiseQLError) -> Response {
    if let FraiseQLError::File(file_err) = err {
        let (status, code) = match file_err {
            FileError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            FileError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, "permission_denied"),
            FileError::InvalidKey { .. } => (StatusCode::BAD_REQUEST, "invalid_key"),
            FileError::IoError { .. } => {
                tracing::error!(error = %err, "Storage I/O error");
                (StatusCode::INTERNAL_SERVER_ERROR, "io_error")
            },
            FileError::NotImplemented { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "not_implemented")
            },
            FileError::Unsupported { .. } => (StatusCode::INTERNAL_SERVER_ERROR, "not_supported"),
            FileError::SizeLimitExceeded { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "size_limit_exceeded")
            },
            FileError::MimeTypeNotAllowed { .. } => {
                (StatusCode::INTERNAL_SERVER_ERROR, "mime_type_not_allowed")
            },
            FileError::Backend { .. } => {
                tracing::error!(error = %err, "Storage backend error");
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            },
            // Pre-F050 FileError variants — unlikely to reach the storage
            // routes but handled for completeness.
            FileError::TooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            FileError::QuotaExceeded => (StatusCode::PAYLOAD_TOO_LARGE, "quota_exceeded"),
            FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "invalid_type")
            },
            FileError::VirusDetected { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "virus_detected"),
            FileError::Storage { .. } | FileError::Processing { .. } => {
                tracing::error!(error = %err, "Storage backend error");
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_error")
            },
            // SECURITY: `FileError` is `#[non_exhaustive]`. Any future variant
            // added without updating this match falls through to a generic
            // 500 response rather than silently leaking the wrong status.
            _ => {
                tracing::error!(error = %err, "Unhandled FileError variant");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            },
        };
        error_response(status, code, &file_err.to_string())
    } else {
        tracing::error!(error = %err, "Unexpected storage error");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &err.to_string())
    }
}

/// Check if a MIME type pattern matches a content type.
/// Supports wildcard patterns like "image/*".
fn mime_matches(pattern: &str, content_type: &str) -> bool {
    if pattern == "*/*" || pattern == content_type {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return content_type.starts_with(prefix)
            && content_type.as_bytes().get(prefix.len()) == Some(&b'/');
    }
    false
}
