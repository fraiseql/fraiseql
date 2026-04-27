//! Local filesystem storage backend.

use std::{path::PathBuf, time::Duration};

use fraiseql_error::{FraiseQLError, Result};
use super::{validate_key, types::{ListResult, ObjectInfo}};

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

    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on I/O failures.
    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on I/O failures.
    pub async fn list(
        &self,
        prefix: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListResult> {
        // Walk the directory tree
        let mut objects = Vec::new();
        let prefix_path = self.root.join(prefix);

        // If prefix directory doesn't exist, return empty list
        if !prefix_path.exists() {
            return Ok(ListResult {
                objects: Vec::new(),
                next_cursor: None,
            });
        }

        // Walk the directory and collect matching files
        for entry in walkdir::WalkDir::new(&prefix_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let full_path = entry.path();
            let relative_path = full_path
                .strip_prefix(&self.root)
                .map_err(|_| FraiseQLError::Storage {
                    message: "Failed to compute relative path".to_string(),
                    code: Some("io_error".to_string()),
                })?
                .to_string_lossy()
                .into_owned();

            // Normalize path separators to forward slashes
            let key = relative_path.replace('\\', "/");

            // Get file metadata
            let metadata = tokio::fs::metadata(full_path)
                .await
                .map_err(|e| FraiseQLError::Storage {
                    message: format!("Failed to read file metadata: {e}"),
                    code: Some("io_error".to_string()),
                })?;

            let size = metadata.len();
            let last_modified = metadata
                .modified()
                .ok()
                .and_then(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                    chrono::DateTime::from_timestamp(
                        duration.as_secs() as i64,
                        duration.subsec_nanos(),
                    )
                })
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            // Generate simple etag from size and mtime
            let etag = format!("{:x}", fnv1a_hash(&format!("{}-{}", size, last_modified)));

            objects.push((
                key.clone(),
                ObjectInfo {
                    key,
                    size,
                    content_type: "application/octet-stream".to_string(), // Default for local storage
                    etag,
                    last_modified,
                },
            ));
        }

        // Sort by key
        objects.sort_by(|a, b| a.0.cmp(&b.0));

        // Apply cursor pagination
        let start_idx = if let Some(c) = cursor {
            objects.iter().position(|(k, _)| k == c).map(|i| i + 1).unwrap_or(0)
        } else {
            0
        };

        let end_idx = (start_idx + limit).min(objects.len());
        let page: Vec<ObjectInfo> = objects[start_idx..end_idx]
            .iter()
            .map(|(_, info)| info.clone())
            .collect();

        let next_cursor = if end_idx < objects.len() {
            page.last().map(|o| o.key.clone())
        } else {
            None
        };

        Ok(ListResult {
            objects: page,
            next_cursor,
        })
    }
}

/// Simple FNV-1a hash function
fn fnv1a_hash(data: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
