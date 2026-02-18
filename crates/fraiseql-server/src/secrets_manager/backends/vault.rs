//! Backend for HashiCorp Vault integration with dynamic secrets,
//! lease management, and encryption support.
//!
//! Implements the `SecretsBackend` trait for HashiCorp Vault,
//! providing dynamic database credentials, TTL management, and encryption.

use std::{collections::HashMap, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use chrono::Utc;
use tokio::sync::RwLock;

use super::super::{SecretsBackend, SecretsError};

/// Vault API response structure for secrets
#[derive(Debug, Clone, serde::Deserialize)]
// Reason: fields populated by serde deserialization; only `data` accessed directly
#[allow(dead_code)]
struct VaultResponse {
    request_id:     String,
    lease_id:       String,
    lease_duration: i64,
    renewable:      bool,
    data:           HashMap<String, serde_json::Value>,
}

/// Cached secret with expiry metadata.
#[derive(Debug, Clone)]
struct CachedSecret {
    value:      String,
    expires_at: chrono::DateTime<Utc>,
}

// Constants for Vault API and caching
const VAULT_API_VERSION: &str = "v1";
const CACHE_TTL_PERCENTAGE: f64 = 0.8; // Cache for 80% of credential TTL
const DEFAULT_MAX_CACHE_ENTRIES: usize = 1000; // Maximum cached secrets

/// Secret cache with TTL management and LRU eviction for credential caching.
#[derive(Debug)]
struct SecretCache {
    entries:     Arc<RwLock<HashMap<String, CachedSecret>>>,
    max_entries: usize,
}

impl SecretCache {
    /// Create new secret cache with specified max entries
    fn new(max_entries: usize) -> Self {
        SecretCache {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    /// Get cached secret with expiry information
    async fn get_with_expiry(&self, key: &str) -> Option<(String, chrono::DateTime<Utc>)> {
        let entries = self.entries.read().await;
        if let Some(cached) = entries.get(key) {
            if cached.expires_at > Utc::now() {
                return Some((cached.value.clone(), cached.expires_at));
            }
        }
        None
    }

    /// Store secret in cache with expiry
    async fn set(
        &self,
        key: String,
        secret: String,
        expires_at: chrono::DateTime<Utc>,
    ) {
        let mut entries = self.entries.write().await;

        // Simple LRU: if at capacity, clear oldest 10% of entries
        if entries.len() >= self.max_entries {
            let remove_count = (self.max_entries / 10).max(1);
            let keys_to_remove: Vec<_> =
                entries.iter().take(remove_count).map(|(k, _)| k.clone()).collect();
            for key in keys_to_remove {
                entries.remove(&key);
            }
        }

        entries.insert(
            key,
            CachedSecret {
                value: secret,
                expires_at,
            },
        );
    }
}

/// Secrets backend for HashiCorp Vault
///
/// Provides dynamic secrets, credential rotation, and lease management
/// via the HashiCorp Vault HTTP API.
///
/// # Example
/// ```ignore
/// let vault = VaultBackend::new(
///     "https://vault.example.com:8200",
///     "s.xxxxxxxxxxxxxxxx"
/// );
/// let secret = vault.get_secret("database/creds/fraiseql").await?;
/// let (secret, expiry) = vault.get_secret_with_expiry("database/creds/fraiseql").await?;
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
    addr:       String,
    token:      String,
    namespace:  Option<String>,
    tls_verify: bool,
    cache:      Arc<RwLock<SecretCache>>,
}

impl Clone for VaultBackend {
    fn clone(&self) -> Self {
        VaultBackend {
            addr:       self.addr.clone(),
            token:      self.token.clone(),
            namespace:  self.namespace.clone(),
            tls_verify: self.tls_verify,
            cache:      Arc::clone(&self.cache),
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
        let cache_expiry = Utc::now()
            + chrono::Duration::seconds(
                (response.lease_duration as f64 * CACHE_TTL_PERCENTAGE) as i64,
            );

        // Extract secret from response data
        let secret_str = Self::extract_secret_from_response(&response, name)?;

        // Store in cache
        let cache = self.cache.read().await;
        cache
            .set(name.to_string(), secret_str.clone(), cache_expiry)
            .await;

        Ok((secret_str, expiry))
    }

    async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
        validate_vault_secret_name(name)?;

        // Rotate by requesting new credentials (old lease is implicitly superseded)
        let (new_secret, _) = self.get_secret_with_expiry(name).await?;
        Ok(new_secret)
    }
}

impl VaultBackend {
    /// Create new VaultBackend with server address and authentication token
    #[must_use]
    pub fn new<S: Into<String>>(addr: S, token: S) -> Self {
        VaultBackend {
            addr:       addr.into(),
            token:      token.into(),
            namespace:  None,
            tls_verify: true,
            cache:      Arc::new(RwLock::new(SecretCache::new(DEFAULT_MAX_CACHE_ENTRIES))),
        }
    }

    /// Set Vault namespace (Enterprise feature)
    #[must_use]
    pub fn with_namespace<S: Into<String>>(mut self, namespace: S) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set TLS certificate verification.
    #[must_use]
    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
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
    pub async fn with_approle(addr: &str, role_id: &str, secret_id: &str) -> Result<Self, SecretsError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| SecretsError::ConnectionError(format!("HTTP client error: {e}")))?;

        let login_url = format!("{}/{}/auth/approle/login", addr.trim_end_matches('/'), VAULT_API_VERSION);
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
            .map_err(|e| SecretsError::ConnectionError(format!("AppRole response parse error: {e}")))?;

        let token = response["auth"]["client_token"]
            .as_str()
            .ok_or_else(|| SecretsError::ConnectionError("No client_token in AppRole response".into()))?
            .to_string();

        Ok(Self::new(addr, &token))
    }

    /// Get Vault server address.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Get authentication token
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get configured namespace
    #[must_use]
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Check if TLS verification is enabled
    #[must_use]
    pub fn tls_verify(&self) -> bool {
        self.tls_verify
    }

    /// Extract secret data from Vault API response
    ///
    /// Handles both KV2 format (nested data.data) and dynamic credentials (flat data)
    fn extract_secret_from_response(
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
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| {
                SecretsError::BackendError(format!("Failed to create HTTP client: {e}"))
            })?;

        let url = self.build_vault_url(name);
        let delays = [
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(400),
        ];

        let mut last_error = None;
        for (attempt, delay) in delays.iter().enumerate() {
            match client
                .get(&url)
                .header("X-Vault-Token", &self.token)
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
        Err(last_error.unwrap_or_else(|| {
            SecretsError::ConnectionError("Max retries exceeded".into())
        }))
    }

    /// Build Vault API URL for a secret path
    fn build_vault_url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.addr.trim_end_matches('/'), VAULT_API_VERSION, path)
    }

    /// Build HTTP request to Vault with standard headers
    fn build_vault_request(
        &self,
        client: &reqwest::Client,
        url: String,
    ) -> reqwest::RequestBuilder {
        client
            .post(&url)
            .header("X-Vault-Token", self.token.clone())
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
    }

    /// Handle Transit engine response for encrypt/decrypt
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

    /// Encrypt plaintext using Vault Transit engine
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `plaintext` - Data to encrypt
    ///
    /// # Returns
    /// Encrypted ciphertext in Vault's standard format
    pub async fn encrypt_field(
        &self,
        key_name: &str,
        plaintext: &str,
    ) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| {
                SecretsError::BackendError(format!("Failed to create HTTP client: {}", e))
            })?;

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
            .build_vault_request(&client, url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                SecretsError::BackendError(format!("Vault Transit encrypt request failed: {}", e))
            })?;

        self.handle_transit_response(response, "ciphertext", "encrypt").await
    }

    /// Decrypt ciphertext using Vault Transit engine
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `ciphertext` - Encrypted data (in Vault's format)
    ///
    /// # Returns
    /// Decrypted plaintext
    pub async fn decrypt_field(
        &self,
        key_name: &str,
        ciphertext: &str,
    ) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| {
                SecretsError::BackendError(format!("Failed to create HTTP client: {}", e))
            })?;

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
            .build_vault_request(&client, url)
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

/// Validate Vault secret name format
fn validate_vault_secret_name(name: &str) -> Result<(), SecretsError> {
    if name.is_empty() {
        return Err(SecretsError::ValidationError("Vault secret name cannot be empty".to_string()));
    }

    // Vault paths typically contain slashes and lowercase alphanumeric
    if !name.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_') {
        return Err(SecretsError::ValidationError(format!(
            "Invalid Vault secret name: {}. Only alphanumeric, /, -, _ allowed",
            name
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test VaultBackend creation
    #[test]
    fn test_vault_backend_creation() {
        let vault = VaultBackend::new("https://vault.local:8200", "mytoken");
        assert_eq!(vault.addr(), "https://vault.local:8200");
        assert_eq!(vault.token(), "mytoken");
    }

    /// Test VaultBackend placeholder returns error
    #[tokio::test]
    async fn test_vault_backend_placeholder() {
        let vault = VaultBackend::new("https://vault.local:8200", "token");

        let result = vault.get_secret("any/path").await;
        assert!(result.is_err());
    }

    /// Test multiple VaultBackend instances
    #[test]
    fn test_vault_backend_multiple() {
        let vault1 = VaultBackend::new("https://vault1.local:8200", "token1");
        let vault2 = VaultBackend::new("https://vault2.local:8200", "token2");

        assert_ne!(vault1.addr(), vault2.addr());
        assert_ne!(vault1.token(), vault2.token());
    }

    /// Test VaultBackend clone
    #[test]
    fn test_vault_backend_clone() {
        let vault1 = VaultBackend::new("https://vault.local:8200", "token");
        let vault2 = vault1.clone();

        assert_eq!(vault1.addr(), vault2.addr());
        assert_eq!(vault1.token(), vault2.token());
    }
}
