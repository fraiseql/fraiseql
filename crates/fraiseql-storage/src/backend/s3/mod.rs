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

/// Decode the multipart continuation state a session persisted. Corrupt state
/// is a loud backend error — an upload cannot continue against parts we cannot
/// name.
fn parse_multipart_state(state: &serde_json::Value) -> Result<(String, Vec<String>)> {
    let upload_id = state
        .get("upload_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            FraiseQLError::File(FileError::Backend {
                message: "multipart continuation state has no upload_id".to_string(),
                source:  None,
            })
        })?
        .to_string();
    let etags = state
        .get("etags")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            FraiseQLError::File(FileError::Backend {
                message: "multipart continuation state has no etags array".to_string(),
                source:  None,
            })
        })?
        .iter()
        .map(|v| {
            v.as_str().map(str::to_string).ok_or_else(|| {
                FraiseQLError::File(FileError::Backend {
                    message: "multipart continuation state has a non-string etag".to_string(),
                    source:  None,
                })
            })
        })
        .collect::<Result<Vec<String>>>()?;
    Ok((upload_id, etags))
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

    /// Begin an S3 multipart upload for a resumable session (#369). Returns
    /// the continuation state (`upload_id` + accepted part etags) the session
    /// persists between chunks.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure.
    pub async fn multipart_begin(
        &self,
        key: &str,
        content_type: &str,
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let created = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| storage_err_src("create_multipart_upload", e))?;
        let upload_id = created.upload_id().ok_or_else(|| {
            FraiseQLError::File(FileError::Backend {
                message: "S3 create_multipart_upload returned no upload id".to_string(),
                source:  None,
            })
        })?;
        Ok(serde_json::json!({ "upload_id": upload_id, "etags": [] }))
    }

    /// Upload one chunk as the next S3 part. Returns the updated continuation
    /// state with the part's etag appended.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure or corrupted
    /// continuation state.
    pub async fn multipart_append(
        &self,
        key: &str,
        state: serde_json::Value,
        data: &[u8],
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let (upload_id, mut etags) = parse_multipart_state(&state)?;
        // S3 part numbers are 1-based; the accepted-part count is the index.
        let part_number = i32::try_from(etags.len() + 1).map_err(|_| {
            FraiseQLError::File(FileError::Backend {
                message: "S3 multipart upload exceeded the maximum part count".to_string(),
                source:  None,
            })
        })?;
        let part = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .map_err(|e| storage_err_src("upload_part", e))?;
        let etag = part.e_tag().ok_or_else(|| {
            FraiseQLError::File(FileError::Backend {
                message: "S3 upload_part returned no etag".to_string(),
                source:  None,
            })
        })?;
        etags.push(etag.to_string());
        Ok(serde_json::json!({ "upload_id": upload_id, "etags": etags }))
    }

    /// Complete an S3 multipart upload. Returns the assembled object's etag.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure or corrupted
    /// continuation state.
    pub async fn multipart_complete(&self, key: &str, state: &serde_json::Value) -> Result<String> {
        validate_key(key)?;
        let (upload_id, etags) = parse_multipart_state(state)?;
        let parts: Vec<aws_sdk_s3::types::CompletedPart> = etags
            .iter()
            .enumerate()
            .map(|(i, etag)| {
                // The part count was bounded by `i32::try_from` at append
                // time, so `i + 1` always fits; the saturation is unreachable.
                let part_number = i32::try_from(i + 1).unwrap_or(i32::MAX);
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(etag)
                    .build()
            })
            .collect();
        let completed = self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(&upload_id)
            .multipart_upload(
                aws_sdk_s3::types::CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| storage_err_src("complete_multipart_upload", e))?;
        Ok(completed.e_tag().unwrap_or_default().to_string())
    }

    /// Abort an S3 multipart upload, discarding its stored parts.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure or corrupted
    /// continuation state.
    pub async fn multipart_abort(&self, key: &str, state: &serde_json::Value) -> Result<()> {
        validate_key(key)?;
        let (upload_id, _) = parse_multipart_state(state)?;
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(&upload_id)
            .send()
            .await
            .map_err(|e| storage_err_src("abort_multipart_upload", e))?;
        Ok(())
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
