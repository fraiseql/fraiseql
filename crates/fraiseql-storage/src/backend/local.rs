//! Local filesystem storage backend.

use std::{path::PathBuf, time::Duration};

use fraiseql_error::{FraiseQLError, Result};

use super::validate_key;

/// Stores files on the local filesystem under a root directory.
pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    /// Creates a new local storage backend rooted at `root`.
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    fn key_path(&self, key: &str) -> Result<PathBuf> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }

    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the upload fails.

    /// Uploads data and returns the storage key.
    pub async fn upload(&self, key: &str, data: &[u8], _content_type: &str) -> Result<String> {
        let path = self.key_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| FraiseQLError::Storage {
                message: format!("Failed to create directory: {e}"),
                code: Some("io_error".to_string()),
            })?;
        }
        tokio::fs::write(&path, data).await.map_err(|e| FraiseQLError::Storage {
            message: format!("Failed to write file: {e}"),
            code: Some("io_error".to_string()),
        })?;
        Ok(key.to_string())
    }

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` with code "not_found" if the key does not exist,
    /// or other error codes on backend failures.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.key_path(key)?;
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FraiseQLError::Storage {
                    message: format!("File not found: {key}"),
                    code: Some("not_found".to_string()),
                }
            } else {
                FraiseQLError::Storage {
                    message: format!("Failed to read file: {e}"),
                    code: Some("io_error".to_string()),
                }
            }
        })
    }

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend failures.
    pub async fn delete(&self, key: &str) -> Result<()> {
        let path = self.key_path(key)?;
        tokio::fs::remove_file(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FraiseQLError::Storage {
                    message: format!("File not found: {key}"),
                    code: Some("not_found".to_string()),
                }
            } else {
                FraiseQLError::Storage {
                    message: format!("Failed to delete file: {e}"),
                    code: Some("io_error".to_string()),
                }
            }
        })
    }

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.key_path(key)?;
        match tokio::fs::metadata(&path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(FraiseQLError::Storage {
                message: format!("Failed to check file existence: {e}"),
                code: Some("io_error".to_string()),
            }),
        }
    }

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if presigned URLs are not supported by the backend.
    pub async fn presigned_url(&self, _key: &str, _expiry: Duration) -> Result<String> {
        Err(FraiseQLError::Storage {
            message: "Presigned URLs are not supported for local storage".to_string(),
            code: Some("unsupported".to_string()),
        })
    }
}
