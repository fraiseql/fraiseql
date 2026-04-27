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

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::backend::StorageBackend;
use crate::config::BucketConfig;
use crate::metadata::{NewStorageObject, StorageMetadataRepo, StorageMetadataRow};
use crate::rls::StorageRlsEvaluator;
use fraiseql_error::FraiseQLError;

#[cfg(feature = "aws-s3")]
use crate::{PresignCapable, PresignedUrl};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for all storage route handlers.
#[derive(Clone)]
pub struct StorageState {
    /// Storage backend (shared across all buckets).
    pub backend: Arc<StorageBackend>,
    /// Metadata repository for object tracking.
    pub metadata: Arc<StorageMetadataRepo>,
    /// RLS evaluator for access control.
    pub rls: StorageRlsEvaluator,
    /// Bucket configurations keyed by bucket name.
    pub buckets: Arc<HashMap<String, BucketConfig>>,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for presigned URL generation.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Operation: "upload" (PUT) or "download" (GET).
    pub operation: String,
    /// MIME type (required for uploads, optional for downloads).
    #[serde(default)]
    pub content_type: Option<String>,
    /// URL validity duration in seconds (default: 3600, max: 86400).
    #[serde(default = "default_expiry_secs")]
    pub expires_in_secs: u64,
}

fn default_expiry_secs() -> u64 {
    3600
}

/// Response body for presigned URL generation.
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    /// The presigned URL.
    pub url: String,
    /// When the URL expires (RFC3339 format).
    pub expires_at: String,
    /// HTTP method this URL is valid for.
    pub method: String,
}

#[cfg(feature = "aws-s3")]
impl From<PresignedUrl> for PresignResponse {
    fn from(url: PresignedUrl) -> Self {
        Self {
            url: url.url,
            expires_at: url.expires_at.to_rfc3339(),
            method: url.method,
        }
    }
}

/// Query parameters for list endpoint.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Filter by key prefix.
    pub prefix: Option<String>,
    /// Maximum results (default: 100, max: 1000).
    pub limit: Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}

/// User identity extracted from request (populated by auth middleware).
#[derive(Debug, Clone, Default)]
pub struct StorageUser {
    /// User identifier (sub claim from JWT).
    pub user_id: Option<String>,
    /// User roles.
    pub roles: Vec<String>,
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
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required");
    }

    // Validate size
    if let Some(max_bytes) = bucket.max_object_bytes {
        if body.len() as u64 > max_bytes {
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
        Err(e) => return storage_error_response(e),
    };

    // Record metadata
    let new_obj = NewStorageObject {
        bucket: bucket_name,
        key,
        content_type: content_type.to_string(),
        size_bytes: body.len() as i64,
        etag: Some(etag.clone()),
        owner_id: user.user_id,
    };
    if let Err(e) = state.metadata.upsert(&new_obj).await {
        return storage_error_response(e);
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
        Ok(None) => {
            return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found")
        }
        Err(e) => return storage_error_response(e),
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_read(user.user_id.as_deref(), &user.roles, bucket, &row) {
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
                "public, max-age=3600".parse().unwrap(),
            );
            (StatusCode::OK, headers, Body::from(data)).into_response()
        }
        Err(e) => storage_error_response(e),
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
        Ok(None) => {
            return error_response(StatusCode::NOT_FOUND, "not_found", "Object not found")
        }
        Err(e) => return storage_error_response(e),
    };

    let user = user.map(|Extension(u)| u).unwrap_or_default();
    if !state.rls.can_delete(user.user_id.as_deref(), &user.roles, bucket, &row) {
        return error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied");
    }

    // Delete from backend
    if let Err(e) = state.backend.delete(&key).await {
        return storage_error_response(e);
    }

    // Remove metadata
    if let Err(e) = state.metadata.delete(&bucket_name, &key).await {
        return storage_error_response(e);
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

    let rows = match state
        .metadata
        .list(&bucket_name, query.prefix.as_deref(), limit, offset)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return storage_error_response(e),
    };

    // Apply RLS filtering
    let visible = state
        .rls
        .filter_visible(user.user_id.as_deref(), &user.roles, bucket, rows);

    let items: Vec<ListItem> = visible.iter().map(ListItem::from).collect();
    axum::Json(items).into_response()
}

/// Generate a presigned URL.
#[tracing::instrument(skip(state, request), fields(bucket = %bucket_name, key = %key))]
async fn presign_handler(
    State(state): State<StorageState>,
    Path((bucket_name, key)): Path<(String, String)>,
    axum::Json(request): axum::Json<PresignRequest>,
) -> Response {
    let Some(_bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };

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

    #[cfg(feature = "aws-s3")]
    {
        use std::time::Duration;
        let expires_in = Duration::from_secs(request.expires_in_secs);

        let result = if operation == "upload" {
            let content_type = match request.content_type {
                Some(ct) => ct,
                None => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "missing_content_type",
                        "content_type required for upload",
                    );
                }
            };
            state.backend.presign_put(&key, &content_type, expires_in).await
        } else {
            state.backend.presign_get(&key, expires_in).await
        };

        match result {
            Ok(url) => axum::Json(PresignResponse::from(url)).into_response(),
            Err(e) => storage_error_response(e),
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
    key: String,
    size: i64,
    content_type: String,
    etag: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<&StorageMetadataRow> for ListItem {
    fn from(row: &StorageMetadataRow) -> Self {
        Self {
            key: row.key.clone(),
            size: row.size_bytes,
            content_type: row.content_type.clone(),
            etag: row.etag.clone(),
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
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
fn storage_error_response(err: FraiseQLError) -> Response {
    match &err {
        FraiseQLError::Storage { code, message } => {
            let status = match code.as_deref() {
                Some("not_found") => StatusCode::NOT_FOUND,
                Some("permission_denied") => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            error_response(status, code.as_deref().unwrap_or("storage_error"), message)
        }
        _ => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &err.to_string(),
        ),
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
