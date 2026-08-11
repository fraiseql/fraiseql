//! Google Cloud Storage backend.
//!
//! Authentication is resolved in order:
//! 1. `GOOGLE_CLOUD_TOKEN` env var — static bearer token (simplest; suitable for short-lived tasks)
//! 2. `GOOGLE_APPLICATION_CREDENTIALS` env var — path to a service account JSON file (tokens are
//!    auto-refreshed via JWT exchange)

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fraiseql_error::{FileError, FraiseQLError, Result};
use parking_lot::RwLock;
use reqwest::StatusCode;

use super::validate_key;

const GCS_DEFAULT_API_BASE: &str = "https://storage.googleapis.com";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/devstorage.full_control";

/// The status GCS answers a cancelled resumable session with. It is outside the
/// IANA registry, so `reqwest` has no constant for it.
const GCS_CLIENT_CLOSED_REQUEST: u16 = 499;

/// Smallest chunk unit a GCS resumable session accepts: every chunk but the
/// last must be a multiple of 256 `KiB`.
pub(super) const GCS_RESUMABLE_CHUNK_UNIT: u64 = 256 * 1024;

/// The continuation state a resumable session persists between chunks.
struct GcsSession {
    /// The session URI GCS returned from the `uploadType=resumable` POST.
    uri:    String,
    /// The object's declared size, fixed when the session was opened.
    total:  u64,
    /// Bytes GCS has accepted so far.
    offset: u64,
    /// The assembled object's etag, present once the final chunk landed.
    etag:   Option<String>,
}

impl GcsSession {
    /// Decode persisted continuation state. Corrupt state is a loud backend
    /// error — an upload cannot continue against a session we cannot name.
    fn parse(state: &serde_json::Value) -> Result<Self> {
        let uri = state
            .get("session_uri")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| gcs_err("resumable state", "no session_uri"))?
            .to_owned();
        let total = state
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| gcs_err("resumable state", "no total"))?;
        let offset = state
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| gcs_err("resumable state", "no offset"))?;
        let etag = state
            .get("etag")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .filter(|e| !e.is_empty());
        Ok(Self {
            uri,
            total,
            offset,
            etag,
        })
    }
}

/// Stores files in a Google Cloud Storage bucket.
pub struct GcsBackend {
    bucket:   String,
    auth:     GcsAuth,
    /// API base override (e.g. a fake-gcs-server emulator URL). `None` means
    /// the production `https://storage.googleapis.com` host is used.
    endpoint: Option<String>,
    client:   reqwest::Client,
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

impl GcsBackend {
    /// Creates a new GCS backend for the given bucket.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::Backend)` if neither
    /// `GOOGLE_CLOUD_TOKEN` nor `GOOGLE_APPLICATION_CREDENTIALS` is set, or
    /// if the credentials file is unreadable or malformed.
    pub fn new(bucket: &str) -> Result<Self> {
        Self::new_with_endpoint(bucket, None)
    }

    /// Creates a new GCS backend with an optional API base override.
    ///
    /// When `endpoint` is `None`, the production GCS host is used:
    /// `https://storage.googleapis.com`. When set, it is used as the API base
    /// for object operations — for the fake-gcs-server emulator this is
    /// typically `http://127.0.0.1:4443`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::Backend)` if neither
    /// `GOOGLE_CLOUD_TOKEN` nor `GOOGLE_APPLICATION_CREDENTIALS` is set, if the
    /// credentials file is unreadable or malformed, or if `endpoint` is set but
    /// is not a valid URL.
    pub fn new_with_endpoint(bucket: &str, endpoint: Option<&str>) -> Result<Self> {
        if let Some(ep) = endpoint {
            reqwest::Url::parse(ep).map_err(|e| {
                FraiseQLError::File(FileError::Backend {
                    message: format!("GCS endpoint is not a valid URL: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;
        }

        let auth = if let Ok(token) = std::env::var("GOOGLE_CLOUD_TOKEN") {
            GcsAuth::BearerToken(token)
        } else if let Ok(creds_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            let creds_json = std::fs::read_to_string(&creds_path).map_err(|e| {
                FraiseQLError::File(FileError::Backend {
                    message: format!("Failed to read GCS credentials file '{creds_path}': {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;
            let creds: serde_json::Value = serde_json::from_str(&creds_json).map_err(|e| {
                FraiseQLError::File(FileError::Backend {
                    message: format!("Failed to parse GCS credentials JSON: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;
            let client_email = creds
                .get("client_email")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    FraiseQLError::File(FileError::Backend {
                        message: "GCS credentials missing 'client_email' field".to_string(),
                        source:  None,
                    })
                })?
                .to_owned();
            let private_key = creds
                .get("private_key")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    FraiseQLError::File(FileError::Backend {
                        message: "GCS credentials missing 'private_key' field".to_string(),
                        source:  None,
                    })
                })?
                .to_owned();
            GcsAuth::ServiceAccount {
                client_email,
                private_key,
                token: RwLock::new(None),
            }
        } else {
            return Err(FraiseQLError::File(FileError::Backend {
                message: "GCS authentication requires GOOGLE_CLOUD_TOKEN or \
                          GOOGLE_APPLICATION_CREDENTIALS environment variable"
                    .to_string(),
                source:  None,
            }));
        };

        Ok(Self {
            bucket: bucket.to_owned(),
            auth,
            endpoint: endpoint.map(str::to_owned),
            client: reqwest::Client::new(),
        })
    }

    /// Returns the API base URL for object operations, honouring the
    /// configured `endpoint` override and falling back to the production host.
    fn api_base(&self) -> &str {
        self.endpoint
            .as_deref()
            .map_or(GCS_DEFAULT_API_BASE, |ep| ep.trim_end_matches('/'))
    }

    /// Returns a valid access token, refreshing via JWT exchange if needed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if JWT creation or token exchange fails.
    pub async fn get_token(&self) -> Result<String> {
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

    /// Exchanges a signed JWT for an `OAuth2` access token from Google.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::Backend)` if the HTTP request
    /// fails or the response is invalid.
    pub async fn exchange_jwt(&self, jwt: &str) -> Result<String> {
        let resp = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt),
            ])
            .send()
            .await
            .map_err(|e| {
                FraiseQLError::File(FileError::Backend {
                    message: format!("GCS token exchange request failed: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(FraiseQLError::File(FileError::Backend {
                message: format!("GCS token exchange returned error: {body}"),
                source:  None,
            }));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: format!("Failed to parse GCS token response: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;

        body.get("access_token")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                FraiseQLError::File(FileError::Backend {
                    message: "GCS token response missing 'access_token' field".to_string(),
                    source:  None,
                })
            })
    }
}

fn create_gcs_jwt(client_email: &str, private_key: &str) -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: format!("System clock is before the UNIX epoch: {e}"),
                source:  Some(Box::new(e)),
            })
        })?
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
        FraiseQLError::File(FileError::Backend {
            message: format!("Invalid GCS private key: {e}"),
            source:  Some(Box::new(e)),
        })
    })?;

    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
        FraiseQLError::File(FileError::Backend {
            message: format!("Failed to create GCS JWT: {e}"),
            source:  Some(Box::new(e)),
        })
    })
}

fn gcs_err(op: &str, err: impl std::fmt::Display) -> FraiseQLError {
    FraiseQLError::File(FileError::Backend {
        message: format!("GCS {op} failed: {err}"),
        source:  None,
    })
}

/// Like [`gcs_err`] but preserves the underlying error in the chain.
fn gcs_err_src(op: &str, err: impl std::error::Error + Send + Sync + 'static) -> FraiseQLError {
    let message = format!("GCS {op} failed: {err}");
    FraiseQLError::File(FileError::Backend {
        message,
        source: Some(Box::new(err)),
    })
}

impl GcsBackend {
    /// Uploads data to GCS and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the upload request fails.
    pub async fn upload(&self, key: &str, data: &[u8], content_type: &str) -> Result<String> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let base = self.api_base();
        let url = format!(
            "{base}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
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
            .map_err(|e| gcs_err_src("upload", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("upload response", body));
        }

        Ok(key.to_owned())
    }

    /// Downloads the contents of the given key from GCS.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the download fails or the key does not exist.
    pub async fn download(&self, key: &str) -> Result<Vec<u8>> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let base = self.api_base();
        let url =
            format!("{base}/storage/v1/b/{}/o/{}?alt=media", self.bucket, urlencoding::encode(key));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err_src("download", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            }
            .into());
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("download response", body));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| gcs_err_src("download body", e))
    }

    /// Deletes the object at the given key from GCS.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the delete fails or the key does not exist.
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let base = self.api_base();
        let url = format!("{base}/storage/v1/b/{}/o/{}", self.bucket, urlencoding::encode(key));

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err_src("delete", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            }
            .into());
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("delete response", body));
        }

        Ok(())
    }

    /// Checks whether an object exists at the given key in GCS.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend communication errors.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let base = self.api_base();
        // Metadata-only request (no ?alt=media) to check existence.
        let url = format!("{base}/storage/v1/b/{}/o/{}", self.bucket, urlencoding::encode(key));

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| gcs_err_src("exists check", e))?;

        match resp.status() {
            s if s.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(gcs_err("exists check response", body))
            },
        }
    }

    /// Open a GCS resumable-upload session for a resumable upload (#972).
    ///
    /// The returned continuation state carries the session URI GCS hands back,
    /// the declared total size, and the byte offset accepted so far. Every
    /// subsequent chunk is a `PUT` to that session URI.
    ///
    /// The total is declared up front because GCS finalises the object when it
    /// receives the chunk whose `Content-Range` reaches it — which is the only
    /// finalisation form GCS documents for chunked uploads, and it needs the
    /// size before the first chunk is sent.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the session cannot be opened or GCS
    /// answers without a `Location` header.
    pub async fn multipart_begin(
        &self,
        key: &str,
        content_type: &str,
        total_bytes: u64,
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let token = self.get_token().await?;
        let base = self.api_base();
        let url = format!(
            "{base}/upload/storage/v1/b/{}/o?uploadType=resumable&name={}",
            self.bucket,
            urlencoding::encode(key)
        );

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("X-Upload-Content-Type", content_type)
            .header("X-Upload-Content-Length", total_bytes.to_string())
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .send()
            .await
            .map_err(|e| gcs_err_src("resumable session start", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("resumable session start response", body));
        }

        let session_uri = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                gcs_err("resumable session start", "response carried no Location header")
            })?
            .to_owned();

        Ok(serde_json::json!({
            "session_uri": session_uri,
            "total": total_bytes,
            "offset": 0,
        }))
    }

    /// Send one chunk to an open resumable session. Returns the updated
    /// continuation state, carrying the assembled object's etag once the
    /// session's final chunk has been accepted.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure, on corrupted
    /// continuation state, or if the chunk would run past the declared total.
    pub async fn multipart_append(
        &self,
        key: &str,
        state: serde_json::Value,
        data: &[u8],
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let session = GcsSession::parse(&state)?;
        let len = data.len() as u64;
        let end = session.offset.checked_add(len).ok_or_else(|| {
            gcs_err("resumable append", "chunk offset overflowed the declared total")
        })?;
        if end > session.total {
            return Err(gcs_err(
                "resumable append",
                format!(
                    "chunk would carry the upload to {end} bytes, past the declared total of {}",
                    session.total
                ),
            ));
        }

        let range = format!("bytes {}-{}/{}", session.offset, end - 1, session.total);
        let resp = self
            .client
            .put(&session.uri)
            .header(reqwest::header::CONTENT_RANGE, range)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| gcs_err_src("resumable append", e))?;

        // 308 "Resume Incomplete" is GCS's success status for a non-final
        // chunk; 200/201 means that chunk completed the object.
        let status = resp.status();
        if status == StatusCode::PERMANENT_REDIRECT {
            return Ok(serde_json::json!({
                "session_uri": session.uri,
                "total": session.total,
                "offset": end,
            }));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(gcs_err("resumable append response", body));
        }

        let object: serde_json::Value =
            resp.json().await.map_err(|e| gcs_err_src("resumable append body", e))?;
        let etag = object
            .get("etag")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok(serde_json::json!({
            "session_uri": session.uri,
            "total": session.total,
            "offset": end,
            "etag": etag,
        }))
    }

    /// Finalise a resumable upload.
    ///
    /// GCS finalises the object when the session's last chunk lands, so there
    /// is no separate commit call: this reports the etag that append recorded.
    /// A state without one describes an upload GCS never finalised, which is a
    /// loud error rather than a silently missing object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the continuation state is corrupt or
    /// describes an upload that never reached its declared total.
    pub fn multipart_complete(&self, key: &str, state: &serde_json::Value) -> Result<String> {
        validate_key(key)?;
        let session = GcsSession::parse(state)?;
        session.etag.ok_or_else(|| {
            gcs_err(
                "resumable completion",
                format!(
                    "the session stopped at {} of {} bytes, so GCS never finalised the object",
                    session.offset, session.total
                ),
            )
        })
    }

    /// Cancel a resumable session and discard whatever it has staged.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure or corrupted
    /// continuation state.
    pub async fn multipart_abort(&self, key: &str, state: &serde_json::Value) -> Result<()> {
        validate_key(key)?;
        let session = GcsSession::parse(state)?;
        let resp = self
            .client
            .delete(&session.uri)
            .header(reqwest::header::CONTENT_LENGTH, "0")
            .send()
            .await
            .map_err(|e| gcs_err_src("resumable abort", e))?;

        // 499 is the status GCS documents for a cancelled resumable session;
        // 404 means it has already gone (an expired or twice-aborted session).
        let status = resp.status();
        if status.is_success()
            || status == StatusCode::NOT_FOUND
            || status.as_u16() == GCS_CLIENT_CLOSED_REQUEST
        {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        Err(gcs_err("resumable abort response", body))
    }

    /// Generates a presigned URL for direct access to a GCS object.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::NotImplemented)` as V4 signing
    /// is not yet implemented.
    pub async fn presigned_url(&self, _key: &str, _expiry: Duration) -> Result<String> {
        // GCS V4 signed URLs require the service account private key and a
        // complex canonical-request construction.  This is planned but not yet
        // implemented — use the `gsutil signurl` CLI or GCS client libraries
        // for presigned URL generation in the meantime.
        Err(FraiseQLError::File(FileError::NotImplemented {
            message: "Presigned URLs for GCS require V4 signing (not yet implemented)".to_string(),
        }))
    }

    /// Lists objects in the bucket by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::NotImplemented)` since list
    /// is not yet implemented for GCS.
    pub async fn list(
        &self,
        _prefix: &str,
        _cursor: Option<&str>,
        _limit: usize,
    ) -> Result<super::types::ListResult> {
        Err(FraiseQLError::File(FileError::NotImplemented {
            message: "list not yet implemented for GCS".to_string(),
        }))
    }
}
