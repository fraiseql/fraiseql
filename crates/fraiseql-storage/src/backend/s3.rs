//! AWS S3 (and S3-compatible) storage backend.
//!
//! Also supports Hetzner, Scaleway, OVH, Exoscale, Backblaze B2, and Cloudflare R2
//! via the `endpoint` configuration option.

use std::time::Duration;

use aws_sdk_s3::{Client, primitives::ByteStream};
use fraiseql_error::{FraiseQLError, Result};

use super::validate_key;

/// Stores files in an AWS S3 bucket or S3-compatible service.
pub struct S3Backend {
    client: Client,
    bucket: String,
}

impl S3Backend {
    /// Creates a new S3 storage backend.
    ///
    /// Authentication uses standard AWS credential resolution (environment
    /// variables, shared credentials file, instance profile, etc.).
    ///
    /// Set `endpoint` for S3-compatible services like Hetzner, Scaleway, OVH,
    /// Exoscale, Backblaze B2, Cloudflare R2, or MinIO.
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

fn storage_err(op: &str, err: impl std::fmt::Display) -> FraiseQLError {
    FraiseQLError::Storage {
        message: format!("S3 {op} failed: {err}"),
        code:  None,
    }
}

impl S3Backend {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the upload fails.
    pub async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> Result<String> {
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

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` with code "not_found" if the key does not exist,
    /// or other error codes on backend failures.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        validate_key(key)?;
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("NoSuchKey") || msg.contains("404") {
                    FraiseQLError::Storage {
                        message: format!("File not found: {key}"),
                        code: Some("not_found".to_string()),
                    }
                } else {
                    storage_err("get_object", e)
                }
            })?;

        let body = resp.body.collect().await.map_err(|e| storage_err("get_object body", e))?;
        Ok(body.into_bytes().to_vec())
    }

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend failures.
    pub async fn delete(&self, key: &str) -> Result<()> {
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

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_key(key)?;
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("NotFound") || msg.contains("NoSuchKey") || msg.contains("404") {
                    Ok(false)
                } else {
                    Err(storage_err("head_object", err))
                }
            }
        }
    }

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if presigned URL generation fails.
    pub async fn presigned_url(&self, key: &str, expiry: Duration) -> Result<String> {
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
