//! Backup storage backends.

use std::path::{Path, PathBuf};

/// Storage backend trait.
#[async_trait::async_trait]
pub trait BackupStorage: Send + Sync {
    /// Store backup data.
    ///
    /// # Arguments
    /// * `backup_id` - Unique backup identifier
    /// * `data` - Backup content bytes
    async fn store(&self, backup_id: &str, data: &[u8]) -> Result<u64, StorageError>;

    /// Retrieve backup data.
    async fn retrieve(&self, backup_id: &str) -> Result<Vec<u8>, StorageError>;

    /// Delete backup data.
    async fn delete(&self, backup_id: &str) -> Result<(), StorageError>;

    /// List all backup IDs.
    async fn list(&self) -> Result<Vec<String>, StorageError>;

    /// Get backup size in bytes.
    async fn get_size(&self, backup_id: &str) -> Result<u64, StorageError>;

    /// Check if backup exists.
    async fn exists(&self, backup_id: &str) -> Result<bool, StorageError>;
}

/// Storage backend errors.
#[derive(Debug, Clone)]
pub enum StorageError {
    IoError { message: String },
    NotFound { backup_id: String },
    PermissionDenied { message: String },
    Other { message: String },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError { message } => write!(f, "IO error: {}", message),
            Self::NotFound { backup_id } => write!(f, "Backup not found: {}", backup_id),
            Self::PermissionDenied { message } => write!(f, "Permission denied: {}", message),
            Self::Other { message } => write!(f, "Storage error: {}", message),
        }
    }
}

impl std::error::Error for StorageError {}

/// Local filesystem storage backend.
pub struct LocalFileStorage {
    base_path: PathBuf,
}

impl LocalFileStorage {
    /// Create new local file storage.
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = base_path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&path).map_err(|e| StorageError::IoError {
            message: format!("Failed to create backup directory: {}", e),
        })?;

        Ok(Self { base_path: path })
    }

    /// Get full path for a backup ID.
    fn get_path(&self, backup_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.backup", backup_id))
    }
}

#[async_trait::async_trait]
impl BackupStorage for LocalFileStorage {
    async fn store(&self, backup_id: &str, data: &[u8]) -> Result<u64, StorageError> {
        let path = self.get_path(backup_id);

        tokio::fs::write(&path, data)
            .await
            .map_err(|e| StorageError::IoError {
                message: format!("Failed to write backup: {}", e),
            })?;

        Ok(data.len() as u64)
    }

    async fn retrieve(&self, backup_id: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.get_path(backup_id);

        if !path.exists() {
            return Err(StorageError::NotFound {
                backup_id: backup_id.to_string(),
            });
        }

        tokio::fs::read(&path)
            .await
            .map_err(|e| StorageError::IoError {
                message: format!("Failed to read backup: {}", e),
            })
    }

    async fn delete(&self, backup_id: &str) -> Result<(), StorageError> {
        let path = self.get_path(backup_id);

        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| StorageError::IoError {
                    message: format!("Failed to delete backup: {}", e),
                })?;
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>, StorageError> {
        let mut entries = Vec::new();

        let mut dir = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| StorageError::IoError {
                message: format!("Failed to list backups: {}", e),
            })?;

        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| StorageError::IoError {
                message: format!("Failed to read backup entry: {}", e),
            })?
        {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".backup") {
                    let backup_id = name.strip_suffix(".backup").unwrap_or(name).to_string();
                    entries.push(backup_id);
                }
            }
        }

        Ok(entries)
    }

    async fn get_size(&self, backup_id: &str) -> Result<u64, StorageError> {
        let path = self.get_path(backup_id);

        if !path.exists() {
            return Err(StorageError::NotFound {
                backup_id: backup_id.to_string(),
            });
        }

        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|e| StorageError::IoError {
                message: format!("Failed to get backup size: {}", e),
            })?;

        Ok(metadata.len())
    }

    async fn exists(&self, backup_id: &str) -> Result<bool, StorageError> {
        let path = self.get_path(backup_id);
        Ok(path.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_storage_store_retrieve() {
        let tmpdir = TempDir::new().unwrap();
        let storage = LocalFileStorage::new(tmpdir.path()).unwrap();

        let data = b"test backup data";
        let size = storage.store("test-backup-1", data).await.unwrap();
        assert_eq!(size, data.len() as u64);

        let retrieved = storage.retrieve("test-backup-1").await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_local_storage_delete() {
        let tmpdir = TempDir::new().unwrap();
        let storage = LocalFileStorage::new(tmpdir.path()).unwrap();

        storage.store("test-backup-2", b"data").await.unwrap();
        assert!(storage.exists("test-backup-2").await.unwrap());

        storage.delete("test-backup-2").await.unwrap();
        assert!(!storage.exists("test-backup-2").await.unwrap());
    }

    #[tokio::test]
    async fn test_local_storage_list() {
        let tmpdir = TempDir::new().unwrap();
        let storage = LocalFileStorage::new(tmpdir.path()).unwrap();

        storage.store("backup-1", b"data1").await.unwrap();
        storage.store("backup-2", b"data2").await.unwrap();

        let list = storage.list().await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"backup-1".to_string()));
        assert!(list.contains(&"backup-2".to_string()));
    }
}
