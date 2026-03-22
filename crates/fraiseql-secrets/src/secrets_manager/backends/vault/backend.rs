use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use chrono::Utc;
use tokio::sync::RwLock;
use zeroize::Zeroizing;

use super::{
    cache::{CACHE_TTL_PERCENTAGE, DEFAULT_MAX_CACHE_ENTRIES, SecretCache, VaultResponse},
    validation::{validate_vault_addr, validate_vault_secret_name},
};
use crate::secrets_manager::{SecretsBackend, SecretsError};

/// Fraction of the token TTL after which the token should be proactively renewed.
/// At 80% of TTL elapsed, renewal is triggered before the token expires.
#[allow(dead_code)] // Reason: used by renew_token() scheduling logic to avoid a magic literal
const TOKEN_RENEWAL_THRESHOLD: f64 = 0.8;

const VAULT_API_VERSION: &str = "v1";

/// Vault HTTP request timeout.
const VAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Build a shared `reqwest::Client` for Vault HTTP calls.
///
/// The client is created once per `VaultBackend` instance and reused across
/// all requests to avoid per-call TLS handshake overhead.
fn build_http_client(tls_verify: bool) -> Result<reqwest::Client, SecretsError> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(!tls_verify)
        .timeout(VAULT_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| SecretsError::ConnectionError(format!("HTTP client error: {e}")))
}

/// Secrets backend for HashiCorp Vault
///
/// Provides dynamic secrets, credential rotation, and lease management
/// via the HashiCorp Vault HTTP API.
///
/// # Example
/// ```no_run
/// // Requires: live HashiCorp Vault server.
/// # async fn example() -> Result<(), fraiseql_secrets::secrets_manager::SecretsError> {
/// use fraiseql_secrets::secrets_manager::VaultBackend;
/// use fraiseql_secrets::secrets_manager::SecretsBackend;
/// let vault = VaultBackend::new(
///     "https://vault.example.com:8200",
///     "s.xxxxxxxxxxxxxxxx"
/// );
/// let secret = vault.get_secret("database/creds/fraiseql").await?;
/// let (secret, expiry) = vault.get_secret_with_expiry("database/creds/fraiseql").await?;
/// # Ok(())
/// # }
/// ```
///
/// # Features
/// - Dynamic database credentials from configured roles
/// - Automatic lease tracking and renewal
/// - Generic secret retrieval from KV2 engine
/// - Encryption via Transit engine
/// - Audit logging for all operations
/// - Connection pooling and retry logic
///
/// # Token Renewal
///
/// When the Vault token is obtained via AppRole login (`with_approle`), the token carries
/// a TTL. To avoid using an expired token, callers should check `token_needs_renewal()` and
/// call `renew_token()` proactively at `TOKEN_RENEWAL_THRESHOLD` (80%) of TTL elapsed.
///
/// A background task should be spawned to call `renew_token()` periodically; for example:
/// ```rust,ignore
/// let vault = Arc::new(VaultBackend::with_approle(addr, role_id, secret_id).await?);
/// let vault_clone = Arc::clone(&vault);
/// tokio::spawn(async move {
///     loop {
///         tokio::time::sleep(Duration::from_secs(60)).await;
///         if vault_clone.token_needs_renewal() {
///             if let Err(e) = vault_clone.renew_token().await { /* log */ }
///         }
///     }
/// });
/// ```
///
/// # Configuration
/// ```toml
/// [secrets.vault]
/// addr = "https://vault.example.com:8200"
/// token = "s.xxxxxxxxxxxxxxxx"  # From environment or secrets manager
/// namespace = "fraiseql/prod"    # Optional, for Enterprise
/// tls_verify = true              # Verify TLS certificates
/// ```
#[derive(Debug)]
pub struct VaultBackend {
    addr:              String,
    token:             Zeroizing<String>,
    namespace:         Option<String>,
    tls_verify:        bool,
    /// Shared HTTP client — built once to reuse TLS sessions across requests.
    client:            reqwest::Client,
    cache:             Arc<RwLock<SecretCache>>,
    /// When the current token was obtained (for renewal tracking).
    /// `None` when using a static long-lived token.
    token_obtained_at: Option<chrono::DateTime<Utc>>,
    /// Token TTL as reported by Vault at login time (seconds).
    /// `None` when using a static long-lived token.
    token_ttl_secs:    Option<i64>,
}

impl Clone for VaultBackend {
    fn clone(&self) -> Self {
        VaultBackend {
            addr:              self.addr.clone(),
            token:             Zeroizing::new((*self.token).clone()),
            namespace:         self.namespace.clone(),
            tls_verify:        self.tls_verify,
            client:            self.client.clone(),
            cache:             Arc::clone(&self.cache),
            token_obtained_at: self.token_obtained_at,
            token_ttl_secs:    self.token_ttl_secs,
        }
    }
}

#[async_trait::async_trait]
impl SecretsBackend for VaultBackend {
    async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
        validate_vault_secret_name(name)?;

        let (secret, _) = self.get_secret_with_expiry(name).await?;
        Ok(secret)
    }

    async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, chrono::DateTime<Utc>), SecretsError> {
        validate_vault_secret_name(name)?;

        // Check cache first
        let cache = self.cache.read().await;
        if let Some((cached_value, cached_expiry)) = cache.get_with_expiry(name).await {
            return Ok((cached_value, cached_expiry));
        }
        drop(cache); // Release read lock before fetching

        // Fetch from Vault
        let response = self.fetch_secret(name).await?;

        // Calculate expiry: now + lease_duration
        let expiry = Utc::now() + chrono::Duration::seconds(response.lease_duration);
        // Scale lease_duration by CACHE_TTL_PERCENTAGE (0.8) using integer arithmetic to
        // avoid the f64→i64 precision loss that occurs for large TTLs (> 2^53 seconds).
        // Saturating multiplication prevents overflow if Vault returns an extreme TTL.
        let cache_ttl_secs =
            response.lease_duration.saturating_mul((CACHE_TTL_PERCENTAGE * 100.0) as i64) / 100;
        let cache_expiry = Utc::now() + chrono::Duration::seconds(cache_ttl_secs);

        // Extract secret from response data
        let secret_str = Self::extract_secret_from_response(&response, name)?;

        // Store in cache
        let cache = self.cache.read().await;
        cache.set(name.to_string(), secret_str.clone(), cache_expiry).await;

        Ok((secret_str, expiry))
    }

    async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        validate_vault_secret_name(name)?;

        // Invalidate the cache so get_secret_with_expiry fetches fresh credentials
        // instead of returning the stale (pre-rotation) cached value.
        let cache = self.cache.read().await;
        cache.invalidate(name).await;
        drop(cache);

        let (new_secret, _) = self.get_secret_with_expiry(name).await?;
        Ok(new_secret)
    }
}

impl VaultBackend {
    /// Create new VaultBackend with server address and authentication token.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (this should never happen in
    /// practice — only invalid TLS configuration can trigger this path), or if
    /// `addr` targets a private/loopback address (SSRF protection).
    #[must_use]
    pub fn new<S: Into<String>>(addr: S, token: S) -> Self {
        let addr_str: String = addr.into();
        // SECURITY: Reject Vault addresses that target private/loopback ranges (SSRF).
        validate_vault_addr(&addr_str).expect("Vault address failed SSRF validation");
        let client = build_http_client(true).expect("Failed to build Vault HTTP client");
        VaultBackend {
            addr: addr_str,
            token: Zeroizing::new(token.into()),
            namespace: None,
            tls_verify: true,
            client,
            cache: Arc::new(RwLock::new(SecretCache::new(DEFAULT_MAX_CACHE_ENTRIES))),
            // Static token — no TTL tracking
            token_obtained_at: None,
            token_ttl_secs: None,
        }
    }

    /// Set Vault namespace (Enterprise feature).
    #[must_use]
    pub fn with_namespace<S: Into<String>>(mut self, namespace: S) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set TLS certificate verification.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be rebuilt (should not happen in practice).
    #[must_use]
    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        // Rebuild the shared client with the updated TLS setting.
        self.client = build_http_client(verify).expect("Failed to rebuild Vault HTTP client");
        self
    }

    /// Create a `VaultBackend` by authenticating via AppRole.
    ///
    /// Posts to `/v1/auth/approle/login` with the given `role_id` and `secret_id`,
    /// then uses the returned client token for subsequent requests.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::ConnectionError` if login fails.
    pub async fn with_approle(
        addr: &str,
        role_id: &str,
        secret_id: &str,
    ) -> Result<Self, SecretsError> {
        let client = reqwest::Client::builder()
            .timeout(VAULT_REQUEST_TIMEOUT)
            .build()
            .map_err(|e| SecretsError::ConnectionError(format!("HTTP client error: {e}")))?;

        let login_url =
            format!("{}/{}/auth/approle/login", addr.trim_end_matches('/'), VAULT_API_VERSION);
        let body = serde_json::json!({
            "role_id": role_id,
            "secret_id": secret_id,
        });

        let response: serde_json::Value = client
            .post(&login_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretsError::ConnectionError(format!("AppRole login failed: {e}")))?
            .json()
            .await
            .map_err(|e| {
                SecretsError::ConnectionError(format!("AppRole response parse error: {e}"))
            })?;

        let token = response["auth"]["client_token"]
            .as_str()
            .ok_or_else(|| {
                SecretsError::ConnectionError("No client_token in AppRole response".into())
            })?
            .to_string();

        let token_ttl_secs = response["auth"]["lease_duration"].as_i64();
        let token_obtained_at = Some(Utc::now());

        let mut backend = Self::new(addr, &token);
        backend.token_obtained_at = token_obtained_at;
        backend.token_ttl_secs = token_ttl_secs;
        Ok(backend)
    }

    /// Get Vault server address.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Get authentication token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get configured namespace.
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Check if TLS verification is enabled.
    #[must_use]
    pub fn tls_verify(&self) -> bool {
        self.tls_verify
    }

    /// Create a `VaultBackend` pointing at a loopback address for unit/integration tests.
    ///
    /// **This constructor bypasses SSRF validation** — it exists solely so that tests
    /// can point the backend at a `wiremock::MockServer` bound to `127.0.0.1`.
    /// Do NOT use in production code.
    #[cfg(test)]
    pub(super) fn new_for_test(addr: impl Into<String>, token: impl Into<String>) -> Self {
        let client = build_http_client(false).expect("Failed to build test Vault HTTP client");
        VaultBackend {
            addr: addr.into(),
            token: Zeroizing::new(token.into()),
            namespace: None,
            tls_verify: false,
            client,
            cache: Arc::new(RwLock::new(SecretCache::new(DEFAULT_MAX_CACHE_ENTRIES))),
            token_obtained_at: None,
            token_ttl_secs: None,
        }
    }

    /// Returns `true` if the Vault auth token should be renewed.
    ///
    /// Returns `false` for static tokens (created via `new()`). For AppRole tokens
    /// (created via `with_approle()`), returns `true` when more than
    /// `TOKEN_RENEWAL_THRESHOLD` (80%) of the token's TTL has elapsed.
    ///
    /// Callers should invoke `renew_token()` when this returns `true` to avoid
    /// authentication failures from expired tokens.
    #[must_use]
    pub fn token_needs_renewal(&self) -> bool {
        let (Some(obtained_at), Some(ttl_secs)) = (self.token_obtained_at, self.token_ttl_secs)
        else {
            // Static long-lived token — no renewal needed
            return false;
        };

        let elapsed_secs = (Utc::now() - obtained_at).num_seconds();
        // Use integer arithmetic to avoid f64 precision loss for large TTL values.
        let renewal_threshold_secs =
            ttl_secs.saturating_mul((TOKEN_RENEWAL_THRESHOLD * 100.0) as i64) / 100;
        elapsed_secs >= renewal_threshold_secs
    }

    /// Renew the Vault auth token via `POST /v1/auth/token/renew-self`.
    ///
    /// On success, resets the `token_obtained_at` clock and updates `token_ttl_secs`
    /// from the response so that `token_needs_renewal()` reflects the renewed lease.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::ConnectionError` if the renewal request fails or if
    /// the token is not renewable (e.g. orphaned or periodic tokens).
    pub async fn renew_token(&mut self) -> Result<(), SecretsError> {
        let url = format!(
            "{}/{}/auth/token/renew-self",
            self.addr.trim_end_matches('/'),
            VAULT_API_VERSION
        );
        let response: serde_json::Value = self
            .client
            .post(&url)
            .header("X-Vault-Token", &*self.token)
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
            .send()
            .await
            .map_err(|e| {
                SecretsError::ConnectionError(format!("Token renewal request failed: {e}"))
            })?
            .json()
            .await
            .map_err(|e| {
                SecretsError::ConnectionError(format!("Token renewal response parse error: {e}"))
            })?;

        // Vault returns the renewed token info under `auth`
        let new_token = response["auth"]["client_token"].as_str().ok_or_else(|| {
            SecretsError::ConnectionError(
                "No client_token in renewal response — token may not be renewable".into(),
            )
        })?;

        self.token = Zeroizing::new(new_token.to_string());
        self.token_obtained_at = Some(Utc::now());
        if let Some(ttl) = response["auth"]["lease_duration"].as_i64() {
            self.token_ttl_secs = Some(ttl);
        }

        Ok(())
    }

    /// Token TTL as set at login time (seconds). `None` for static tokens.
    /// Exposed for test assertions only.
    #[cfg(test)]
    pub(super) fn token_ttl_secs_for_test(&self) -> Option<i64> {
        self.token_ttl_secs
    }

    /// Extract secret data from Vault API response.
    ///
    /// Handles both KV2 format (nested data.data) and dynamic credentials (flat data).
    pub(super) fn extract_secret_from_response(
        response: &VaultResponse,
        path: &str,
    ) -> Result<String, SecretsError> {
        // For KV2 engine: response.data.data contains actual secret
        // For dynamic credentials: response.data contains username/password
        if let Some(data_obj) = response.data.get("data") {
            serde_json::to_string(data_obj).map_err(|e| {
                SecretsError::BackendError(format!(
                    "Failed to serialize KV2 secret from {}: {}",
                    path, e
                ))
            })
        } else {
            // Dynamic credentials or generic secret
            serde_json::to_string(&response.data).map_err(|e| {
                SecretsError::BackendError(format!(
                    "Failed to serialize secret from {}: {}",
                    path, e
                ))
            })
        }
    }

    /// Fetch secret from Vault HTTP API with retry on transient errors.
    ///
    /// Retries up to 3 times with exponential backoff (100ms, 200ms, 400ms)
    /// on 503 (Service Unavailable), 429 (Too Many Requests), or connection errors.
    /// Non-retryable errors (403, 404) fail immediately.
    async fn fetch_secret(&self, name: &str) -> Result<VaultResponse, SecretsError> {
        let url = self.build_vault_url(name);
        let delays = [
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(400),
        ];

        let mut last_error = None;
        for (attempt, delay) in delays.iter().enumerate() {
            match self
                .client
                .get(&url)
                .header("X-Vault-Token", &*self.token)
                .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
                .send()
                .await
            {
                Ok(response) => {
                    match response.status() {
                        reqwest::StatusCode::OK => {
                            return response.json::<VaultResponse>().await.map_err(|e| {
                                SecretsError::BackendError(format!(
                                    "Failed to parse Vault response for {name}: {e}"
                                ))
                            });
                        },
                        // Retryable statuses
                        reqwest::StatusCode::SERVICE_UNAVAILABLE
                        | reqwest::StatusCode::TOO_MANY_REQUESTS => {
                            last_error = Some(SecretsError::BackendError(format!(
                                "Vault returned {} (attempt {}/3)",
                                response.status(),
                                attempt + 1
                            )));
                            tokio::time::sleep(*delay).await;
                        },
                        // Non-retryable statuses
                        reqwest::StatusCode::NOT_FOUND => {
                            return Err(SecretsError::NotFound(format!(
                                "Secret not found in Vault: {name}"
                            )));
                        },
                        reqwest::StatusCode::FORBIDDEN => {
                            return Err(SecretsError::BackendError(format!(
                                "Permission denied accessing Vault secret: {name}"
                            )));
                        },
                        status => {
                            return Err(SecretsError::BackendError(format!(
                                "Vault request failed with status {status} for {name}"
                            )));
                        },
                    }
                },
                Err(e) => {
                    // Connection errors are retryable
                    last_error = Some(SecretsError::ConnectionError(format!(
                        "Vault connection error (attempt {}/3): {e}",
                        attempt + 1
                    )));
                    tokio::time::sleep(*delay).await;
                },
            }
        }
        Err(last_error
            .unwrap_or_else(|| SecretsError::ConnectionError("Max retries exceeded".into())))
    }

    /// Build Vault API URL for a secret path.
    fn build_vault_url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.addr.trim_end_matches('/'), VAULT_API_VERSION, path)
    }

    /// Build HTTP request to Vault with standard headers.
    fn build_vault_request(
        &self,
        client: &reqwest::Client,
        url: String,
    ) -> reqwest::RequestBuilder {
        client
            .post(&url)
            .header("X-Vault-Token", (*self.token).clone())
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
    }

    /// Handle Transit engine response for encrypt/decrypt.
    async fn handle_transit_response(
        &self,
        response: reqwest::Response,
        data_field: &str,
        operation: &str,
    ) -> Result<String, SecretsError> {
        match response.status() {
            reqwest::StatusCode::OK => {
                let body = response.json::<serde_json::Value>().await.map_err(|e| {
                    SecretsError::BackendError(format!("Failed to parse Transit response: {}", e))
                })?;

                body["data"][data_field].as_str().map(|s| s.to_string()).ok_or_else(|| {
                    SecretsError::EncryptionError(format!("Missing {} in response", data_field))
                })
            },
            reqwest::StatusCode::NOT_FOUND => {
                Err(SecretsError::NotFound("Transit key not found".to_string()))
            },
            status => Err(SecretsError::EncryptionError(format!(
                "Vault Transit {} failed with status {}",
                operation, status
            ))),
        }
    }

    /// Encrypt plaintext using Vault Transit engine.
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `plaintext` - Data to encrypt
    ///
    /// # Returns
    /// Encrypted ciphertext in Vault's standard format.
    ///
    /// # Errors
    ///
    /// - `SecretsError::ValidationError` if the key name is invalid.
    /// - `SecretsError::BackendError` if the Vault request fails.
    /// - `SecretsError::NotFound` if the transit key does not exist.
    pub async fn encrypt_field(
        &self,
        key_name: &str,
        plaintext: &str,
    ) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let url = format!(
            "{}/{}/transit/encrypt/{}",
            self.addr.trim_end_matches('/'),
            VAULT_API_VERSION,
            key_name
        );

        let request_body = serde_json::json!({
            "plaintext": STANDARD_NO_PAD.encode(plaintext)
        });

        let response = self
            .build_vault_request(&self.client, url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                SecretsError::BackendError(format!("Vault Transit encrypt request failed: {}", e))
            })?;

        self.handle_transit_response(response, "ciphertext", "encrypt").await
    }

    /// Decrypt ciphertext using Vault Transit engine.
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `ciphertext` - Encrypted data (in Vault's format)
    ///
    /// # Returns
    /// Decrypted plaintext.
    ///
    /// # Errors
    ///
    /// - `SecretsError::ValidationError` if the key name is invalid.
    /// - `SecretsError::BackendError` if the Vault request fails.
    /// - `SecretsError::EncryptionError` if Transit decryption fails.
    pub async fn decrypt_field(
        &self,
        key_name: &str,
        ciphertext: &str,
    ) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let url = format!(
            "{}/{}/transit/decrypt/{}",
            self.addr.trim_end_matches('/'),
            VAULT_API_VERSION,
            key_name
        );

        let request_body = serde_json::json!({
            "ciphertext": ciphertext
        });

        let response = self
            .build_vault_request(&self.client, url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                SecretsError::BackendError(format!("Vault Transit decrypt request failed: {}", e))
            })?;

        // Get plaintext from response
        let plaintext_b64 = self.handle_transit_response(response, "plaintext", "decrypt").await?;

        // Decode base64 to get original plaintext
        STANDARD_NO_PAD
            .decode(&plaintext_b64)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| SecretsError::EncryptionError("Failed to decode plaintext".to_string()))
    }
}
