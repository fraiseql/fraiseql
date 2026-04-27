//! Bucket configuration and validation.

/// Bucket configuration.
///
/// Defines size limits, allowed MIME types, and access policies for a bucket.
#[derive(Debug, Clone)]
pub struct BucketConfig {
    /// Name of the bucket.
    pub name: String,
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
