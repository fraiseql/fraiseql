//! Local filesystem storage backend.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use fraiseql_error::FileError;

use super::{validate_key, StorageBackend, StorageResult};

/// Stores files on the local filesystem under a root directory.
pub struct LocalStorageBackend {
    root: PathBuf,
}

impl LocalStorageBackend {
    /// Creates a new local storage backend rooted at `root`.
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    fn key_path(&self, key: &str) -> StorageResult<PathBuf> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    async fn upload(&self, key: &str, data: &[u8], _content_type: &str) -> StorageResult<String> {
        let path = self.key_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileError::Storage {
                    message: format!("Failed to create directory: {e}"),
                    source:  Some(Box::new(e)),
                })?;
        }
        tokio::fs::write(&path, data)
            .await
            .map_err(|e| FileError::Storage {
                message: format!("Failed to write file: {e}"),
                source:  Some(Box::new(e)),
            })?;
        Ok(key.to_string())
    }

    async fn download(&self, key: &str) -> StorageResult<Vec<u8>> {
        let path = self.key_path(key)?;
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileError::NotFound {
                    id: key.to_string(),
                }
            } else {
                FileError::Storage {
                    message: format!("Failed to read file: {e}"),
                    source:  Some(Box::new(e)),
                }
            }
        })
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        let path = self.key_path(key)?;
        tokio::fs::remove_file(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileError::NotFound {
                    id: key.to_string(),
                }
            } else {
                FileError::Storage {
                    message: format!("Failed to delete file: {e}"),
                    source:  Some(Box::new(e)),
                }
            }
        })
    }

    async fn exists(&self, key: &str) -> StorageResult<bool> {
        let path = self.key_path(key)?;
        match tokio::fs::metadata(&path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(FileError::Storage {
                message: format!("Failed to check file existence: {e}"),
                source:  Some(Box::new(e)),
            }),
        }
    }

    async fn presigned_url(&self, _key: &str, _expiry: Duration) -> StorageResult<String> {
        Err(FileError::Storage {
            message: "Presigned URLs are not supported for local storage".to_string(),
            source:  None,
        })
    }
}
