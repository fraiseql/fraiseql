//! Local filesystem storage backend

use std::{collections::HashMap, path::PathBuf, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::fs;

use crate::files::{
    config::StorageConfig,
    error::StorageError,
    traits::{StorageBackend, StorageMetadata, StorageResult},
};

pub struct LocalStorage {
    base_path: PathBuf,
    serve_url: String,
}

impl LocalStorage {
    pub fn new(config: &StorageConfig) -> Result<Self, StorageError> {
        let base_path = config
            .base_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./uploads"));

        let serve_url = config.serve_path.clone().unwrap_or_else(|| "/files".to_string());

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&base_path).map_err(|e| StorageError::Configuration {
            message: format!("Failed to create upload directory: {}", e),
        })?;

        Ok(Self {
            base_path,
            serve_url,
        })
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    fn name(&self) -> &'static str {
        "local"
    }

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        _content_type: &str,
        _metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError> {
        let path = self.base_path.join(key);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| StorageError::UploadFailed {
                message: e.to_string(),
            })?;
        }

        fs::write(&path, &data).await.map_err(|e| StorageError::UploadFailed {
            message: e.to_string(),
        })?;

        Ok(StorageResult {
            key:  key.to_string(),
            url:  self.public_url(key),
            etag: None,
            size: data.len() as u64,
        })
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        let path = self.base_path.join(key);

        let data = fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound {
                    key: key.to_string(),
                }
            } else {
                StorageError::DownloadFailed {
                    message: e.to_string(),
                }
            }
        })?;

        Ok(Bytes::from(data))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.base_path.join(key);

        fs::remove_file(&path).await.map_err(|e| StorageError::Provider {
            message: e.to_string(),
        })?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.base_path.join(key);
        Ok(path.exists())
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        let path = self.base_path.join(key);

        let meta = fs::metadata(&path).await.map_err(|e| StorageError::Provider {
            message: e.to_string(),
        })?;

        let last_modified = meta.modified().ok().and_then(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
            chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
        });

        Ok(StorageMetadata {
            content_type: mime_guess::from_path(&path).first_or_octet_stream().to_string(),
            content_length: meta.len(),
            etag: None,
            last_modified,
            custom: HashMap::new(),
        })
    }

    async fn signed_url(&self, key: &str, _expiry: Duration) -> Result<String, StorageError> {
        // Local storage doesn't support signed URLs in production
        // For dev, just return the public URL
        Ok(self.public_url(key))
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.serve_url, key)
    }
}
