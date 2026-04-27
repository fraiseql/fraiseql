//! Bucket configuration and validation.

/// Access control policy for a bucket.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum BucketAccess {
    /// All operations require authentication.
    Private,
    /// Read operations are public; write operations require authentication.
    PublicRead,
}

/// Bucket configuration.
///
/// Defines size limits, allowed MIME types, and access policies for a bucket.
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
}

/// Storage configuration (from fraiseql-server config).
///
/// This struct represents the storage backend configuration that specifies
/// which storage provider to use and its settings.
#[derive(Debug, Clone)]
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
