//! Object storage backends for file upload and download.
//!
//! Provides enum-based dispatch to local filesystem, AWS S3, Google Cloud Storage,
//! Azure Blob Storage, and S3-compatible European providers (Hetzner, Scaleway, OVH,
//! Exoscale, Backblaze B2, Cloudflare R2).

use std::time::Duration;

use chrono::{DateTime, Utc};
use fraiseql_error::Result;
use serde::{Deserialize, Serialize};

pub mod local;
pub mod types;

/// Presigned URL for time-limited direct access to an object.
///
/// Can be used for direct uploads (PUT) or downloads (GET) without going through
/// the FraiseQL server, reducing server load and enabling client-side uploads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedUrl {
    /// The complete presigned URL (including query parameters)
    pub url: String,
    /// When the URL expires (UTC)
    pub expires_at: DateTime<Utc>,
    /// HTTP method this URL is valid for (GET or PUT)
    pub method: String,
}

impl PresignedUrl {
    /// Creates a new presigned URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The complete presigned URL
    /// * `expires_at` - When the URL expires
    /// * `method` - HTTP method (GET or PUT)
    pub fn new(url: String, expires_at: DateTime<Utc>, method: &str) -> Self {
        Self {
            url,
            expires_at,
            method: method.to_uppercase(),
        }
    }
}

/// Capability trait for backends that support presigned URLs.
///
/// Not all backends support presigned URLs. For example, `LocalBackend` cannot
/// generate presigned URLs for direct client access (it's a filesystem, not a service).
///
/// This trait is implemented separately from `StorageBackend` to make it optional.
/// Check if a backend implements this trait before using presigned URL features.
#[cfg(feature = "aws-s3")]
#[allow(async_fn_in_trait)] // Reason: native async syntax as specified in Phase 2, Cycle 2
pub trait PresignCapable {
    /// Generates a presigned URL for uploading an object (PUT).
    ///
    /// The returned URL can be used directly by clients to upload files without
    /// credentials, useful for browser-based uploads.
    ///
    /// # Arguments
    ///
    /// * `key` - The object key (storage path)
    /// * `content_type` - The MIME type for the upload
    /// * `expires_in` - How long the URL remains valid
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if URL generation fails.
    async fn presign_put(
        &self,
        key: &str,
        content_type: &str,
        expires_in: Duration,
    ) -> Result<PresignedUrl>;

    /// Generates a presigned URL for downloading an object (GET).
    ///
    /// The returned URL can be used directly by clients to download files,
    /// useful for serving content from S3 directly.
    ///
    /// # Arguments
    ///
    /// * `key` - The object key (storage path)
    /// * `expires_in` - How long the URL remains valid
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if URL generation fails.
    async fn presign_get(&self, key: &str, expires_in: Duration) -> Result<PresignedUrl>;
}

#[cfg(feature = "aws-s3")]
pub mod s3;

#[cfg(feature = "gcs")]
pub mod gcs;

#[cfg(feature = "azure-blob")]
pub mod azure;

#[cfg(test)]
mod tests;

pub use local::LocalBackend;

#[cfg(feature = "azure-blob")]
pub use self::azure::AzureBackend;
#[cfg(feature = "gcs")]
pub use self::gcs::GcsBackend;
#[cfg(feature = "aws-s3")]
pub use self::s3::S3Backend;

/// Enum-based storage backend dispatch to local filesystem, S3, GCS, or Azure.
///
/// Provides unified async methods for file upload, download, deletion, existence checks,
/// and presigned URL generation across all supported providers.
///
/// # Errors
///
/// All methods return [`FraiseQLError::Storage`] on failure.
#[non_exhaustive]
pub enum StorageBackend {
    /// Local filesystem storage.
    Local(LocalBackend),
    /// AWS S3 storage.
    #[cfg(feature = "aws-s3")]
    S3(S3Backend),
    /// Hetzner Object Storage (S3-compatible).
    #[cfg(feature = "aws-s3")]
    Hetzner(S3Backend),
    /// Scaleway Object Storage (S3-compatible).
    #[cfg(feature = "aws-s3")]
    Scaleway(S3Backend),
    /// OVH Object Storage (S3-compatible).
    #[cfg(feature = "aws-s3")]
    Ovh(S3Backend),
    /// Exoscale Object Storage (S3-compatible).
    #[cfg(feature = "aws-s3")]
    Exoscale(S3Backend),
    /// Backblaze B2 (S3-compatible).
    #[cfg(feature = "aws-s3")]
    Backblaze(S3Backend),
    /// Cloudflare R2 (S3-compatible).
    #[cfg(feature = "aws-s3")]
    R2(S3Backend),
    /// Google Cloud Storage.
    #[cfg(feature = "gcs")]
    Gcs(GcsBackend),
    /// Azure Blob Storage.
    #[cfg(feature = "azure-blob")]
    Azure(AzureBackend),
}

impl StorageBackend {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the upload fails.
    pub async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> Result<String> {
        match self {
            Self::Local(b) => b.upload(key, data, content_type).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => {
                b.upload(key, data, content_type).await
            }
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.upload(key, data, content_type).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.upload(key, data, content_type).await,
        }
    }

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` with code "not_found" if the key does not exist,
    /// or other error codes on backend failures.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        match self {
            Self::Local(b) => b.download(key).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => b.download(key).await,
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.download(key).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.download(key).await,
        }
    }

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend failures.
    pub async fn delete(&self, key: &str) -> Result<()> {
        match self {
            Self::Local(b) => b.delete(key).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => b.delete(key).await,
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.delete(key).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.delete(key).await,
        }
    }

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        match self {
            Self::Local(b) => b.exists(key).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => b.exists(key).await,
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.exists(key).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.exists(key).await,
        }
    }

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if presigned URLs are not supported by
    /// the backend or if generation fails.
    pub async fn presigned_url(&self, key: &str, expiry: Duration) -> Result<String> {
        match self {
            Self::Local(b) => b.presigned_url(key, expiry).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => {
                b.presigned_url(key, expiry).await
            }
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.presigned_url(key, expiry).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.presigned_url(key, expiry).await,
        }
    }

    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on I/O or backend failures.
    pub async fn list(
        &self,
        prefix: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<types::ListResult> {
        match self {
            Self::Local(b) => b.list(prefix, cursor, limit).await,
            #[cfg(feature = "aws-s3")]
            Self::S3(b) | Self::Hetzner(b) | Self::Scaleway(b) | Self::Ovh(b)
            | Self::Exoscale(b) | Self::Backblaze(b) | Self::R2(b) => {
                b.list(prefix, cursor, limit).await
            }
            #[cfg(feature = "gcs")]
            Self::Gcs(b) => b.list(prefix, cursor, limit).await,
            #[cfg(feature = "azure-blob")]
            Self::Azure(b) => b.list(prefix, cursor, limit).await,
        }
    }
}

/// Validates that a storage key is safe (no path traversal).
///
/// # Errors
///
/// Returns `FraiseQLError::Storage` if the key is empty, contains `..`,
/// or starts with `/` or `\`.
pub fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(fraiseql_error::FraiseQLError::Storage {
            message: "Storage key must not be empty".to_string(),
            code: None,
        });
    }
    if key.contains("..") || key.starts_with('/') || key.starts_with('\\') {
        return Err(fraiseql_error::FraiseQLError::Storage {
            message: "Invalid storage key: must be a relative path without '..'".to_string(),
            code: Some("invalid_key".to_string()),
        });
    }
    Ok(())
}

/// Returns a well-known endpoint template for S3-compatible providers.
///
/// The `region` placeholder is substituted with the configured region.  If the
/// config already provides an explicit `endpoint`, it takes precedence.
#[cfg(any(feature = "aws-s3", test))]
#[allow(dead_code)] // Reason: only used when aws-s3 feature is enabled
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
/// S3-compatible providers (`s3`, `hetzner`, `scaleway`, `ovh`, `exoscale`,
/// `backblaze`, `r2`) each get their own enum variant but use the same underlying
/// `S3Backend` implementation. Provider-specific defaults for the endpoint URL are
/// applied when `endpoint` is not set in the config.
///
/// # Errors
///
/// Returns `FraiseQLError::Storage` if the backend type is unknown, the required
/// feature is not enabled, or required configuration fields are missing.
pub async fn create_backend(
    config: &crate::config::StorageConfig,
) -> Result<StorageBackend> {
    let backend_name = config.backend.as_str();

    match backend_name {
        "local" => {
            let path = config.path.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Local storage backend requires 'path' configuration".to_string(),
                code:  None,
            })?;
            Ok(StorageBackend::Local(LocalBackend::new(path)))
        },
        #[cfg(feature = "aws-s3")]
        "s3" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "AWS S3 storage backend requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config.endpoint.as_deref().map(str::to_owned);
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::S3(backend))
        },
        #[cfg(feature = "aws-s3")]
        "hetzner" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Hetzner Object Storage requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint("hetzner", config.region.as_deref()));
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::Hetzner(backend))
        },
        #[cfg(feature = "aws-s3")]
        "scaleway" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Scaleway Object Storage requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint("scaleway", config.region.as_deref()));
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::Scaleway(backend))
        },
        #[cfg(feature = "aws-s3")]
        "ovh" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "OVH Object Storage requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint("ovh", config.region.as_deref()));
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::Ovh(backend))
        },
        #[cfg(feature = "aws-s3")]
        "exoscale" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Exoscale Object Storage requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint("exoscale", config.region.as_deref()));
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::Exoscale(backend))
        },
        #[cfg(feature = "aws-s3")]
        "backblaze" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Backblaze B2 storage requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config
                .endpoint
                .as_deref()
                .map(str::to_owned)
                .or_else(|| default_s3_endpoint("backblaze", config.region.as_deref()));
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), endpoint.as_deref()).await;
            Ok(StorageBackend::Backblaze(backend))
        },
        #[cfg(feature = "aws-s3")]
        "r2" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Cloudflare R2 requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let endpoint = config.endpoint.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Cloudflare R2 requires 'endpoint' configuration (account ID in URL)".to_string(),
                code:  None,
            })?;
            let backend =
                S3Backend::new(bucket, config.region.as_deref(), Some(endpoint)).await;
            Ok(StorageBackend::R2(backend))
        },
        #[cfg(feature = "gcs")]
        "gcs" => {
            let bucket = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "GCS storage backend requires 'bucket' configuration".to_string(),
                code:  None,
            })?;
            let backend = GcsBackend::new(bucket)?;
            Ok(StorageBackend::Gcs(backend))
        },
        #[cfg(feature = "azure-blob")]
        "azure" => {
            let container = config.bucket.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Azure Blob storage requires 'bucket' (container) configuration".to_string(),
                code:  None,
            })?;
            let account = config.account_name.as_deref().ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                message: "Azure Blob storage requires 'account_name' configuration".to_string(),
                code:  None,
            })?;
            let backend = AzureBackend::new(account, container)?;
            Ok(StorageBackend::Azure(backend))
        },
        #[cfg(not(feature = "aws-s3"))]
        "s3" | "hetzner" | "scaleway" | "ovh" | "exoscale" | "backblaze" | "r2" => {
            Err(fraiseql_error::FraiseQLError::Storage {
                message: "S3-compatible storage backends require the 'aws-s3' feature".to_string(),
                code:  None,
            })
        },
        #[cfg(not(feature = "gcs"))]
        "gcs" => Err(fraiseql_error::FraiseQLError::Storage {
            message: "GCS storage backend requires the 'gcs' feature".to_string(),
            code:  None,
        }),
        #[cfg(not(feature = "azure-blob"))]
        "azure" => Err(fraiseql_error::FraiseQLError::Storage {
            message: "Azure Blob storage backend requires the 'azure-blob' feature".to_string(),
            code:  None,
        }),
        other => Err(fraiseql_error::FraiseQLError::Storage {
            message: format!("Unknown storage backend: {other}"),
            code:  None,
        }),
    }
}
