//! HashiCorp Vault Transit secrets engine provider.

use std::collections::HashMap;

use serde_json::json;

use crate::security::kms::{
    base::{BaseKmsProvider, KeyInfo, RotationPolicyInfo},
    error::{KmsError, KmsResult},
};

/// Configuration for Vault KMS provider.
///
/// # Security Considerations
/// Token Handling:
/// - The Vault token is stored in memory for the provider's lifetime
/// - For production deployments, consider:
///   1. Using short-lived tokens with automatic renewal
///   2. Vault Agent with auto-auth for token management
///   3. AppRole authentication with response wrapping
///   4. Kubernetes auth method in K8s environments
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Vault server address (e.g., "https://vault.example.com")
    pub vault_addr: String,
    /// Vault authentication token
    pub token:      String,
    /// Transit mount path (default: "transit")
    pub mount_path: String,
    /// Optional Vault namespace
    pub namespace:  Option<String>,
    /// Verify TLS certificates (default: true)
    pub verify_tls: bool,
    /// Request timeout in seconds (default: 30)
    pub timeout:    u64,
}

impl VaultConfig {
    /// Create a new Vault configuration.
    pub fn new(vault_addr: String, token: String) -> Self {
        Self {
            vault_addr,
            token,
            mount_path: "transit".to_string(),
            namespace: None,
            verify_tls: true,
            timeout: 30,
        }
    }

    /// Set the transit mount path.
    #[must_use]
    pub fn with_mount_path(mut self, mount_path: String) -> Self {
        self.mount_path = mount_path;
        self
    }

    /// Set the Vault namespace.
    #[must_use]
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Set TLS verification.
    #[must_use]
    pub fn with_verify_tls(mut self, verify_tls: bool) -> Self {
        self.verify_tls = verify_tls;
        self
    }

    /// Set request timeout in seconds.
    #[must_use]
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build full API URL for a path.
    fn api_url(&self, path: &str) -> String {
        let addr = self.vault_addr.trim_end_matches('/');
        format!("{}/v1/{}/{}", addr, self.mount_path, path)
    }
}

/// HashiCorp Vault Transit secrets engine provider.
///
/// Uses Vault's Transit secrets engine for encryption/decryption operations.
/// Supports envelope encryption via data key generation.
///
/// All operations use authenticated encryption (AES-256-GCM).
pub struct VaultKmsProvider {
    config: VaultConfig,
    client: reqwest::Client,
}

impl VaultKmsProvider {
    /// Create a new Vault KMS provider.
    pub fn new(config: VaultConfig) -> KmsResult<Self> {
        let client = reqwest::Client::new();
        Ok(Self { config, client })
    }

    /// Build a request with Vault headers.
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();

        headers.insert(
            "X-Vault-Token",
            reqwest::header::HeaderValue::from_str(&self.config.token)
                .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
        );

        if let Some(namespace) = &self.config.namespace {
            headers.insert(
                "X-Vault-Namespace",
                reqwest::header::HeaderValue::from_str(namespace)
                    .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
            );
        }

        headers
    }
}

#[async_trait::async_trait]
impl BaseKmsProvider for VaultKmsProvider {
    fn provider_name(&self) -> &'static str {
        "vault"
    }

    async fn do_encrypt(
        &self,
        plaintext: &[u8],
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<(String, String)> {
        let url = self.config.api_url(&format!("encrypt/{}", key_id));

        let plaintext_b64 = base64_encode(plaintext);

        let mut payload = json!({
            "plaintext": plaintext_b64,
        });

        // Add context if provided (used for key derivation)
        if !context.is_empty() {
            let context_json =
                serde_json::to_string(context).map_err(|e| KmsError::SerializationError {
                    message: e.to_string(),
                })?;
            let context_b64 = base64_encode(context_json.as_bytes());
            payload["context"] = json!(context_b64);
        }

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&payload)
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(KmsError::EncryptionFailed {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        let data = response.json::<serde_json::Value>().await.map_err(|e| {
            KmsError::SerializationError {
                message: e.to_string(),
            }
        })?;

        let ciphertext = data["data"]["ciphertext"]
            .as_str()
            .ok_or_else(|| KmsError::EncryptionFailed {
                message: "No ciphertext in Vault response".to_string(),
            })?
            .to_string();

        Ok((ciphertext, "aes256-gcm96".to_string()))
    }

    async fn do_decrypt(
        &self,
        ciphertext: &str,
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<Vec<u8>> {
        let url = self.config.api_url(&format!("decrypt/{}", key_id));

        let mut payload = json!({
            "ciphertext": ciphertext,
        });

        // Add context if provided
        if !context.is_empty() {
            let context_json =
                serde_json::to_string(context).map_err(|e| KmsError::SerializationError {
                    message: e.to_string(),
                })?;
            let context_b64 = base64_encode(context_json.as_bytes());
            payload["context"] = json!(context_b64);
        }

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&payload)
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(KmsError::DecryptionFailed {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        let data = response.json::<serde_json::Value>().await.map_err(|e| {
            KmsError::SerializationError {
                message: e.to_string(),
            }
        })?;

        let plaintext_b64 =
            data["data"]["plaintext"].as_str().ok_or_else(|| KmsError::DecryptionFailed {
                message: "No plaintext in Vault response".to_string(),
            })?;

        base64_decode(plaintext_b64).map_err(|_| KmsError::DecryptionFailed {
            message: "Failed to decode plaintext from Vault".to_string(),
        })
    }

    async fn do_generate_data_key(
        &self,
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<(Vec<u8>, String)> {
        let url = self.config.api_url(&format!("datakey/plaintext/{}", key_id));

        let mut payload = json!({
            "bits": 256,  // AES-256
        });

        // Add context if provided
        if !context.is_empty() {
            let context_json =
                serde_json::to_string(context).map_err(|e| KmsError::SerializationError {
                    message: e.to_string(),
                })?;
            let context_b64 = base64_encode(context_json.as_bytes());
            payload["context"] = json!(context_b64);
        }

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&payload)
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(KmsError::EncryptionFailed {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        let data = response.json::<serde_json::Value>().await.map_err(|e| {
            KmsError::SerializationError {
                message: e.to_string(),
            }
        })?;

        let plaintext_b64 =
            data["data"]["plaintext"].as_str().ok_or_else(|| KmsError::EncryptionFailed {
                message: "No plaintext key in Vault response".to_string(),
            })?;

        let plaintext_key =
            base64_decode(plaintext_b64).map_err(|_| KmsError::EncryptionFailed {
                message: "Failed to decode plaintext key from Vault".to_string(),
            })?;

        let ciphertext = data["data"]["ciphertext"]
            .as_str()
            .ok_or_else(|| KmsError::EncryptionFailed {
                message: "No encrypted key in Vault response".to_string(),
            })?
            .to_string();

        Ok((plaintext_key, ciphertext))
    }

    async fn do_rotate_key(&self, key_id: &str) -> KmsResult<()> {
        let url = self.config.api_url(&format!("keys/{}/rotate", key_id));

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&json!({}))
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(KmsError::RotationFailed {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        Ok(())
    }

    async fn do_get_key_info(&self, key_id: &str) -> KmsResult<KeyInfo> {
        let url = self.config.api_url(&format!("keys/{}", key_id));

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers())
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if response.status() == 404 {
            return Err(KmsError::KeyNotFound {
                key_id: key_id.to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(KmsError::ProviderConnectionError {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        let data = response.json::<serde_json::Value>().await.map_err(|e| {
            KmsError::SerializationError {
                message: e.to_string(),
            }
        })?;

        let key_data = &data["data"];
        let alias = key_data["name"].as_str().map(|s| s.to_string());
        let created_at = key_data["creation_time"]
            .as_i64()
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        Ok(KeyInfo { alias, created_at })
    }

    async fn do_get_rotation_policy(&self, key_id: &str) -> KmsResult<RotationPolicyInfo> {
        let url = self.config.api_url(&format!("keys/{}", key_id));

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers())
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await
            .map_err(|e| KmsError::ProviderConnectionError {
                message: e.to_string(),
            })?;

        if response.status() == 404 {
            return Err(KmsError::KeyNotFound {
                key_id: key_id.to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(KmsError::ProviderConnectionError {
                message: format!("Vault returned status {}", response.status()),
            });
        }

        let _data = response.json::<serde_json::Value>().await.map_err(|e| {
            KmsError::SerializationError {
                message: e.to_string(),
            }
        })?;

        // Vault doesn't have explicit rotation policies in transit engine
        // Return disabled by default
        Ok(RotationPolicyInfo {
            enabled:              false,
            rotation_period_days: 0,
            last_rotation:        None,
            next_rotation:        None,
        })
    }
}

/// Encode bytes as base64.
fn base64_encode(data: &[u8]) -> String {
    use base64::prelude::*;
    BASE64_STANDARD.encode(data)
}

/// Decode base64 to bytes.
fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::prelude::*;
    BASE64_STANDARD.decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_config_api_url() {
        let config =
            VaultConfig::new("https://vault.example.com".to_string(), "token123".to_string());
        assert_eq!(
            config.api_url("encrypt/my-key"),
            "https://vault.example.com/v1/transit/encrypt/my-key"
        );
    }

    #[test]
    fn test_vault_config_custom_mount_path() {
        let config =
            VaultConfig::new("https://vault.example.com".to_string(), "token123".to_string())
                .with_mount_path("custom-transit".to_string());

        assert_eq!(
            config.api_url("encrypt/my-key"),
            "https://vault.example.com/v1/custom-transit/encrypt/my-key"
        );
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}
