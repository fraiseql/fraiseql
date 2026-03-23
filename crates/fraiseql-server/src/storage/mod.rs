//! Object storage backends for file upload and download.
//!
//! Provides a [`StorageBackend`] trait with implementations for local filesystem,
//! AWS S3, Google Cloud Storage, Azure Blob Storage, and S3-compatible European
//! providers (Hetzner, Scaleway, OVH, Exoscale, Backblaze B2).

use std::time::Duration;

use async_trait::async_trait;
use fraiseql_error::FileError;

pub mod local;

#[cfg(feature = "aws-s3")]
pub mod s3;

#[cfg(feature = "gcs")]
pub mod gcs;

#[cfg(feature = "azure-blob")]
pub mod azure;

#[cfg(test)]
mod tests;

pub use local::LocalStorageBackend;

#[cfg(feature = "azure-blob")]
pub use self::azure::AzureBlobStorageBackend;
#[cfg(feature = "gcs")]
pub use self::gcs::GcsStorageBackend;
#[cfg(feature = "aws-s3")]
pub use self::s3::S3StorageBackend;

/// Result type for storage operations.
pub type StorageResult<T> = Result<T, FileError>;

/// Trait for object storage backends.
///
/// Implementations handle file upload, download, deletion, existence checks,
/// and presigned URL generation across different storage providers.
///
/// # Errors
///
/// All methods return [`FileError`] on failure. Common variants:
/// - [`FileError::Storage`] for backend communication errors
/// - [`FileError::NotFound`] when a requested key does not exist
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::Storage`] if the upload fails.
    async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> StorageResult<String>;

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if the key does not exist,
    /// or [`FileError::Storage`] on backend errors.
    async fn download(&self, key: &str) -> StorageResult<Vec<u8>>;

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if the key does not exist,
    /// or [`FileError::Storage`] on backend errors.
    async fn delete(&self, key: &str) -> StorageResult<()>;

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::Storage`] on backend communication errors.
    async fn exists(&self, key: &str) -> StorageResult<bool>;

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::Storage`] if presigned URLs are not supported by
    /// the backend or if generation fails.
    async fn presigned_url(&self, key: &str, expiry: Duration) -> StorageResult<String>;
}

/// Validates that a storage key is safe (no path traversal).
///
/// # Errors
///
/// Returns [`FileError::Storage`] if the key is empty, contains `..`,
/// or starts with `/` or `\`.
pub fn validate_key(key: &str) -> StorageResult<()> {
    if key.is_empty() {
        return Err(FileError::Storage {
            message: "Storage key must not be empty".to_string(),
            source:  None,
        });
    }
    if key.contains("..") || key.starts_with('/') || key.starts_with('\\') {
        return Err(FileError::Storage {
            message: "Invalid storage key: must be a relative path without '..'".to_string(),
            source:  None,
        });
    }
    Ok(())
}

/// All S3-compatible backend names recognised by the factory.
const S3_COMPAT_BACKENDS: &[&str] = &[
    "s3",
    "r2",
    "hetzner",
    "scaleway",
    "ovh",
    "exoscale",
    "backblaze",
];

/// Returns a well-known endpoint template for S3-compatible providers.
#[cfg(any(feature = "aws-s3", test))]
/// The `region` placeholder is substituted with the configured region.  If the
/// config already provides an explicit `endpoint`, it takes precedence.
fn default_s3_endpoint(backend: &str, region: Option<&str>) -> Option<String> {
    match backend {
        "r2" => {
            // R2 endpoint requires account ID via config.endpoint; no useful default.
            None
        },
        "hetzner" => {
            let r = region.unwrap_or("fsn1");
            Some(format!("https://{r}.your-objectstorage.com"))
        },
        "scaleway" => {
            let r = region.unwrap_or("fr-par");
            Some(format!("https://s3.{r}.scw.cloud"))
        },
        "ovh" => {
            let r = region.unwrap_or("gra");
            Some(format!("https://s3.{r}.perf.cloud.ovh.net"))
        },
        "exoscale" => {
            let r = region.unwrap_or("de-fra-1");
            Some(format!("https://sos-{r}.exo.io"))
        },
        "backblaze" => {
            // Backblaze B2 S3-compatible endpoint — region is the key-id region prefix.
            let r = region.unwrap_or("us-west-004");
            Some(format!("https://s3.{r}.backblazeb2.com"))
        },
        _ => None,
    }
}

/// Creates a storage backend from a [`StorageConfig`](crate::config::StorageConfig).
///
/// S3-compatible providers (`s3`, `r2`, `hetzner`, `scaleway`, `ovh`, `exoscale`,
/// `backblaze`) all use [`S3StorageBackend`] under the hood.  Provider-specific
/// defaults for the endpoint URL are applied when `endpoint` is not set in the
/// config.
///
/// # Errors
///
/// Returns [`FileError::Storage`] if the backend type is unknown, the required
/// feature is not enabled, or required configuration fields are missing.
pub async fn create_backend(
    config: &crate::config::StorageConfig,
) -> StorageResult<Box<dyn StorageBackend>> {
    let backend_name = config.backend.as_str();

    match backend_name {
        "local" => {
            let path = config.path.as_deref().ok_or_else(|| FileError::Storage {
                message: "Local storage backend requires 'path' configuration".to_string(),
                source:  None,
            })?;
            Ok(Box::new(LocalStorageBackend::new(path)))
        },
        #[cfg(feature = "aws-s3")]
        b if S3_COMPAT_BACKENDS.contains(&b) => {
            let bucket = config.bucket.as_deref().ok_or_else(|| FileError::Storage {
                message: format!("{b} storage backend requires 'bucket' configuration"),
                source:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint(b, config.region.as_deref()));
            let backend =
                S3StorageBackend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(Box::new(backend))
        },
        #[cfg(feature = "gcs")]
        "gcs" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| FileError::Storage {
                message: "GCS storage backend requires 'bucket' configuration".to_string(),
                source:  None,
            })?;
            let backend = GcsStorageBackend::new(bucket)?;
            Ok(Box::new(backend))
        },
        #[cfg(feature = "azure-blob")]
        "azure" => {
            let container = config.bucket.as_deref().ok_or_else(|| FileError::Storage {
                message: "Azure Blob storage requires 'bucket' (container) configuration"
                    .to_string(),
                source:  None,
            })?;
            let account = config.account_name.as_deref().ok_or_else(|| FileError::Storage {
                message: "Azure Blob storage requires 'account_name' configuration".to_string(),
                source:  None,
            })?;
            let backend = AzureBlobStorageBackend::new(account, container)?;
            Ok(Box::new(backend))
        },
        #[cfg(not(feature = "aws-s3"))]
        b if S3_COMPAT_BACKENDS.contains(&b) => Err(FileError::Storage {
            message: format!("{b} storage backend requires the 'aws-s3' feature"),
            source:  None,
        }),
        #[cfg(not(feature = "gcs"))]
        "gcs" => Err(FileError::Storage {
            message: "GCS storage backend requires the 'gcs' feature".to_string(),
            source:  None,
        }),
        #[cfg(not(feature = "azure-blob"))]
        "azure" => Err(FileError::Storage {
            message: "Azure Blob storage backend requires the 'azure-blob' feature".to_string(),
            source:  None,
        }),
        other => Err(FileError::Storage {
            message: format!("Unknown storage backend: {other}"),
            source:  None,
        }),
    }
}
