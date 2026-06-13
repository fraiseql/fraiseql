//! AWS S3 (and S3-compatible) storage backend.
//!
//! Also supports Hetzner, Scaleway, OVH, Exoscale, Backblaze B2, and Cloudflare R2
//! via the `endpoint` configuration option.

use std::time::Duration;

use aws_sdk_s3::{Client, primitives::ByteStream};
use fraiseql_error::{FileError, FraiseQLError, Result};

use super::validate_key;

#[cfg(test)]
mod tests;

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
    /// Exoscale, Backblaze B2, Cloudflare R2, or `MinIO`.
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

/// Build a `FileError::Backend` from an AWS SDK error, preserving the source chain.
fn storage_err_src(op: &str, err: impl std::error::Error + Send + Sync + 'static) -> FraiseQLError {
    let message = format!("S3 {op} failed: {err}");
    FraiseQLError::File(FileError::Backend {
        message,
        source: Some(Box::new(err)),
    })
}

impl S3Backend {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the upload fails.
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
            .map_err(|e| storage_err_src("put_object", e))?;
        Ok(key.to_owned())
    }

    /// Downloads the contents of the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` with code `not_found` if the key does not exist,
    /// or other error codes on backend failures.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        validate_key(key)?;
        let resp =
            self.client
                .get_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| {
                    // A missing key is a typed `NoSuchKey` service error. The
                    // `SdkError` Display does not contain the code (it is just
                    // "service error"), so detect it structurally on the typed
                    // error rather than by string-matching (H40).
                    if e.as_service_error().is_some_and(
                        aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key,
                    ) {
                        FraiseQLError::File(FileError::NotFound {
                            id: key.to_string(),
                        })
                    } else {
                        storage_err_src("get_object", e)
                    }
                })?;

        let body = resp.body.collect().await.map_err(|e| storage_err_src("get_object body", e))?;
        Ok(body.into_bytes().to_vec())
    }

    /// Deletes the object at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failures.
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_key(key)?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| storage_err_src("delete_object", e))?;
        Ok(())
    }

    /// Checks whether an object exists at the given key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_key(key)?;
        match self.client.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(_) => Ok(true),
            Err(err) => {
                // A missing object is a typed `NotFound` on the head_object
                // error; detect it structurally rather than by string-matching
                // the `SdkError` Display (H40).
                if err
                    .as_service_error()
                    .is_some_and(aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found)
                {
                    Ok(false)
                } else {
                    Err(storage_err_src("head_object", err))
                }
            },
        }
    }

    /// Generates a presigned (time-limited) URL for direct access to an object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if presigned URL generation fails.
    pub async fn presigned_url(&self, key: &str, expiry: Duration) -> Result<String> {
        validate_key(key)?;
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expiry)
            .map_err(|e| storage_err_src("presigning config", e))?;
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| storage_err_src("presigned URL", e))?;
        Ok(presigned.uri().to_string())
    }

    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failures.
    pub async fn list(
        &self,
        prefix: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<super::types::ListResult> {
        let mut objects = Vec::new();
        let continuation_token = cursor.map(|s| s.to_string());

        // Use list_objects_v2 with the provided cursor as continuation token
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            // Reason: AWS SDK's max_keys takes i32; limit is a u32 capped at S3's documented 1000.
            // Truncation/sign-wrap cannot occur in practice; the SDK itself clamps server-side.
            .max_keys(i32::try_from(limit).unwrap_or(i32::MAX))
            .set_continuation_token(continuation_token)
            .send()
            .await
            .map_err(|e| storage_err_src("list_objects_v2", e))?;

        for obj in resp.contents() {
            let key = obj.key().unwrap_or("").to_string();
            // Reason: object size is reported as i64 by the SDK but is non-negative per S3
            // contract.
            #[allow(clippy::cast_sign_loss)]
            let size = obj.size().unwrap_or(0) as u64;
            let etag = obj.e_tag().unwrap_or("").to_string();
            let last_modified = obj
                .last_modified()
                .map_or_else(|| chrono::Utc::now().to_rfc3339(), |dt| dt.to_string());

            objects.push(super::types::ObjectInfo {
                key,
                size,
                content_type: "application/octet-stream".to_string(),
                etag,
                last_modified,
            });
        }

        let next_cursor =
            resp.next_continuation_token().filter(|t| !t.is_empty()).map(|t| t.to_string());

        Ok(super::types::ListResult {
            objects,
            next_cursor,
        })
    }
}

/// Implementation of `PresignCapable` for `S3Backend`.
///
/// Enables time-limited direct access URLs for S3 objects, allowing clients
/// to upload/download without going through the `FraiseQL` server.
impl super::PresignCapable for S3Backend {
    async fn presign_put(
        &self,
        key: &str,
        content_type: &str,
        expires_in: Duration,
    ) -> Result<super::PresignedUrl> {
        validate_key(key)?;

        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| storage_err_src("presigning config", e))?;

        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(presigning_config)
            .await
            .map_err(|e| storage_err_src("presigned PUT URL", e))?;

        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(expires_in)
                .map_err(|e| storage_err_src("duration conversion", e))?;

        Ok(super::PresignedUrl::new(presigned.uri().to_string(), expires_at, "PUT"))
    }

    async fn presign_get(&self, key: &str, expires_in: Duration) -> Result<super::PresignedUrl> {
        validate_key(key)?;

        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| storage_err_src("presigning config", e))?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| storage_err_src("presigned GET URL", e))?;

        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(expires_in)
                .map_err(|e| storage_err_src("duration conversion", e))?;

        Ok(super::PresignedUrl::new(presigned.uri().to_string(), expires_at, "GET"))
    }
}
