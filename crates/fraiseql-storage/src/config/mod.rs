//! Bucket configuration and validation.

use serde::{Deserialize, Serialize};

/// Access control policy for a bucket.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum BucketAccess {
    /// All operations require authentication.
    Private,
    /// Read operations are public; write operations require authentication.
    PublicRead,
}

/// Image transform preset for predefined transformations.
///
/// Allows defining common image transformations (e.g., thumbnails, previews)
/// that can be applied by name via the render endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformPreset {
    /// Name of the preset (e.g., "thumbnail", "medium", "preview")
    pub name: String,

    /// Target width in pixels
    pub width: Option<u32>,

    /// Target height in pixels
    pub height: Option<u32>,

    /// Output format (e.g., "webp", "jpeg", "png")
    pub format: Option<String>,

    /// Quality for lossy formats (1-100)
    pub quality: Option<u8>,
}

/// Bucket configuration.
///
/// Defines size limits, allowed MIME types, access policies, and transform presets for a bucket.
#[derive(Debug, Clone)]
pub struct BucketConfig {
    /// Name of the bucket.
    pub name: String,

    /// Maximum object size in bytes (None = unlimited).
    pub max_object_bytes: Option<u64>,

    /// Allowed MIME types (None = any; Some([]) = none allowed).
    pub allowed_mime_types: Option<Vec<String>>,

    /// Access control policy.
    pub access: BucketAccess,

    /// Predefined image transform presets
    pub transform_presets: Option<Vec<TransformPreset>>,
}

/// Storage configuration (from fraiseql-server config).
///
/// This struct represents the storage backend configuration that specifies
/// which storage provider to use and its settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type: "local", "s3", "gcs", "azure"
    pub backend: String,

    /// Path for local filesystem backend
    pub path: Option<String>,

    /// Bucket name for S3, GCS, and Azure backends
    pub bucket: Option<String>,

    /// AWS region for S3 backend
    pub region: Option<String>,

    /// Custom endpoint for S3-compatible services
    pub endpoint: Option<String>,

    /// GCP project ID for GCS backend
    pub project_id: Option<String>,

    /// Azure account name
    pub account_name: Option<String>,
}
