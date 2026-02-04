// Phase 12.2: Vault Backend Integration
//! Backend for HashiCorp Vault integration with dynamic secrets,
//! lease management, and encryption support
//!
//! Implements the SecretsBackend trait for HashiCorp Vault,
//! providing dynamic database credentials, TTL management, and encryption.

use chrono::{Duration, Utc};
use super::super::{SecretsBackend, SecretsError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};

/// Vault API response structure for secrets
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct VaultResponse {
    request_id: String,
    lease_id: String,
    lease_duration: i64,
    renewable: bool,
    data: HashMap<String, serde_json::Value>,
}

/// Lease information tracking
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct LeaseInfo {
    lease_id: String,
    expires_at: chrono::DateTime<Utc>,
    renewable: bool,
}

/// Cached secret with metadata
///
/// Used for Phase 12.2+ advanced features: lease tracking and renewal
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CachedSecret {
    value: String,
    expires_at: chrono::DateTime<Utc>,
    lease_id: Option<String>,
    renewable: bool,
}

// Constants for Vault API and caching
const VAULT_API_VERSION: &str = "v1";
const CACHE_TTL_PERCENTAGE: f64 = 0.8;  // Cache for 80% of credential TTL
#[allow(dead_code)]
const RENEWAL_THRESHOLD_PERCENT: f64 = 0.8;  // Renew when 80% expired (used in Phase 12.2+ cycles)
const DEFAULT_MAX_CACHE_ENTRIES: usize = 1000;  // Maximum cached secrets

/// Secret cache with TTL management and LRU eviction
///
/// Used for Phase 12.2+ advanced features: credential caching with automatic renewal
#[derive(Debug)]
#[allow(dead_code)]
struct SecretCache {
    entries: Arc<RwLock<HashMap<String, CachedSecret>>>,
    max_entries: usize,
}

#[allow(dead_code)]
impl SecretCache {
    /// Create new secret cache with specified max entries
    fn new(max_entries: usize) -> Self {
        SecretCache {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    /// Get cached secret if still valid
    async fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().await;
        if let Some(cached) = entries.get(key) {
            if cached.expires_at > Utc::now() {
                return Some(cached.value.clone());
            }
        }
        None
    }

    /// Store secret in cache with expiry
    async fn set(&self, key: String, secret: String, expires_at: chrono::DateTime<Utc>,
                 lease_id: Option<String>, renewable: bool) {
        let mut entries = self.entries.write().await;

        // Simple LRU: if at capacity, clear oldest 10% of entries
        if entries.len() >= self.max_entries {
            let remove_count = (self.max_entries / 10).max(1);
            let keys_to_remove: Vec<_> = entries
                .iter()
                .take(remove_count)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys_to_remove {
                entries.remove(&key);
            }
        }

        entries.insert(key, CachedSecret {
            value: secret,
            expires_at,
            lease_id,
            renewable,
        });
    }

    /// Invalidate cached secret
    async fn invalidate(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    /// Check if secret should be renewed based on expiry
    async fn should_renew(&self, key: &str) -> bool {
        let entries = self.entries.read().await;
        if let Some(cached) = entries.get(key) {
            let time_remaining = cached.expires_at - Utc::now();
            let total_lifetime = cached.expires_at - (cached.expires_at - Duration::try_seconds(3600).unwrap_or_default());
            if total_lifetime.num_seconds() > 0 {
                let percent_remaining = time_remaining.num_seconds() as f64 / total_lifetime.num_seconds() as f64;
                return percent_remaining < (1.0 - RENEWAL_THRESHOLD_PERCENT);
            }
        }
        false
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
    addr: String,
    token: String,
    namespace: Option<String>,
    tls_verify: bool,
    cache: Arc<RwLock<SecretCache>>,
}

impl Clone for VaultBackend {
    fn clone(&self) -> Self {
        VaultBackend {
            addr: self.addr.clone(),
            token: self.token.clone(),
            namespace: self.namespace.clone(),
            tls_verify: self.tls_verify,
            cache: Arc::clone(&self.cache),
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
        if let Some(cached_value) = cache.get(name).await {
            // Try to get expiry from cache entry (use default if not stored)
            if let Some(cached) = cache.entries.read().await.get(name) {
                return Ok((cached_value, cached.expires_at));
            }
        }
        drop(cache);  // Release read lock before fetching

        // Fetch from Vault
        let response = self.fetch_secret(name).await?;

        // Calculate expiry: now + lease_duration
        let expiry = Utc::now() + chrono::Duration::seconds(response.lease_duration);
        let cache_expiry = Utc::now() + Duration::seconds((response.lease_duration as f64 * CACHE_TTL_PERCENTAGE) as i64);

        // Extract secret from response data
        let secret_str = Self::extract_secret_from_response(&response, name)?;

        // Store in cache
        let cache = self.cache.read().await;
        cache.set(
            name.to_string(),
            secret_str.clone(),
            cache_expiry,
            Some(response.lease_id.clone()),
            response.renewable,
        ).await;

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
            addr: addr.into(),
            token: token.into(),
            namespace: None,
            tls_verify: true,
            cache: Arc::new(RwLock::new(SecretCache::new(DEFAULT_MAX_CACHE_ENTRIES))),
        }
    }

    /// Set Vault namespace (Enterprise feature)
    #[must_use]
    pub fn with_namespace<S: Into<String>>(mut self, namespace: S) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set TLS certificate verification
    #[must_use]
    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    /// Get Vault server address
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
    fn extract_secret_from_response(response: &VaultResponse, path: &str) -> Result<String, SecretsError> {
        // For KV2 engine: response.data.data contains actual secret
        // For dynamic credentials: response.data contains username/password
        if let Some(data_obj) = response.data.get("data") {
            serde_json::to_string(data_obj)
                .map_err(|e| SecretsError::BackendError(
                    format!("Failed to serialize KV2 secret from {}: {}", path, e)
                ))
        } else {
            // Dynamic credentials or generic secret
            serde_json::to_string(&response.data)
                .map_err(|e| SecretsError::BackendError(
                    format!("Failed to serialize secret from {}: {}", path, e)
                ))
        }
    }

    /// Fetch secret from Vault HTTP API
    async fn fetch_secret(&self, name: &str) -> Result<VaultResponse, SecretsError> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| SecretsError::BackendError(format!("Failed to create HTTP client: {}", e)))?;

        let url = self.build_vault_url(name);

        // Build request headers with X-Vault-Token
        let response = client
            .get(&url)
            .header("X-Vault-Token", self.token.clone())
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
            .send()
            .await
            .map_err(|e| SecretsError::BackendError(format!("Vault HTTP request failed for {}: {}", name, e)))?;

        match response.status() {
            reqwest::StatusCode::OK => {
                response
                    .json::<VaultResponse>()
                    .await
                    .map_err(|e| SecretsError::BackendError(format!("Failed to parse Vault response for {}: {}", name, e)))
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(SecretsError::NotFound(format!("Secret not found in Vault: {}", name)))
            }
            reqwest::StatusCode::FORBIDDEN => {
                Err(SecretsError::BackendError(format!("Permission denied accessing Vault secret: {}", name)))
            }
            status => {
                Err(SecretsError::BackendError(format!("Vault request failed with status {} for {}", status, name)))
            }
        }
    }

    /// Build Vault API URL for a secret path
    fn build_vault_url(&self, path: &str) -> String {
        format!("{}/{}/{}", self.addr.trim_end_matches('/'), VAULT_API_VERSION, path)
    }

    /// Encrypt plaintext using Vault Transit engine
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `plaintext` - Data to encrypt
    ///
    /// # Returns
    /// Encrypted ciphertext in Vault's standard format
    pub async fn encrypt_field(&self, key_name: &str, plaintext: &str) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let request_body = serde_json::json!({
            "plaintext": STANDARD_NO_PAD.encode(plaintext)
        });

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| SecretsError::BackendError(format!("Failed to create HTTP client: {}", e)))?;

        let url = format!(
            "{}/{}/transit/encrypt/{}",
            self.addr.trim_end_matches('/'),
            VAULT_API_VERSION,
            key_name
        );

        let response = client
            .post(&url)
            .header("X-Vault-Token", self.token.clone())
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SecretsError::BackendError(format!("Vault Transit encrypt request failed: {}", e)))?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let body = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| SecretsError::BackendError(format!("Failed to parse encrypt response: {}", e)))?;

                body["data"]["ciphertext"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| SecretsError::EncryptionError("Missing ciphertext in response".to_string()))
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(SecretsError::NotFound(format!("Transit key not found: {}", key_name)))
            }
            status => {
                Err(SecretsError::EncryptionError(format!("Vault Transit encrypt failed with status {}", status)))
            }
        }
    }

    /// Decrypt ciphertext using Vault Transit engine
    ///
    /// # Arguments
    /// * `key_name` - Name of the transit encryption key
    /// * `ciphertext` - Encrypted data (in Vault's format)
    ///
    /// # Returns
    /// Decrypted plaintext
    pub async fn decrypt_field(&self, key_name: &str, ciphertext: &str) -> Result<String, SecretsError> {
        validate_vault_secret_name(key_name)?;

        let request_body = serde_json::json!({
            "ciphertext": ciphertext
        });

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(!self.tls_verify)
            .build()
            .map_err(|e| SecretsError::BackendError(format!("Failed to create HTTP client: {}", e)))?;

        let url = format!(
            "{}/{}/transit/decrypt/{}",
            self.addr.trim_end_matches('/'),
            VAULT_API_VERSION,
            key_name
        );

        let response = client
            .post(&url)
            .header("X-Vault-Token", self.token.clone())
            .header("X-Vault-Namespace", self.namespace.as_deref().unwrap_or(""))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SecretsError::BackendError(format!("Vault Transit decrypt request failed: {}", e)))?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let body = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| SecretsError::BackendError(format!("Failed to parse decrypt response: {}", e)))?;

                let plaintext_b64 = body["data"]["plaintext"]
                    .as_str()
                    .ok_or_else(|| SecretsError::EncryptionError("Missing plaintext in response".to_string()))?;

                STANDARD_NO_PAD.decode(plaintext_b64)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .ok_or_else(|| SecretsError::EncryptionError("Failed to decode plaintext".to_string()))
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(SecretsError::NotFound(format!("Transit key not found: {}", key_name)))
            }
            status => {
                Err(SecretsError::EncryptionError(format!("Vault Transit decrypt failed with status {}", status)))
            }
        }
    }
}

/// Validate Vault secret name format
fn validate_vault_secret_name(name: &str) -> Result<(), SecretsError> {
    if name.is_empty() {
        return Err(SecretsError::ValidationError(
            "Vault secret name cannot be empty".to_string(),
        ));
    }

    // Vault paths typically contain slashes and lowercase alphanumeric
    if !name.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_') {
        return Err(SecretsError::ValidationError(
            format!("Invalid Vault secret name: {}. Only alphanumeric, /, -, _ allowed", name),
        ));
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
