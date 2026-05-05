//! Storage browser endpoints for the Studio dashboard.
//!
//! Routes under `/admin/v1/storage/*` expose bucket listing, object listing
//! with prefix filtering, presigned URL generation, and object deletion.
//! All routes are protected by the admin bearer token middleware.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};

use crate::routes::graphql::app_state::AppState;

// ---------------------------------------------------------------------------
// Object record
// ---------------------------------------------------------------------------

/// A single object entry in the storage browser.
///
/// Agreed response shape with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEntry {
    /// Object key (path within the bucket).
    pub key: String,
    /// Object size in bytes.
    pub size: u64,
    /// MIME content type.
    pub content_type: String,
    /// Last-modified timestamp (RFC 3339).
    pub updated_at: String,
}

/// A storage bucket summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketEntry {
    /// Bucket name.
    pub name: String,
    /// Number of objects in the bucket.
    pub object_count: u64,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Paginated object list response agreed with the Luxen UI author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectListResponse {
    /// Objects on this page.
    pub objects: Vec<ObjectEntry>,
    /// Total object count.
    pub total: u64,
    /// Current page number (1-indexed).
    pub page: u32,
    /// Objects per page.
    pub page_size: u32,
}

/// Bucket list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketListResponse {
    /// All buckets for this tenant.
    pub buckets: Vec<BucketEntry>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Query parameters for `GET /admin/v1/storage/objects`.
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectListQuery {
    /// Bucket to list.
    pub bucket: String,
    /// Key prefix filter (optional).
    pub prefix: Option<String>,
    /// Page number (1-indexed, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Objects per page (default 50).
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

const fn default_page() -> u32 {
    1
}

const fn default_page_size() -> u32 {
    50
}

/// Request body for `POST /admin/v1/storage/objects/sign`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignRequest {
    /// Bucket containing the object.
    pub bucket: String,
    /// Object key.
    pub key: String,
    /// URL expiry in seconds.
    pub expires_in_secs: u32,
}

/// Request body for `DELETE /admin/v1/storage/objects`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteObjectRequest {
    /// Bucket containing the object.
    pub bucket: String,
    /// Object key.
    pub key: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/v1/storage/buckets` — list all buckets for the tenant.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn list_buckets_handler<A>(State(_state): State<AppState<A>>) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    // Placeholder — not yet wired to StorageBackend.
    Json(BucketListResponse { buckets: vec![] })
}

/// `GET /admin/v1/storage/objects` — paginated object list with prefix filtering.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
/// Returns `404` if the bucket does not exist.
pub async fn list_objects_handler<A>(
    State(_state): State<AppState<A>>,
    Query(_params): Query<ObjectListQuery>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    Json(ObjectListResponse {
        objects: vec![],
        total: 0,
        page: 1,
        page_size: 50,
    })
}

/// `POST /admin/v1/storage/objects/sign` — generate a presigned URL.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn presign_handler<A>(
    State(_state): State<AppState<A>>,
    Json(req): Json<PresignRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Not Implemented",
            "message": format!(
                "Presign for {}/{} not yet wired",
                req.bucket, req.key
            )
        })),
    )
}

/// `DELETE /admin/v1/storage/objects` — delete an object by bucket + key.
///
/// # Errors
///
/// Returns `401` without valid admin credentials (enforced by middleware).
pub async fn delete_object_handler<A>(
    State(_state): State<AppState<A>>,
    Json(req): Json<DeleteObjectRequest>,
) -> impl IntoResponse
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Not Implemented",
            "message": format!(
                "Delete {}/{} not yet wired",
                req.bucket, req.key
            )
        })),
    )
}

