//! Local filesystem storage backend.

use std::{path::PathBuf, time::Duration};

use fraiseql_error::{FileError, FraiseQLError, Result};

use super::{
    types::{ListResult, ObjectInfo},
    validate_key,
};

/// Stores files on the local filesystem under a root directory.
pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    /// Creates a new local storage backend rooted at `root`.
    #[must_use]
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    /// Resolve a storage key to its path under the backend root.
    ///
    /// Purely lexical: [`validate_key`] has already rejected every key shape
    /// that could name a path outside the root or alias onto another key, so
    /// the join below cannot escape. Symlink containment is checked separately,
    /// against the filesystem, by [`Self::ensure_contained`].
    pub(crate) fn key_path(&self, key: &str) -> Result<PathBuf> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }

    /// Reject a resolved path that leaves the backend root, or whose final
    /// component is a symlink.
    ///
    /// [`validate_key`] guarantees the *lexical* join stays inside the root, but
    /// it cannot see the filesystem: a directory symlink planted under the root
    /// redirects the whole subtree, and a symlinked leaf redirects a single
    /// object — in both cases `tokio::fs` follows the link and reads or writes
    /// a file the storage system does not own. Resolving the parent and
    /// comparing it against the resolved root closes the first; refusing a
    /// symlinked leaf closes the second (`fs::write` writes *through* a link,
    /// so checking the parent alone is not enough).
    ///
    /// Called on every I/O path, after any directory creation so the parent
    /// exists to be resolved.
    async fn ensure_contained(&self, path: &std::path::Path) -> Result<()> {
        let Some(parent) = path.parent() else {
            return Err(FraiseQLError::File(FileError::InvalidKey {
                message: "Invalid storage key: resolves outside the storage root".to_string(),
            }));
        };

        let root = tokio::fs::canonicalize(&self.root).await.map_err(|e| {
            FraiseQLError::File(FileError::IoError {
                message: format!("Failed to resolve the storage root: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;
        let parent = tokio::fs::canonicalize(parent).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FraiseQLError::File(FileError::NotFound {
                    id: path.to_string_lossy().into_owned(),
                })
            } else {
                FraiseQLError::File(FileError::IoError {
                    message: format!("Failed to resolve the object path: {e}"),
                    source:  Some(Box::new(e)),
                })
            }
        })?;

        if !parent.starts_with(&root) {
            return Err(FraiseQLError::File(FileError::InvalidKey {
                message: "Invalid storage key: resolves outside the storage root".to_string(),
            }));
        }

        // The leaf itself: `symlink_metadata` does not follow the link, so a
        // present-and-symlinked object is refused rather than followed.
        match tokio::fs::symlink_metadata(path).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                Err(FraiseQLError::File(FileError::InvalidKey {
                    message: "Invalid storage key: the object path is a symlink".to_string(),
                }))
            },
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(FraiseQLError::File(FileError::IoError {
                message: format!("Failed to inspect the object path: {e}"),
                source:  Some(Box::new(e)),
            })),
        }
    }

    /// Reject a resolved directory that leaves the backend root.
    ///
    /// The listing path joins a caller-supplied `prefix` rather than a key, so
    /// it cannot reuse [`Self::ensure_contained`] (which checks a *parent* and
    /// a leaf). Same guarantee, one level up.
    async fn ensure_contained_dir(&self, dir: &std::path::Path) -> Result<()> {
        let root = tokio::fs::canonicalize(&self.root).await.map_err(|e| {
            FraiseQLError::File(FileError::IoError {
                message: format!("Failed to resolve the storage root: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;
        let resolved = tokio::fs::canonicalize(dir).await.map_err(|e| {
            FraiseQLError::File(FileError::IoError {
                message: format!("Failed to resolve the listing prefix: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;
        if resolved.starts_with(&root) {
            Ok(())
        } else {
            Err(FraiseQLError::File(FileError::InvalidKey {
                message: "Invalid listing prefix: resolves outside the storage root".to_string(),
            }))
        }
    }

    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::IoError)` if the upload fails.
    pub async fn upload(&self, key: &str, data: &[u8], _content_type: &str) -> Result<String> {
        let path = self.key_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                FraiseQLError::File(FileError::IoError {
                    message: format!("Failed to create directory: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;
        }
        self.ensure_contained(&path).await?;
        tokio::fs::write(&path, data).await.map_err(|e| {
            FraiseQLError::File(FileError::IoError {
                message: format!("Failed to write file: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;
        Ok(key.to_string())
    }

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::NotFound)` if the key does not exist,
    /// or `FileError::IoError` on backend failures.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.key_path(key)?;
        self.ensure_contained(&path).await?;
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FraiseQLError::File(FileError::NotFound {
                    id: key.to_string(),
                })
            } else {
                FraiseQLError::File(FileError::IoError {
                    message: format!("Failed to read file: {e}"),
                    source:  Some(Box::new(e)),
                })
            }
        })
    }

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failures.
    pub async fn delete(&self, key: &str) -> Result<()> {
        let path = self.key_path(key)?;
        self.ensure_contained(&path).await?;
        tokio::fs::remove_file(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FraiseQLError::File(FileError::NotFound {
                    id: key.to_string(),
                })
            } else {
                FraiseQLError::File(FileError::IoError {
                    message: format!("Failed to delete file: {e}"),
                    source:  Some(Box::new(e)),
                })
            }
        })
    }

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::IoError)` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.key_path(key)?;
        // A missing parent directory means the object does not exist — that is
        // `Ok(false)`, not an error, so the containment check's `NotFound` is
        // absorbed here rather than propagated.
        match self.ensure_contained(&path).await {
            Ok(()) => {},
            Err(FraiseQLError::File(FileError::NotFound { .. })) => return Ok(false),
            Err(e) => return Err(e),
        }
        match tokio::fs::metadata(&path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(FraiseQLError::File(FileError::IoError {
                message: format!("Failed to check file existence: {e}"),
                source:  Some(Box::new(e)),
            })),
        }
    }

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::Unsupported)` because presigned URLs
    /// are not supported by the local backend.
    pub async fn presigned_url(&self, _key: &str, _expiry: Duration) -> Result<String> {
        Err(FraiseQLError::File(FileError::Unsupported {
            message: "Presigned URLs are not supported for local storage".to_string(),
        }))
    }

    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::IoError)` on I/O failures.
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
                objects:     Vec::new(),
                next_cursor: None,
            });
        }

        // `prefix` is caller-supplied and is joined onto the root without going
        // through `key_path` (a prefix legitimately ends in `/`, which
        // `validate_key` rejects). Resolve it and require containment so a
        // `..` prefix — or a symlinked directory under the root — cannot list
        // outside the backend.
        self.ensure_contained_dir(&prefix_path).await?;

        // Walk the directory and collect matching files
        for entry in walkdir::WalkDir::new(&prefix_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let full_path = entry.path();
            let relative_path = full_path
                .strip_prefix(&self.root)
                .map_err(|_| {
                    FraiseQLError::File(FileError::IoError {
                        message: "Failed to compute relative path".to_string(),
                        source:  None,
                    })
                })?
                .to_string_lossy()
                .into_owned();

            // Normalize path separators to forward slashes
            let key = relative_path.replace('\\', "/");

            // Get file metadata
            let metadata = tokio::fs::metadata(full_path).await.map_err(|e| {
                FraiseQLError::File(FileError::IoError {
                    message: format!("Failed to read file metadata: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;

            let size = metadata.len();
            let last_modified = metadata
                .modified()
                .ok()
                .and_then(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                    // Reason: u64→i64 cast for chrono timestamp; only wraps after year
                    // 292277026596.
                    #[allow(clippy::cast_possible_wrap)]
                    let secs = duration.as_secs() as i64;
                    chrono::DateTime::from_timestamp(secs, duration.subsec_nanos())
                })
                .map_or_else(|| chrono::Utc::now().to_rfc3339(), |dt| dt.to_rfc3339());

            // Generate simple etag from size and mtime
            let etag = format!("{:x}", fnv1a_hash(&format!("{size}-{last_modified}")));

            objects.push((
                key.clone(),
                ObjectInfo {
                    key,
                    size,
                    content_type: "application/octet-stream".to_string(), /* Default for local
                                                                           * storage */
                    etag,
                    last_modified,
                },
            ));
        }

        // Sort by key
        objects.sort_by(|a, b| a.0.cmp(&b.0));

        // Apply cursor pagination
        let start_idx = if let Some(c) = cursor {
            objects.iter().position(|(k, _)| k == c).map_or(0, |i| i + 1)
        } else {
            0
        };

        let end_idx = (start_idx + limit).min(objects.len());
        // `start_idx <= objects.len()` (sourced from `.position()` or `0`) and
        // `end_idx <= objects.len()` by the `.min()` above, so this slice is
        // always in-bounds; fall back to an empty page if the invariant ever
        // breaks rather than panicking.
        let page: Vec<ObjectInfo> = objects
            .get(start_idx..end_idx)
            .unwrap_or(&[])
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
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
