//! HTTP route handlers for storage operations.
//!
//! This module provides handler functions for common storage operations.
//! Route mounting and HTTP server integration is handled by fraiseql-server.

use serde::{Deserialize, Serialize};

use crate::PresignedUrl;

#[cfg(feature = "aws-s3")]
use std::time::Duration;

#[cfg(feature = "aws-s3")]
use fraiseql_error::{FraiseQLError, Result};

#[cfg(feature = "aws-s3")]
use crate::{StorageRlsEvaluator, PresignCapable};

/// Request body for presigned URL generation.
///
/// Specifies what kind of presigned URL to generate and for how long.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Operation: "upload" (PUT) or "download" (GET)
    pub operation: String,
    /// MIME type (required for uploads, optional for downloads)
    #[serde(default)]
    pub content_type: Option<String>,
    /// URL validity duration in seconds (default: 3600)
    #[serde(default = "default_expiry_secs")]
    pub expires_in_secs: u64,
}

fn default_expiry_secs() -> u64 {
    3600 // 1 hour
}

/// Response body for presigned URL generation.
///
/// Contains the signed URL and expiration time.
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    /// The presigned URL (can be used directly by clients)
    pub url: String,
    /// When the URL expires (RFC3339 format)
    pub expires_at: String,
    /// HTTP method this URL is valid for
    pub method: String,
}

impl From<PresignedUrl> for PresignResponse {
    fn from(url: PresignedUrl) -> Self {
        Self {
            url: url.url,
            expires_at: url.expires_at.to_rfc3339(),
            method: url.method,
        }
    }
}

/// Generates a presigned URL for direct S3 access.
///
/// This handler:
/// 1. Validates the request
/// 2. Checks RLS permissions (if evaluator provided) [Phase 5+]
/// 3. Generates the presigned URL
/// 4. Returns the signed URL to the client
///
/// # Arguments
///
/// * `backend` - Storage backend that supports presigned URLs
/// * `bucket` - Bucket name
/// * `key` - Object key (storage path)
/// * `request` - Presign request with operation type and expiry
/// * `_rls_evaluator` - Optional RLS evaluator for permission checks (Phase 5+)
/// * `_user_id` - Current user ID for RLS checks (Phase 5+)
///
/// # Errors
///
/// Returns error if:
/// - RLS denies the operation (Phase 5+)
/// - Backend doesn't support presigned URLs
/// - URL generation fails
#[cfg(feature = "aws-s3")]
pub async fn presign_handler(
    backend: &(impl PresignCapable + ?Sized),
    _bucket: &str,
    key: &str,
    request: PresignRequest,
    _rls_evaluator: Option<&StorageRlsEvaluator>,
    _user_id: Option<&str>,
) -> Result<PresignResponse> {
    // Validate request
    let operation = request.operation.to_lowercase();
    if operation != "upload" && operation != "download" {
        return Err(FraiseQLError::Validation {
            message: "operation must be 'upload' or 'download'".to_string(),
            path: Some("operation".to_string()),
        });
    }

    if request.expires_in_secs == 0 || request.expires_in_secs > 86400 {
        return Err(FraiseQLError::Validation {
            message: "expires_in_secs must be between 1 and 86400 (24 hours)".to_string(),
            path: Some("expires_in_secs".to_string()),
        });
    }

    // TODO: Phase 5 - Check RLS if evaluator provided
    // This would check bucket-level and object-level read/write permissions
    // For now, presigned URLs require the user to have already been authorized

    // Generate presigned URL
    let expires_in = Duration::from_secs(request.expires_in_secs);

    let presigned_url = if operation == "upload" {
        let content_type = request
            .content_type
            .ok_or_else(|| FraiseQLError::Validation {
                message: "content_type required for upload operation".to_string(),
                path: Some("content_type".to_string()),
            })?;

        backend
            .presign_put(key, &content_type, expires_in)
            .await?
    } else {
        backend.presign_get(key, expires_in).await?
    };

    Ok(PresignResponse::from(presigned_url))
}

/// Render handler for on-the-fly image transforms with caching.
///
/// This handler:
/// 1. Fetches the original image from storage
/// 2. Applies the requested transform (resize, format conversion, etc.)
/// 3. Caches the result for future requests
/// 4. Returns the transformed image with appropriate headers
///
/// # Arguments
///
/// * `cache` - Transform cache for storing and retrieving cached results
/// * `key` - Object key (storage path)
/// * `width` - Optional target width for resizing
/// * `preset` - Optional preset name for predefined transforms
///
/// # Returns
///
/// Transformed image output with content type and dimensions
///
/// # Errors
///
/// Returns error if:
/// - Object doesn't exist (404)
/// - Input is not a valid image (400)
/// - Transform fails
#[cfg(all(feature = "transforms", test))]
pub async fn render_handler(
    cache: &crate::transforms::cache::TransformCache,
    key: &str,
    width: Option<u32>,
    preset: Option<&str>,
) -> fraiseql_error::Result<crate::transforms::TransformOutput> {
    use crate::transforms::TransformParams;

    // Fetch original image
    let source_data = cache.get_source(key).await?;

    // Build transform params
    let params = if let Some(_preset_name) = preset {
        // Look up preset - for now, use direct width if available
        // Phase 2, Cycle 5: Implement preset lookup from config
        TransformParams {
            width,
            height: None,
            format: None,
            quality: None,
        }
    } else {
        TransformParams {
            width,
            height: None,
            format: None,
            quality: None,
        }
    };

    // Get or transform
    cache
        .get_or_transform(key, &source_data, &params)
        .await?
        .ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
            message: "Failed to transform image".to_string(),
            code: Some("transform_failed".to_string()),
        })
}

/// Storage router setup.
///
/// This function would mount HTTP routes in a real HTTP server integration.
/// For now, it's a placeholder for when routes are fully integrated with fraiseql-server.
pub fn storage_router() {
    // Routes:
    // POST /storage/v1/presign/:bucket/:key -> presign_handler
    // GET /storage/v1/render/:bucket/:key -> render_handler (Phase 2, Cycle 4)
    // PUT /storage/v1/upload/:bucket/:key -> upload_handler (Phase 2, Cycle 4)
    // DELETE /storage/v1/objects/:bucket/:key -> delete_handler
    // GET /storage/v1/objects/:bucket -> list_handler
}
