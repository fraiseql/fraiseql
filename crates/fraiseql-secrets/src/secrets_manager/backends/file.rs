//! Backend for reading secrets from local files

use std::path::PathBuf;

use chrono::{Duration, Utc};

use super::super::{SecretsBackend, SecretsError};

/// Secrets backend that reads from local files
///
/// Useful for local development and testing
/// Not recommended for production
///
/// # File Format
/// Each secret stored in separate file as plain text
/// Filename is the secret name, content is the value
///
/// # Example
/// ```ignore
/// // Create file ~/.secrets/db_password
/// let backend = FileBackend::new("~/.secrets");
/// let secret = backend.get_secret("db_password").await?;
/// // Reads from ~/.secrets/db_password
/// ```
#[derive(Clone, Debug)]
pub struct FileBackend {
    base_path: PathBuf,
}

#[async_trait::async_trait]
impl SecretsBackend for FileBackend {
    async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
        let path = self.base_path.join(name);

        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            SecretsError::BackendError(format!(
                "Failed to read secret from {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(content.trim().to_string())
    }

    async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, chrono::DateTime<Utc>), SecretsError> {
        let secret = self.get_secret(name).await?;
        // File-based secrets don't expire; use 1-year TTL
        let expiry = Utc::now() + Duration::days(365);
        Ok((secret, expiry))
    }

    async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        // File-based secrets can't be rotated programmatically
        Err(SecretsError::RotationError(format!(
            "Rotation not supported for file-based secret {}",
            name
        )))
    }
}

impl FileBackend {
    /// Create new FileBackend with base path
    pub fn new<P: Into<PathBuf>>(base_path: P) -> Self {
        FileBackend {
            base_path: base_path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test FileBackend reads from file
    #[tokio::test]
    async fn test_file_backend_read_secret() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let secret_file = dir.path().join("test_secret");
        tokio::fs::write(&secret_file, "secret_content_123").await.unwrap();

        let backend = FileBackend::new(dir.path());
        let secret = backend.get_secret("test_secret").await.unwrap();

        assert_eq!(secret, "secret_content_123");
    }

    /// Test FileBackend returns error for missing file
    #[tokio::test]
    async fn test_file_backend_not_found() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path());
        let result = backend.get_secret("nonexistent.txt").await;

        assert!(result.is_err());
    }

    /// Test FileBackend trims whitespace
    #[tokio::test]
    async fn test_file_backend_trims_whitespace() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let secret_file = dir.path().join("whitespace_secret");
        tokio::fs::write(&secret_file, "  secret_value  \n").await.unwrap();

        let backend = FileBackend::new(dir.path());
        let secret = backend.get_secret("whitespace_secret").await.unwrap();

        assert_eq!(secret, "secret_value");
    }

    /// Test FileBackend with_expiry returns future date
    #[tokio::test]
    async fn test_file_backend_with_expiry() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let secret_file = dir.path().join("expiry_test");
        tokio::fs::write(&secret_file, "value").await.unwrap();

        let backend = FileBackend::new(dir.path());
        let (secret, expiry) = backend.get_secret_with_expiry("expiry_test").await.unwrap();

        assert_eq!(secret, "value");
        assert!(expiry > Utc::now());
    }

    /// Test FileBackend rotate returns error
    #[tokio::test]
    async fn test_file_backend_rotate_not_supported() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path());
        let result = backend.rotate_secret("any_file").await;

        assert!(result.is_err());
    }

    /// Test FileBackend with multiple files
    #[tokio::test]
    async fn test_file_backend_multiple_secrets() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("secret1"), "value1").await.unwrap();
        tokio::fs::write(dir.path().join("secret2"), "value2").await.unwrap();

        let backend = FileBackend::new(dir.path());

        let s1 = backend.get_secret("secret1").await.unwrap();
        let s2 = backend.get_secret("secret2").await.unwrap();

        assert_eq!(s1, "value1");
        assert_eq!(s2, "value2");
    }

    /// Test FileBackend handles empty files
    #[tokio::test]
    async fn test_file_backend_empty_file() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("empty"), "").await.unwrap();

        let backend = FileBackend::new(dir.path());
        let secret = backend.get_secret("empty").await.unwrap();

        assert_eq!(secret, "");
    }
}
