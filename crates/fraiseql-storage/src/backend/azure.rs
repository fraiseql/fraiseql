//! Azure Blob Storage backend.
//!
//! Authentication uses the `SharedKey` scheme: the storage account key is read
//! from the `AZURE_STORAGE_KEY` environment variable (base64-encoded).

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use fraiseql_error::{FileError, FraiseQLError, Result};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::validate_key;

const AZURE_API_VERSION: &str = "2023-11-03";

/// Azure's ceiling on the number of blocks a single block blob may commit.
const AZURE_MAX_BLOCKS: usize = 50_000;

/// The request content type for a `Put Block List` body.
const AZURE_BLOCK_LIST_CONTENT_TYPE: &str = "application/xml";

/// Build the block id for the `index`-th block of a blob.
///
/// Azure requires every block id in one blob to decode to the same byte
/// length, so the index is zero-padded to a fixed nine digits before being
/// base64-encoded — a width [`AZURE_MAX_BLOCKS`] puts permanently out of reach
/// of overflowing, which is what keeps the ids uniform. Azurite enforces the
/// uniform-length rule at `Put Block List`, so the emulator suite would fail
/// were it broken.
fn block_id_for(index: usize) -> String {
    general_purpose::STANDARD.encode(format!("{index:09}"))
}

/// Decode the block-list continuation state a session persisted. Corrupt state
/// is a loud backend error — a blob cannot be committed from blocks we cannot
/// name.
fn parse_block_state(state: &serde_json::Value) -> Result<(Vec<String>, String)> {
    let blocks = state
        .get("blocks")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| azure_err("block-list state", "no blocks array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| azure_err("block-list state", "a non-string block id"))
        })
        .collect::<Result<Vec<String>>>()?;
    let content_type = state
        .get("content_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_owned();
    Ok((blocks, content_type))
}

/// Stores files in an Azure Blob Storage container.
pub struct AzureBackend {
    account:     String,
    container:   String,
    account_key: Vec<u8>,
    /// Account-level blob endpoint override (e.g. an Azurite emulator URL).
    /// `None` means the production `https://{account}.blob.core.windows.net`
    /// host is used.
    endpoint:    Option<String>,
    client:      reqwest::Client,
}

impl AzureBackend {
    /// Creates a new Azure Blob storage backend against production Azure.
    ///
    /// The storage account key is read from `AZURE_STORAGE_KEY` (base64).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if `AZURE_STORAGE_KEY` is not set or is
    /// not valid base64.
    pub fn new(account: &str, container: &str) -> Result<Self> {
        Self::new_with_endpoint(account, container, None)
    }

    /// Creates a new Azure Blob storage backend with an optional endpoint
    /// override.
    ///
    /// The storage account key is read from `AZURE_STORAGE_KEY` (base64).
    ///
    /// When `endpoint` is `None`, the production Azure Blob host is used:
    /// `https://{account}.blob.core.windows.net`. When set, it is treated as
    /// the account-level blob endpoint base — for the Azurite emulator this is
    /// `http://127.0.0.1:10000/devstoreaccount1`. Blob URLs are formed as
    /// `{endpoint}/{container}/{key}`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if `AZURE_STORAGE_KEY` is not set, is not
    /// valid base64, or if `endpoint` is set but is not a valid URL.
    pub fn new_with_endpoint(
        account: &str,
        container: &str,
        endpoint: Option<&str>,
    ) -> Result<Self> {
        let key_b64 = std::env::var("AZURE_STORAGE_KEY").map_err(|_| {
            FraiseQLError::File(FileError::Backend {
                message: "Azure Blob storage requires AZURE_STORAGE_KEY environment variable"
                    .to_string(),
                source:  None,
            })
        })?;
        let account_key = general_purpose::STANDARD.decode(&key_b64).map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: format!("Invalid AZURE_STORAGE_KEY (not valid base64): {e}"),
                source:  Some(Box::new(e)),
            })
        })?;

        if let Some(ep) = endpoint {
            reqwest::Url::parse(ep).map_err(|e| {
                FraiseQLError::File(FileError::Backend {
                    message: format!("Azure endpoint is not a valid URL: {e}"),
                    source:  Some(Box::new(e)),
                })
            })?;
        }

        Ok(Self {
            account: account.to_owned(),
            container: container.to_owned(),
            account_key,
            endpoint: endpoint.map(str::to_owned),
            client: reqwest::Client::new(),
        })
    }

    /// Returns the full request URL for the given blob key — empty `key`
    /// yields the container-level URL.
    ///
    /// With an `endpoint` override the emulator base already encodes the
    /// account (path-style, e.g. Azurite's `.../devstoreaccount1`), so the
    /// container/key are appended directly. Against production Azure the
    /// account lives in the host name.
    fn blob_url(&self, key: &str) -> String {
        let base = match &self.endpoint {
            Some(ep) => ep.trim_end_matches('/').to_owned(),
            None => format!("https://{}.blob.core.windows.net", self.account),
        };
        if key.is_empty() {
            format!("{base}/{}", self.container)
        } else {
            format!("{base}/{}/{}", self.container, Self::encode_key_path(key))
        }
    }

    /// Returns the canonicalized resource for `SharedKey` signing: `/{account}`
    /// followed by the request URL path. Empty `key` yields the container-level
    /// resource.
    ///
    /// Against production Azure the path is `/{container}[/{key}]`. With a
    /// path-style emulator (Azurite) the request path already begins with the
    /// account segment, and Azurite canonicalizes as `/{account}` + that path —
    /// so the account legitimately appears twice
    /// (`/devstoreaccount1/devstoreaccount1/...`). Deriving the resource from
    /// the actual URL path reproduces that exactly.
    ///
    /// The key is percent-encoded exactly as [`blob_url`](Self::blob_url)
    /// encodes it. Azure's Shared Key scheme signs the *encoded* URI path, and
    /// what matters is that the string signed and the path actually sent are
    /// byte-identical — which is precisely what #876 broke by interpolating the
    /// key raw into both and letting the URL parser rewrite only one of them.
    fn canonicalized_resource(&self, key: &str) -> String {
        let path = match &self.endpoint {
            Some(ep) => reqwest::Url::parse(ep)
                .ok()
                .map(|u| u.path().trim_end_matches('/').to_owned())
                .unwrap_or_default(),
            None => String::new(),
        };
        if key.is_empty() {
            format!("/{}{path}/{}", self.account, self.container)
        } else {
            format!("/{}{path}/{}/{}", self.account, self.container, Self::encode_key_path(key))
        }
    }

    /// Percent-encode a storage key for use as a URL path.
    ///
    /// #876: the key used to be interpolated raw, so reqwest parsed URL syntax
    /// out of it — `#` began a fragment (silently truncating the request path),
    /// `?` began a query, and an existing `%41` was decoded by Azure to `A`.
    /// The `SharedKey` string-to-sign, built from the *unencoded* key, then
    /// described a different resource than the request did, and Azure answered
    /// `403 AuthenticationFailed` for what is really a perfectly ordinary
    /// filename.
    ///
    /// Each `/`-separated segment is encoded independently so the key stays a
    /// path. [`canonicalized_resource`](Self::canonicalized_resource)
    /// deliberately keeps the *decoded* form, because Azure computes the
    /// canonicalized resource from the decoded request path.
    fn encode_key_path(key: &str) -> String {
        key.split('/')
            .map(|segment| urlencoding::encode(segment))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Creates the configured container if it does not already exist.
    ///
    /// Primarily useful against emulators (Azurite), which start with no
    /// containers. Against production Azure the container is normally created
    /// out of band. A pre-existing container (`409 Conflict`) is treated as
    /// success.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the request fails or the backend
    /// returns an unexpected status.
    pub async fn create_container_if_missing(&self) -> Result<()> {
        let url = format!("{}?restype=container", self.blob_url(""));
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

        // The `restype=container` query parameter participates in the
        // canonicalized resource for SharedKey signing. Content-Length is
        // signed as an empty string for zero-length bodies (SharedKey rule
        // for API version 2015-02-21 and later).
        let canonical_resource_suffix = "\nrestype:container";
        let auth = self.sign_request_with_resource(
            "PUT",
            "",
            "",
            "",
            &date,
            "",
            canonical_resource_suffix,
        );

        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .send()
            .await
            .map_err(|e| azure_err_src("create container", e))?;

        match resp.status() {
            s if s.is_success() => Ok(()),
            reqwest::StatusCode::CONFLICT => Ok(()),
            _ => {
                let body = resp.text().await.unwrap_or_default();
                Err(azure_err("create container response", body))
            },
        }
    }

    /// Computes the `Authorization: SharedKey` header value for an
    /// object-level request (`/{account}/{container}/{key}`).
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
        self.sign_request_with_resource(
            verb,
            key,
            content_type,
            content_length,
            date,
            extra_canonical_headers,
            "",
        )
    }

    /// Computes the `Authorization: SharedKey` header value, appending
    /// `canonical_resource_suffix` (e.g. `"\nrestype:container"`) to the
    /// canonicalized resource for query-parameterised requests.
    ///
    /// When `key` is empty the canonicalized resource is container-level
    /// (`/{account}/{container}`); otherwise it is object-level
    /// (`/{account}/{container}/{key}`).
    #[allow(clippy::too_many_arguments)] // Reason: mirrors Azure SharedKey string-to-sign inputs
    fn sign_request_with_resource(
        &self,
        verb: &str,
        key: &str,
        content_type: &str,
        content_length: &str,
        date: &str,
        extra_canonical_headers: &str,
        canonical_resource_suffix: &str,
    ) -> String {
        let canonicalized_resource =
            format!("{}{canonical_resource_suffix}", self.canonicalized_resource(key));

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
    FraiseQLError::File(FileError::Backend {
        message: format!("Azure Blob {op} failed: {detail}"),
        source:  None,
    })
}

/// Like [`azure_err`] but preserves the underlying error in the chain.
fn azure_err_src(op: &str, err: impl std::error::Error + Send + Sync + 'static) -> FraiseQLError {
    let message = format!("Azure Blob {op} failed: {err}");
    FraiseQLError::File(FileError::Backend {
        message,
        source: Some(Box::new(err)),
    })
}

impl AzureBackend {
    /// Uploads data and returns the storage key.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the upload fails.
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
            .map_err(|e| azure_err_src("upload", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("upload response", body));
        }

        Ok(key.to_owned())
    }

    /// Downloads the contents of the given key from Azure Blob Storage.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the download fails or the key does not exist.
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
            .map_err(|e| azure_err_src("download", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            }
            .into());
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("download response", body));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| azure_err_src("download body", e))
    }

    /// Deletes the object at the given key from Azure Blob Storage.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the delete fails or the key does not exist.
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
            .map_err(|e| azure_err_src("delete", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FileError::NotFound {
                id: key.to_string(),
            }
            .into());
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("delete response", body));
        }

        Ok(())
    }

    /// Checks whether an object exists at the given key in Azure Blob Storage.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend communication errors.
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
            .map_err(|e| azure_err_src("exists check", e))?;

        match resp.status() {
            s if s.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => Err(azure_err("exists check response", resp.status().to_string())),
        }
    }

    /// Begin a block-list upload for a resumable upload (#972).
    ///
    /// Azure stages a block blob as uncommitted blocks and publishes nothing
    /// until `Put Block List`, so there is no server-side session to open. The
    /// container is checked here anyway: a missing container would otherwise
    /// surface as a failure on the first chunk, after the route has already
    /// told the client the upload exists.
    ///
    /// `total_bytes` is unused — Azure needs no declared size — and is accepted
    /// so every backend shares one seam.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the container does not exist or cannot
    /// be reached.
    pub async fn multipart_begin(
        &self,
        key: &str,
        content_type: &str,
        total_bytes: u64,
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let _ = total_bytes;
        self.require_container().await?;
        Ok(serde_json::json!({ "blocks": [], "content_type": content_type }))
    }

    /// Stage one chunk as the next uncommitted block. Returns the updated
    /// continuation state with the new block id appended.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure, on corrupted
    /// continuation state, or once the blob's 50 000-block ceiling is reached.
    pub async fn multipart_append(
        &self,
        key: &str,
        state: serde_json::Value,
        data: &[u8],
    ) -> Result<serde_json::Value> {
        validate_key(key)?;
        let (mut blocks, content_type) = parse_block_state(&state)?;
        if blocks.len() >= AZURE_MAX_BLOCKS {
            return Err(azure_err(
                "put block",
                format!("a block blob holds at most {AZURE_MAX_BLOCKS} blocks"),
            ));
        }
        let block_id = block_id_for(blocks.len());

        let url =
            format!("{}?comp=block&blockid={}", self.blob_url(key), urlencoding::encode(&block_id));
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let content_length = data.len().to_string();
        // Query parameters join the canonicalized resource in lexicographic
        // order, URL-*decoded* — `blockid` sorts before `comp`. The URL carries
        // the percent-encoded form; the signature carries the raw one.
        let auth = self.sign_request_with_resource(
            "PUT",
            key,
            "",
            &content_length,
            &date,
            "",
            &format!("\nblockid:{block_id}\ncomp:block"),
        );

        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| azure_err_src("put block", e))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(azure_err("put block response", body));
        }

        blocks.push(block_id);
        Ok(serde_json::json!({ "blocks": blocks, "content_type": content_type }))
    }

    /// Commit the staged blocks in order, publishing the blob. Returns its etag.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on backend failure or corrupted
    /// continuation state.
    pub async fn multipart_complete(&self, key: &str, state: &serde_json::Value) -> Result<String> {
        validate_key(key)?;
        let (blocks, content_type) = parse_block_state(state)?;

        let mut body = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<BlockList>");
        for id in &blocks {
            // Block ids are base64 of a fixed-width decimal index, so they
            // carry no XML-significant character; nothing here needs escaping.
            body.push_str("<Latest>");
            body.push_str(id);
            body.push_str("</Latest>");
        }
        body.push_str("</BlockList>");

        let url = format!("{}?comp=blocklist", self.blob_url(key));
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let content_length = body.len().to_string();
        let auth = self.sign_request_with_resource(
            "PUT",
            key,
            AZURE_BLOCK_LIST_CONTENT_TYPE,
            &content_length,
            &date,
            // `x-ms-blob-content-type` sorts before `x-ms-date`, so it belongs
            // at the head of the canonicalized headers block.
            &format!("x-ms-blob-content-type:{content_type}\n"),
            "\ncomp:blocklist",
        );

        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .header("x-ms-blob-content-type", &content_type)
            .header("Content-Type", AZURE_BLOCK_LIST_CONTENT_TYPE)
            .body(body)
            .send()
            .await
            .map_err(|e| azure_err_src("put block list", e))?;

        if !resp.status().is_success() {
            let detail = resp.text().await.unwrap_or_default();
            return Err(azure_err("put block list response", detail));
        }

        Ok(resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned())
    }

    /// Discard a block-list upload.
    ///
    /// There is deliberately no request here. Azure has no "abort" for
    /// uncommitted blocks: they belong to no blob until `Put Block List`, and
    /// Azure garbage-collects them a week after the last write. Deleting the
    /// blob instead would be actively wrong — a resumable upload may be
    /// overwriting an object that already exists, and cancelling it must leave
    /// that object exactly as it was.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the continuation state is corrupt.
    pub fn multipart_abort(&self, key: &str, state: &serde_json::Value) -> Result<()> {
        validate_key(key)?;
        let (blocks, _) = parse_block_state(state)?;
        tracing::debug!(
            key = %key,
            blocks = blocks.len(),
            "Abandoning uncommitted Azure blocks; Azure reclaims them after a week"
        );
        Ok(())
    }

    /// Fail unless the configured container exists.
    async fn require_container(&self) -> Result<()> {
        let url = format!("{}?restype=container", self.blob_url(""));
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let auth =
            self.sign_request_with_resource("HEAD", "", "", "", &date, "", "\nrestype:container");

        let resp = self
            .client
            .head(&url)
            .header("Authorization", &auth)
            .header("x-ms-date", &date)
            .header("x-ms-version", AZURE_API_VERSION)
            .send()
            .await
            .map_err(|e| azure_err_src("container check", e))?;

        if resp.status().is_success() {
            return Ok(());
        }
        Err(azure_err(
            "container check",
            format!("container '{}' is not available ({})", self.container, resp.status()),
        ))
    }

    /// Generates a presigned URL for direct access to an Azure Blob.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::NotImplemented)` as SAS token
    /// generation is not yet implemented.
    pub async fn presigned_url(&self, _key: &str, _expiry: Duration) -> Result<String> {
        // Azure presigned access uses SAS (Shared Access Signature) tokens,
        // which require HMAC signing of the resource path, permissions, and
        // expiry.  This is planned but not yet implemented — use the Azure CLI
        // (`az storage blob generate-sas`) in the meantime.
        Err(FraiseQLError::File(FileError::NotImplemented {
            message:
                "Presigned URLs for Azure Blob require SAS token generation (not yet implemented)"
                    .to_string(),
        }))
    }

    /// Lists objects in the container by prefix with pagination.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File(FileError::NotImplemented)` since list
    /// is not yet implemented for Azure Blob.
    pub async fn list(
        &self,
        _prefix: &str,
        _cursor: Option<&str>,
        _limit: usize,
    ) -> Result<super::types::ListResult> {
        Err(FraiseQLError::File(FileError::NotImplemented {
            message: "list not yet implemented for Azure Blob".to_string(),
        }))
    }
}
