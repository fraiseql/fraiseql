//! Google Cloud Storage backend.
//!
//! Authentication is resolved in order:
//! 1. `GOOGLE_CLOUD_TOKEN` env var — static bearer token (simplest; suitable for short-lived tasks)
//! 2. `GOOGLE_APPLICATION_CREDENTIALS` env var — path to a service account JSON file (tokens are
//!    auto-refreshed via JWT exchange)

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use fraiseql_error::FileError;
use parking_lot::RwLock;

use super::{StorageBackend, StorageResult, validate_key};

const GCS_API_BASE: &str = "https://storage.googleapis.com";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/devstorage.full_control";

/// Stores files in a Google Cloud Storage bucket.
pub struct GcsStorageBackend {
    bucket: String,
    auth:   GcsAuth,
    client: reqwest::Client,
}

enum GcsAuth {
    /// Static bearer token from `GOOGLE_CLOUD_TOKEN`.
    BearerToken(String),
    /// Service account credentials with automatic token refresh.
    ServiceAccount {
        client_email: String,
        private_key:  String,
        token:        RwLock<Option<(String, Instant)>>,
    },
}

impl GcsStorageBackend {
    /// Creates a new GCS backend for the given bucket.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::Storage`] if neither `GOOGLE_CLOUD_TOKEN` nor
    /// `GOOGLE_APPLICATION_CREDENTIALS` is set, or if the credentials file is
    /// unreadable or malformed.
    pub fn new(bucket: &str) -> StorageResult<Self> {
        let auth = if let Ok(token) = std::env::var("GOOGLE_CLOUD_TOKEN") {
            GcsAuth::BearerToken(token)
        } else if let Ok(creds_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            let creds_json =
                std::fs::read_to_string(&creds_path).map_err(|e| FileError::Storage {
                    message: format!("Failed to read GCS credentials file '{creds_path}': {e}"),
                    source:  None,
                })?;
            let creds: serde_json::Value =
                serde_json::from_str(&creds_json).map_err(|e| FileError::Storage {
                    message: format!("Failed to parse GCS credentials JSON: {e}"),
                    source:  None,
                })?;
            let client_email = creds["client_email"]
                .as_str()
                .ok_or_else(|| FileError::Storage {
                    message: "GCS credentials missing 'client_email' field".to_string(),
                    source:  None,
                })?
                .to_owned();
            let private_key = creds["private_key"]
                .as_str()
                .ok_or_else(|| FileError::Storage {
                    message: "GCS credentials missing 'private_key' field".to_string(),
                    source:  None,
                })?
                .to_owned();
            GcsAuth::ServiceAccount {
                client_email,
                private_key,
                token: RwLock::new(None),
            }
        } else {
            return Err(FileError::Storage {
                message: "GCS authentication requires GOOGLE_CLOUD_TOKEN or \
                          GOOGLE_APPLICATION_CREDENTIALS environment variable"
                    .to_string(),
                source:  None,
            });
        };

        Ok(Self {
            bucket: bucket.to_owned(),
            auth,
            client: reqwest::Client::new(),
        })
    }

    /// Returns a valid access token, refreshing via JWT exchange if needed.
    async fn get_token(&self) -> StorageResult<String> {
        match &self.auth {
            GcsAuth::BearerToken(token) => Ok(token.clone()),
            GcsAuth::ServiceAccount {
                client_email,
                private_key,
                token,
            } => {
                // Check cached token
                if let Some((cached, expiry)) = token.read().as_ref() {
                    if Instant::now() < *expiry {
                        return Ok(cached.clone());
                    }
                }

                let jwt = create_gcs_jwt(client_email, private_key)?;
                let new_token = self.exchange_jwt(&jwt).await?;

                // Cache for ~58 minutes (tokens last 60 minutes)
                *token.write() =
                    Some((new_token.clone(), Instant::now() + Duration::from_secs(3500)));
                Ok(new_token)
            },
        }
    }

    async fn exchange_jwt(&self, jwt: &str) -> StorageResult<String> {
        let resp = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt),
            ])
            .send()
            .await
            .map_err(|e| FileError::Storage {
                message: format!("GCS token exchange request failed: {e}"),
                source:  None,
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(FileError::Storage {
                message: format!("GCS token exchange returned error: {body}"),
                source:  None,
            });
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| FileError::Storage {
            message: format!("Failed to parse GCS token response: {e}"),
            source:  None,
        })?;

        body["access_token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| FileError::Storage {
                message: "GCS token response missing 'access_token' field".to_string(),
                source:  None,
            })
    }
}

fn create_gcs_jwt(client_email: &str, private_key: &str) -> StorageResult<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();

    let claims = serde_json::json!({
        "iss": client_email,
        "scope": SCOPE,
        "aud": TOKEN_URL,
        "iat": now,
        "exp": now + 3600,
    });

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|e| {
        FileError::Storage {
            message: format!("Invalid GCS private key: {e}"),
            source:  None,
        }
    })?;

    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| FileError::Storage {
        message: format!("Failed to create GCS JWT: {e}"),
        source:  None,
    })
}

fn gcs_err(op: &str, err: impl std::fmt::Display) -> FileError {
    FileError::Storage {
        message: format!("GCS {op} failed: {err}"),
        source:  None,
    }
}

#[async_trait]
impl StorageBackend for GcsStorageBackend {
    async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> StorageResult<String> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let url = format!(
            "{GCS_API_BASE}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.bucket,
            urlencoding::encode(key)
        );

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| gcs_err("upload", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("upload response", body));
        }

        Ok(key.to_owned())
    }

    async fn download(&self, key: &str) -> StorageResult<Vec<u8>> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let url = format!(
            "{GCS_API_BASE}/storage/v1/b/{}/o/{}?alt=media",
            self.bucket,
            urlencoding::encode(key)
        );

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err("download", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            });
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("download response", body));
        }

        resp.bytes().await.map(|b| b.to_vec()).map_err(|e| gcs_err("download body", e))
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let url =
            format!("{GCS_API_BASE}/storage/v1/b/{}/o/{}", self.bucket, urlencoding::encode(key));

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err("delete", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            });
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("delete response", body));
        }

        Ok(())
    }

    async fn exists(&self, key: &str) -> StorageResult<bool> {
        validate_key(key)?;
        let token = self.get_token().await?;
        // Metadata-only request (no ?alt=media) to check existence.
        let url =
            format!("{GCS_API_BASE}/storage/v1/b/{}/o/{}", self.bucket, urlencoding::encode(key));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err("exists check", e))?;

        match resp.status() {
            s if s.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(gcs_err("exists check response", body))
            },
        }
    }

    async fn presigned_url(&self, _key: &str, _expiry: Duration) -> StorageResult<String> {
        // GCS V4 signed URLs require the service account private key and a
        // complex canonical-request construction.  This is planned but not yet
        // implemented — use the `gsutil signurl` CLI or GCS client libraries
        // for presigned URL generation in the meantime.
        Err(FileError::Storage {
            message: "Presigned URLs for GCS require V4 signing (not yet implemented)".to_string(),
            source:  None,
        })
    }
}
