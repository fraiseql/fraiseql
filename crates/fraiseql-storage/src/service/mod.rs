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
    pub fn new(backend: StorageBackend, config: BucketConfig) -> Self {
        Self { backend, config }
    }

    /// Returns a reference to the bucket configuration.
    pub fn config(&self) -> &BucketConfig {
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
mod tests {
    use super::*;
    use crate::backend::LocalBackend;
    use crate::config::BucketAccess;
    use tempfile::TempDir;

    fn temp_service(max_size: Option<u64>, allowed_types: Option<Vec<String>>) -> (BucketService, TempDir) {
        let tmpdir = TempDir::new().expect("create tempdir");
        let backend = StorageBackend::Local(LocalBackend::new(tmpdir.path().to_str().unwrap()));
        let config = BucketConfig {
            name: "test".to_string(),
            max_object_bytes: max_size,
            allowed_mime_types: allowed_types,
            access: BucketAccess::Private,
        };
        (BucketService::new(backend, config), tmpdir)
    }

    #[tokio::test]
    async fn test_size_limit_rejected() {
        let (service, _tmpdir) = temp_service(Some(100), None);

        let result = service
            .upload("test.bin", &[0u8; 150], "application/octet-stream")
            .await;

        let err = result.expect_err("should reject oversized upload");
        assert!(
            matches!(err, FraiseQLError::Storage { .. }),
            "should be a Storage error"
        );
        if let FraiseQLError::Storage { code, .. } = err {
            assert_eq!(code, Some("size_limit_exceeded".to_string()));
        }
    }

    #[tokio::test]
    async fn test_size_limit_accepted() {
        let (service, _tmpdir) = temp_service(Some(100), None);

        let result = service
            .upload("test.bin", &[0u8; 50], "application/octet-stream")
            .await;

        result.expect("should accept upload within limit");
    }

    #[tokio::test]
    async fn test_mime_type_rejected() {
        let (service, _tmpdir) = temp_service(None, Some(vec!["image/jpeg".to_string()]));

        let result = service
            .upload("test.txt", b"text", "text/plain")
            .await;

        let err = result.expect_err("should reject disallowed MIME type");
        if let FraiseQLError::Storage { code, .. } = err {
            assert_eq!(code, Some("mime_type_not_allowed".to_string()));
        }
    }

    #[tokio::test]
    async fn test_mime_type_wildcard() {
        let (service, _tmpdir) = temp_service(None, Some(vec!["*/*".to_string()]));

        let result = service
            .upload("test.anything", b"data", "application/anything")
            .await;

        result.expect("wildcard should accept any MIME type");
    }

    #[tokio::test]
    async fn test_no_policy_passes_through() {
        let (service, _tmpdir) = temp_service(None, None);

        let result = service
            .upload("test.bin", &vec![0u8; 1_000_000], "application/octet-stream")
            .await;

        result.expect("no limits should allow any upload");
    }

    #[tokio::test]
    async fn test_list_delegates_to_backend() {
        let (service, _tmpdir) = temp_service(None, None);

        service
            .upload("file1.txt", b"data", "text/plain")
            .await
            .expect("upload");
        service
            .upload("file2.txt", b"data", "text/plain")
            .await
            .expect("upload");

        let result = service
            .list("", None, 100)
            .await
            .expect("list");

        assert_eq!(result.objects.len(), 2, "should list both files");
    }
}
