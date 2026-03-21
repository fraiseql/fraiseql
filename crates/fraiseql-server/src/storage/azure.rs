//! Azure Blob Storage backend.
//!
//! Authentication uses the SharedKey scheme: the storage account key is read
//! from the `AZURE_STORAGE_KEY` environment variable (base64-encoded).

use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use fraiseql_error::FileError;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::{validate_key, StorageBackend, StorageResult};

const AZURE_API_VERSION: &str = "2023-11-03";

/// Stores files in an Azure Blob Storage container.
pub struct AzureBlobStorageBackend {
    account:     String,
    container:   String,
    account_key: Vec<u8>,
    client:      reqwest::Client,
}

impl AzureBlobStorageBackend {
    /// Creates a new Azure Blob storage backend.
    ///
    /// The storage account key is read from `AZURE_STORAGE_KEY` (base64).
    ///
    /// # Errors
    ///
    /// Returns [`FileError::Storage`] if `AZURE_STORAGE_KEY` is not set or is
    /// not valid base64.
    pub fn new(account: &str, container: &str) -> StorageResult<Self> {
        let key_b64 = std::env::var("AZURE_STORAGE_KEY").map_err(|_| FileError::Storage {
            message: "Azure Blob storage requires AZURE_STORAGE_KEY environment variable"
                .to_string(),
            source:  None,
        })?;
        let account_key =
            general_purpose::STANDARD
                .decode(&key_b64)
                .map_err(|e| FileError::Storage {
                    message: format!("Invalid AZURE_STORAGE_KEY (not valid base64): {e}"),
                    source:  None,
                })?;

        Ok(Self {
            account:   account.to_owned(),
            container: container.to_owned(),
            account_key,
            client: reqwest::Client::new(),
        })
    }

    fn blob_url(&self, key: &str) -> String {
        format!(
            "https://{}.blob.core.windows.net/{}/{}",
            self.account, self.container, key
        )
    }

    /// Computes the `Authorization: SharedKey` header value.
    ///
    /// `extra_canonical_headers` should contain any `x-ms-*` headers (other
    /// than `x-ms-date` and `x-ms-version`) in sorted, `key:value\n` form.
    fn sign_request(
        &self,
        verb: &str,
        key: &str,
        content_type: &str,
        content_length: &str,
        date: &str,
        extra_canonical_headers: &str,
    ) -> String {
        let canonicalized_resource =
            format!("/{}/{}/{}", self.account, self.container, key);

        // Azure SharedKey string-to-sign (Blob service, 2023-11-03):
        // VERB\nContent-Encoding\nContent-Language\nContent-Length\nContent-MD5\n
        // Content-Type\nDate\nIf-Modified-Since\nIf-Match\nIf-None-Match\n
        // If-Unmodified-Since\nRange\nCanonicalizedHeaders\nCanonicalizedResource
        let string_to_sign = format!(
            "{verb}\n\n\n{content_length}\n\n{content_type}\n\n\n\n\n\n\n\
             {extra_canonical_headers}\
             x-ms-date:{date}\n\
             x-ms-version:{AZURE_API_VERSION}\n\
             {canonicalized_resource}"
        );

        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.account_key).expect("HMAC accepts any key length");
        mac.update(string_to_sign.as_bytes());
        let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        format!("SharedKey {}:{signature}", self.account)
    }
}

fn azure_err(op: &str, detail: impl std::fmt::Display) -> FileError {
    FileError::Storage {
        message: format!("Azure Blob {op} failed: {detail}"),
        source:  None,
    }
}

#[async_trait]
impl StorageBackend for AzureBlobStorageBackend {
    async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> StorageResult<String> {
        validate_key(key)?;
        let url = self.blob_url(key);
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let content_length = data.len().to_string();

        let auth = self.sign_request(
            "PUT",
            key,
            content_type,
            &content_length,
            &date,
            "x-ms-blob-type:BlockBlob\n",
        );

        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .header("x-ms-blob-type", "BlockBlob")
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| azure_err("upload", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("upload response", body));
        }

        Ok(key.to_owned())
    }

    async fn download(&self, key: &str) -> StorageResult<Vec<u8>> {
        validate_key(key)?;
        let url = self.blob_url(key);
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        let auth = self.sign_request("GET", key, "", "", &date, "");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .send()
            .await
            .map_err(|e| azure_err("download", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            });
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("download response", body));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| azure_err("download body", e))
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        validate_key(key)?;
        let url = self.blob_url(key);
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        let auth = self.sign_request("DELETE", key, "", "", &date, "");

        let resp = self
            .client
            .delete(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .send()
            .await
            .map_err(|e| azure_err("delete", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            });
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("delete response", body));
        }

        Ok(())
    }

    async fn exists(&self, key: &str) -> StorageResult<bool> {
        validate_key(key)?;
        let url = self.blob_url(key);
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        let auth = self.sign_request("HEAD", key, "", "", &date, "");

        let resp = self
            .client
            .head(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .send()
            .await
            .map_err(|e| azure_err("exists check", e))?;

        match resp.status() {
            s if s.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => Err(azure_err(
                "exists check response",
                resp.status().to_string(),
            )),
        }
    }

    async fn presigned_url(&self, _key: &str, _expiry: Duration) -> StorageResult<String> {
        // Azure presigned access uses SAS (Shared Access Signature) tokens,
        // which require HMAC signing of the resource path, permissions, and
        // expiry.  This is planned but not yet implemented — use the Azure CLI
        // (`az storage blob generate-sas`) in the meantime.
        Err(FileError::Storage {
            message: "Presigned URLs for Azure Blob require SAS token generation (not yet implemented)"
                .to_string(),
            source:  None,
        })
    }
}
