//! AWS S3 (and S3-compatible) storage backend.
//!
//! Also supports Cloudflare R2 and `MinIO` via the `endpoint` configuration option.

use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::{Client, primitives::ByteStream};
use fraiseql_error::FileError;

use super::{StorageBackend, StorageResult, validate_key};

/// Stores files in an AWS S3 bucket (or S3-compatible service).
pub struct S3StorageBackend {
    client: Client,
    bucket: String,
}

impl S3StorageBackend {
    /// Creates a new S3 storage backend.
    ///
    /// Authentication uses standard AWS credential resolution (environment
    /// variables, shared credentials file, instance profile, etc.).
    ///
    /// Set `endpoint` for S3-compatible services like Cloudflare R2 or `MinIO`.
    pub async fn new(bucket: &str, region: Option<&str>, endpoint: Option<&str>) -> Self {
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(r) = region {
            config_loader = config_loader.region(aws_config::Region::new(r.to_owned()));
        }
        let config = config_loader.load().await;

        let client = if let Some(ep) = endpoint {
            let s3_config = aws_sdk_s3::config::Builder::from(&config)
                .endpoint_url(ep)
                .force_path_style(true)
                .build();
            Client::from_conf(s3_config)
        } else {
            Client::new(&config)
        };

        Self {
            client,
            bucket: bucket.to_owned(),
        }
    }
}

fn storage_err(op: &str, err: impl std::fmt::Display) -> FileError {
    FileError::Storage {
        message: format!("S3 {op} failed: {err}"),
        source: None,
    }
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> StorageResult<String> {
        validate_key(key)?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data.to_vec()))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| storage_err("put_object", e))?;
        Ok(key.to_owned())
    }

    async fn download(&self, key: &str) -> StorageResult<Vec<u8>> {
        validate_key(key)?;
        let resp =
            self.client
                .get_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("NoSuchKey") || msg.contains("404") {
                        FileError::NotFound {
                            id: key.to_string(),
                        }
                    } else {
                        storage_err("get_object", e)
                    }
                })?;

        let body = resp.body.collect().await.map_err(|e| storage_err("get_object body", e))?;
        Ok(body.into_bytes().to_vec())
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        validate_key(key)?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| storage_err("delete_object", e))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> StorageResult<bool> {
        validate_key(key)?;
        match self.client.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(_) => Ok(true),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("NotFound") || msg.contains("NoSuchKey") || msg.contains("404") {
                    Ok(false)
                } else {
                    Err(storage_err("head_object", err))
                }
            },
        }
    }

    async fn presigned_url(&self, key: &str, expiry: Duration) -> StorageResult<String> {
        validate_key(key)?;
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expiry)
            .map_err(|e| storage_err("presigning config", e))?;
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| storage_err("presigned URL", e))?;
        Ok(presigned.uri().to_string())
    }
}
