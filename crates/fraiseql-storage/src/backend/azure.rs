//! Azure Blob Storage backend.
//!
//! Authentication uses the `SharedKey` scheme: the storage account key is read
//! from the `AZURE_STORAGE_KEY` environment variable (base64-encoded).

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use fraiseql_error::{FileError, FraiseQLError, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::validate_key;

const AZURE_API_VERSION: &str = "2023-11-03";

/// Stores files in an Azure Blob Storage container.
pub struct AzureBackend {
    account:     String,
    container:   String,
    account_key: Vec<u8>,
    client:      reqwest::Client,
}

impl AzureBackend {
    /// Creates a new Azure Blob storage backend.
    ///
    /// The storage account key is read from `AZURE_STORAGE_KEY` (base64).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Storage`] if `AZURE_STORAGE_KEY` is not set or is
    /// not valid base64.
    pub fn new(account: &str, container: &str) -> Result<Self> {
        let key_b64 = std::env::var("AZURE_STORAGE_KEY").map_err(|_| FraiseQLError::Storage {
            message: "Azure Blob storage requires AZURE_STORAGE_KEY environment variable"
                .to_string(),
            code:  None,
        })?;
        let account_key =
            general_purpose::STANDARD.decode(&key_b64).map_err(|e| FraiseQLError::Storage {
                message: format!("Invalid AZURE_STORAGE_KEY (not valid base64): {e}"),
                code:  None,
            })?;

        Ok(Self {
            account: account.to_owned(),
            container: container.to_owned(),
            account_key,
            client: reqwest::Client::new(),
        })
    }

    fn blob_url(&self, key: &str) -> String {
        format!("https://{}.blob.core.windows.net/{}/{}", self.account, self.container, key)
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
        let canonicalized_resource = format!("/{}/{}/{}", self.account, self.container, key);

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

fn azure_err(op: &str, detail: impl std::fmt::Display) -> FraiseQLError {
    FraiseQLError::Storage {
        message: format!("Azure Blob {op} failed: {detail}"),
        code:  None,
    }
}

impl AzureBackend {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` if the upload fails.
    pub async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> Result<String> {
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

    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
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
            return Err(FileError::NotFound { id: key.to_string() }.into());
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

    pub async fn delete(&self, key: &str) -> Result<()> {
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
            return Err(FileError::NotFound { id: key.to_string() }.into());
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("delete response", body));
        }

        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
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
            _ => Err(azure_err("exists check response", resp.status().to_string())),
        }
    }

    pub async fn presigned_url(&self, _key: &str, _expiry: Duration) -> Result<String> {
        // Azure presigned access uses SAS (Shared Access Signature) tokens,
        // which require HMAC signing of the resource path, permissions, and
        // expiry.  This is planned but not yet implemented — use the Azure CLI
        // (`az storage blob generate-sas`) in the meantime.
        Err(FraiseQLError::Storage {
            message:
                "Presigned URLs for Azure Blob require SAS token generation (not yet implemented)"
                    .to_string(),
            code:  None,
        })
    }

    /// Lists objects in the container by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Storage` with code "not_implemented" since list
    /// is not yet implemented for Azure Blob.
    pub async fn list(
        &self,
        _prefix: &str,
        _cursor: Option<&str>,
        _limit: usize,
    ) -> Result<super::types::ListResult> {
        Err(FraiseQLError::Storage {
            message: "list not yet implemented for Azure Blob".to_string(),
            code: Some("not_implemented".to_string()),
        })
    }
}
