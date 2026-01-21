//! S3 storage backend (and S3-compatible services like R2, MinIO)

use async_trait::async_trait;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use bytes::Bytes;
use std::collections::HashMap;
use std::time::Duration;

use crate::config::StorageConfig;
use crate::error::StorageError;
use crate::traits::{StorageBackend, StorageMetadata, StorageResult};

pub struct S3Storage {
    client: Client,
    bucket: String,
    public_url: Option<String>,
}

impl S3Storage {
    pub async fn new(config: &StorageConfig) -> Result<Self, StorageError> {
        let region = config
            .region
            .as_ref()
            .map(|r| Region::new(r.clone()))
            .unwrap_or_else(|| Region::new("us-east-1"));

        let access_key = std::env::var(config.access_key_env.as_ref().ok_or_else(|| {
            StorageError::Configuration {
                message: "S3 access_key_env required".into(),
            }
        })?)
        .map_err(|_| StorageError::Configuration {
            message: "S3 access key not found in environment".into(),
        })?;

        let secret_key = std::env::var(config.secret_key_env.as_ref().ok_or_else(|| {
            StorageError::Configuration {
                message: "S3 secret_key_env required".into(),
            }
        })?)
        .map_err(|_| StorageError::Configuration {
            message: "S3 secret key not found in environment".into(),
        })?;

        let bucket = std::env::var(config.bucket_env.as_ref().ok_or_else(|| {
            StorageError::Configuration {
                message: "S3 bucket_env required".into(),
            }
        })?)
        .map_err(|_| StorageError::Configuration {
            message: "S3 bucket not found in environment".into(),
        })?;

        let credentials = Credentials::new(access_key, secret_key, None, None, "fraiseql");

        let mut sdk_config_builder = aws_config::from_env().region(region).credentials_provider(credentials);

        // Custom endpoint for S3-compatible services
        if let Some(endpoint_env) = &config.endpoint_env {
            if let Ok(endpoint) = std::env::var(endpoint_env) {
                sdk_config_builder = sdk_config_builder.endpoint_url(&endpoint);
            }
        }

        let sdk_config = sdk_config_builder.load().await;
        let client = Client::new(&sdk_config);

        Ok(Self {
            client,
            bucket,
            public_url: config.public_url.clone(),
        })
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    fn name(&self) -> &'static str {
        "s3"
    }

    async fn upload(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        metadata: Option<&StorageMetadata>,
    ) -> Result<StorageResult, StorageError> {
        let size = data.len() as u64;
        let mut req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .content_type(content_type);

        // Add custom metadata
        if let Some(meta) = metadata {
            for (k, v) in &meta.custom {
                req = req.metadata(k, v);
            }
        }

        let output = req.send().await.map_err(|e| StorageError::UploadFailed {
            message: e.to_string(),
        })?;

        Ok(StorageResult {
            key: key.to_string(),
            url: self.public_url(key),
            etag: output.e_tag().map(std::string::ToString::to_string),
            size,
        })
    }

    async fn download(&self, key: &str) -> Result<Bytes, StorageError> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                message: e.to_string(),
            })?;

        let data = output
            .body
            .collect()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                message: e.to_string(),
            })?;

        Ok(data.into_bytes())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string(),
            })?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("404") || error_str.contains("NotFound") {
                    Ok(false)
                } else {
                    Err(StorageError::Provider {
                        message: error_str,
                    })
                }
            }
        }
    }

    async fn metadata(&self, key: &str) -> Result<StorageMetadata, StorageError> {
        let output = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string(),
            })?;

        let last_modified = output.last_modified().and_then(|t| {
            chrono::DateTime::from_timestamp(t.secs(), 0)
        });

        Ok(StorageMetadata {
            content_type: output
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string(),
            content_length: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().map(std::string::ToString::to_string),
            last_modified,
            custom: output.metadata().cloned().unwrap_or_default(),
        })
    }

    async fn signed_url(&self, key: &str, expiry: Duration) -> Result<String, StorageError> {
        let presigning_config =
            PresigningConfig::expires_in(expiry).map_err(|e| StorageError::Provider {
                message: e.to_string(),
            })?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| StorageError::Provider {
                message: e.to_string(),
            })?;

        Ok(presigned.uri().to_string())
    }

    fn public_url(&self, key: &str) -> String {
        if let Some(url) = &self.public_url {
            format!("{}/{}", url.trim_end_matches('/'), key)
        } else {
            format!("https://{}.s3.amazonaws.com/{}", self.bucket, key)
        }
    }
}
