//! Policy-enforcing wrapper around storage backends.
//!
//! `BucketService` validates size limits and MIME type restrictions
//! before delegating to the underlying storage backend.

use std::time::Duration;

use crate::backend::StorageBackend;
use crate::config::BucketConfig;
use crate::backend::types::ListResult;
use fraiseql_error::{FraiseQLError, Result};
use super::backend::validate_key;

/// A bucket-aware storage service that enforces policies.
///
/// Wraps a `StorageBackend` with a `BucketConfig` to validate
/// upload size and MIME type restrictions before delegating operations.
pub struct BucketService {
    backend: StorageBackend,
    config: BucketConfig,
}

impl BucketService {
    /// Creates a new bucket service.
    pub const fn new(backend: StorageBackend, config: BucketConfig) -> Self {
        Self { backend, config }
    }

    /// Returns a reference to the bucket configuration.
    pub const fn config(&self) -> &BucketConfig {
        &self.config
    }

    /// Uploads data with policy validation.
    ///
    /// Validates:
    /// - Key is safe (no path traversal)
    /// - Data size does not exceed configured limit
    /// - Content type is in allowed MIME types list
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if validation fails or upload fails.
    pub async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String> {
        validate_key(key)?;

        // Validate size limit
        if let Some(max_bytes) = self.config.max_object_bytes {
            if data.len() as u64 > max_bytes {
                return Err(FraiseQLError::Storage {
                    message: format!(
                        "Upload exceeds maximum object size of {} bytes",
                        max_bytes
                    ),
                    code: Some("size_limit_exceeded".to_string()),
                });
            }
        }

        // Validate MIME type
        if let Some(ref allowed) = self.config.allowed_mime_types {
            let is_allowed = allowed.iter().any(|m| m == content_type || m == "*/*");
            if !is_allowed {
                return Err(FraiseQLError::Storage {
                    message: format!(
                        "Content type '{}' is not allowed for this bucket",
                        content_type
                    ),
                    code: Some("mime_type_not_allowed".to_string()),
                });
            }
        }

        self.backend.upload(key, data, content_type).await
    }

    /// Downloads data without policy validation (policies only apply to upload).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if download fails.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        self.backend.download(key).await
    }

    /// Deletes an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if deletion fails.
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.backend.delete(key).await
    }

    /// Checks if an object exists.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.backend.exists(key).await
    }

    /// Lists objects in the bucket by prefix.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if listing fails.
    pub async fn list(
        &self,
        prefix: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListResult> {
        self.backend.list(prefix, cursor, limit).await
    }

    /// Generates a presigned URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if presigned URLs are not supported
    /// by the backend or if generation fails.
    pub async fn presigned_url(&self, key: &str, expiry: Duration) -> Result<String> {
        self.backend.presigned_url(key, expiry).await
    }
}

#[cfg(test)]
mod tests;
