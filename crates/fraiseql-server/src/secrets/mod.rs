//! Secrets management with KMS-backed encryption and database schemas.
//!
//! This module provides both:
//! 1. Startup-time cached encryption (fast, for configuration)
//! 2. Per-request KMS operations (slower, for sensitive data)
//! 3. Database schema definitions for secrets management

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::security::{BaseKmsProvider, DataKeyPair, EncryptedData, KmsError, KmsResult};
use tokio::sync::RwLock;

pub mod schemas;

#[cfg(test)]
mod schema_tests;

pub use schemas::{
    EncryptionKey, ExternalAuthProviderRecord, OAuthSessionRecord, SchemaMigration,
    SecretRotationAudit,
};

/// Secret manager combining cached and per-request encryption.
pub struct SecretManager {
    /// Primary KMS provider
    provider:       Arc<dyn BaseKmsProvider>,
    /// Cached data key for local encryption
    cached_key:     Arc<RwLock<Option<DataKeyPair>>>,
    /// Default key ID for KMS operations
    default_key_id: String,
    /// Context prefix for all encryption operations
    context_prefix: Option<String>,
}

impl SecretManager {
    /// Create a new secret manager.
    pub fn new(provider: Arc<dyn BaseKmsProvider>, default_key_id: String) -> Self {
        Self {
            provider,
            cached_key: Arc::new(RwLock::new(None)),
            default_key_id,
            context_prefix: None,
        }
    }

    /// Set a context prefix (e.g., "fraiseql-prod").
    ///
    /// This prefix is added to all encryption contexts for additional
    /// isolation between environments.
    #[must_use]
    pub fn with_context_prefix(mut self, prefix: String) -> Self {
        self.context_prefix = Some(prefix);
        self
    }

    /// Initialize by generating and caching a data key.
    ///
    /// Call this at application startup. The data key is cached in memory
    /// for fast local encryption during the application's lifetime.
    ///
    /// # Errors
    /// Returns KmsError if data key generation fails
    pub async fn initialize(&self) -> KmsResult<()> {
        let mut context = HashMap::new();
        context.insert("purpose".to_string(), "data_encryption".to_string());
        let context = self.build_context(context);

        let data_key = self.provider.generate_data_key(&self.default_key_id, context).await?;

        let mut cached = self.cached_key.write().await;
        *cached = Some(data_key);

        Ok(())
    }

    /// Check if a data key is cached.
    pub async fn is_initialized(&self) -> bool {
        self.cached_key.read().await.is_some()
    }

    /// Rotate the cached data key.
    ///
    /// Call this periodically to rotate keys. This regenerates the cached
    /// data key via KMS while maintaining application uptime.
    ///
    /// # Errors
    /// Returns KmsError if rotation fails
    pub async fn rotate_cached_key(&self) -> KmsResult<()> {
        self.initialize().await
    }

    /// Encrypt data using the cached data key (NO KMS call).
    ///
    /// This is fast (~microseconds) and safe for use in hot paths.
    /// Requires `initialize()` to be called first.
    ///
    /// # Errors
    /// Returns KmsError::EncryptionFailed if not initialized or encryption fails
    pub async fn local_encrypt(&self, plaintext: &[u8]) -> KmsResult<Vec<u8>> {
        let cached = self.cached_key.read().await;
        let data_key = cached.as_ref().ok_or_else(|| KmsError::EncryptionFailed {
            message: "SecretManager not initialized. Call initialize() at startup.".to_string(),
        })?;

        // Encrypt using AES-256-GCM with the cached plaintext key
        let nonce = Self::generate_nonce();
        let ciphertext = aes_gcm_encrypt(&data_key.plaintext_key, &nonce, plaintext)?;

        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data using the cached data key (NO KMS call).
    ///
    /// # Errors
    /// Returns KmsError::DecryptionFailed if not initialized or decryption fails
    pub async fn local_decrypt(&self, encrypted: &[u8]) -> KmsResult<Vec<u8>> {
        if encrypted.len() < 12 {
            return Err(KmsError::DecryptionFailed {
                message: "Encrypted data too short".to_string(),
            });
        }

        let cached = self.cached_key.read().await;
        let data_key = cached.as_ref().ok_or_else(|| KmsError::DecryptionFailed {
            message: "SecretManager not initialized. Call initialize() at startup.".to_string(),
        })?;

        let nonce = &encrypted[..12];
        let ciphertext = &encrypted[12..];

        aes_gcm_decrypt(&data_key.plaintext_key, nonce, ciphertext)
    }

    /// Encrypt data using KMS (per-request operation).
    ///
    /// This contacts the KMS provider for each encryption, providing
    /// per-request key isolation but with higher latency (50-200ms).
    /// Use for secrets management, not response data.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `key_id` - KMS key identifier (or None for default)
    ///
    /// # Errors
    /// Returns KmsError if encryption fails
    pub async fn encrypt(
        &self,
        plaintext: &[u8],
        key_id: Option<&str>,
    ) -> KmsResult<EncryptedData> {
        let key_id = key_id.unwrap_or(&self.default_key_id);
        let mut context = HashMap::new();
        context.insert("operation".to_string(), "encrypt".to_string());
        let context = self.build_context(context);

        self.provider.encrypt(plaintext, key_id, context).await
    }

    /// Decrypt data using KMS (per-request operation).
    ///
    /// Auto-detects the correct provider from EncryptedData metadata.
    ///
    /// # Errors
    /// Returns KmsError if decryption fails
    pub async fn decrypt(&self, encrypted: &EncryptedData) -> KmsResult<Vec<u8>> {
        let mut context = HashMap::new();
        context.insert("operation".to_string(), "decrypt".to_string());
        let context = self.build_context(context);

        self.provider.decrypt(encrypted, context).await
    }

    /// Encrypt a string field (convenience method).
    ///
    /// Handles UTF-8 encoding/decoding automatically.
    pub async fn encrypt_string(
        &self,
        plaintext: &str,
        key_id: Option<&str>,
    ) -> KmsResult<EncryptedData> {
        let bytes = plaintext.as_bytes();
        self.encrypt(bytes, key_id).await
    }

    /// Decrypt a string field.
    pub async fn decrypt_string(&self, encrypted: &EncryptedData) -> KmsResult<String> {
        let plaintext = self.decrypt(encrypted).await?;
        String::from_utf8(plaintext).map_err(|e| KmsError::SerializationError {
            message: format!("Invalid UTF-8 in decrypted data: {}", e),
        })
    }

    // ─────────────────────────────────────────────────────────────
    // Private helpers
    // ─────────────────────────────────────────────────────────────

    /// Build encryption context with optional prefix.
    fn build_context(
        &self,
        mut context: HashMap<String, String>,
    ) -> Option<HashMap<String, String>> {
        if let Some(prefix) = &self.context_prefix {
            context.insert("service".to_string(), prefix.clone());
        }

        if context.is_empty() {
            None
        } else {
            Some(context)
        }
    }

    /// Generate a 96-bit nonce for AES-GCM.
    fn generate_nonce() -> [u8; 12] {
        use rand::RngCore;
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        nonce
    }
}

/// AES-256-GCM encryption using aes-gcm.
fn aes_gcm_encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8]) -> KmsResult<Vec<u8>> {
    use aes_gcm::{
        Aes256Gcm, Key, Nonce,
        aead::{Aead, KeyInit},
    };

    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce);

    cipher.encrypt(nonce, plaintext).map_err(|e| KmsError::EncryptionFailed {
        message: format!("AES-GCM encryption failed: {}", e),
    })
}

/// AES-256-GCM decryption using aes-gcm.
fn aes_gcm_decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> KmsResult<Vec<u8>> {
    use aes_gcm::{
        Aes256Gcm, Key, Nonce,
        aead::{Aead, KeyInit},
    };

    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce);

    cipher.decrypt(nonce, ciphertext).map_err(|e| KmsError::DecryptionFailed {
        message: format!("AES-GCM decryption failed: {}", e),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fraiseql_core::security::{KmsError, KmsResult};

    use super::*;

    /// Mock KMS provider for testing
    struct MockKmsProvider;

    #[async_trait::async_trait]
    impl BaseKmsProvider for MockKmsProvider {
        fn provider_name(&self) -> &'static str {
            "mock"
        }

        async fn do_encrypt(
            &self,
            plaintext: &[u8],
            _key_id: &str,
            _context: &HashMap<String, String>,
        ) -> KmsResult<(String, String)> {
            // Return base64-encoded plaintext as mock ciphertext
            Ok((base64_encode(plaintext), "mock-algorithm".to_string()))
        }

        async fn do_decrypt(
            &self,
            ciphertext: &str,
            _key_id: &str,
            _context: &HashMap<String, String>,
        ) -> KmsResult<Vec<u8>> {
            base64_decode(ciphertext)
        }

        async fn do_generate_data_key(
            &self,
            _key_id: &str,
            _context: &HashMap<String, String>,
        ) -> KmsResult<(Vec<u8>, String)> {
            let key = vec![0u8; 32]; // 256-bit key
            let encrypted = base64_encode(&key);
            Ok((key, encrypted))
        }

        async fn do_rotate_key(&self, _key_id: &str) -> KmsResult<()> {
            Ok(())
        }

        async fn do_get_key_info(
            &self,
            _key_id: &str,
        ) -> KmsResult<fraiseql_core::security::kms::base::KeyInfo> {
            Ok(fraiseql_core::security::kms::base::KeyInfo {
                alias:      Some("mock-key".to_string()),
                created_at: 1_000_000,
            })
        }

        async fn do_get_rotation_policy(
            &self,
            _key_id: &str,
        ) -> KmsResult<fraiseql_core::security::kms::base::RotationPolicyInfo> {
            Ok(fraiseql_core::security::kms::base::RotationPolicyInfo {
                enabled:              false,
                rotation_period_days: 0,
                last_rotation:        None,
                next_rotation:        None,
            })
        }
    }

    fn base64_encode(data: &[u8]) -> String {
        use base64::prelude::*;
        BASE64_STANDARD.encode(data)
    }

    fn base64_decode(s: &str) -> KmsResult<Vec<u8>> {
        use base64::prelude::*;
        BASE64_STANDARD.decode(s).map_err(|e| KmsError::SerializationError {
            message: e.to_string(),
        })
    }

    #[tokio::test]
    async fn test_secret_manager_initialization() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string());

        assert!(!manager.is_initialized().await);
        assert!(manager.initialize().await.is_ok());
        assert!(manager.is_initialized().await);
    }

    #[tokio::test]
    async fn test_local_encrypt_decrypt_roundtrip() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string());
        manager.initialize().await.unwrap();

        let plaintext = b"secret data";
        let encrypted = manager.local_encrypt(plaintext).await.unwrap();
        let decrypted = manager.local_decrypt(&encrypted).await.unwrap();

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[tokio::test]
    async fn test_local_encrypt_without_initialization() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string());

        let result = manager.local_encrypt(b"secret").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_via_kms() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string());

        let plaintext = b"sensitive data";
        let encrypted = manager.encrypt(plaintext, None).await.unwrap();
        let decrypted = manager.decrypt(&encrypted).await.unwrap();

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[tokio::test]
    async fn test_encrypt_string_roundtrip() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string());

        let plaintext = "secret string";
        let encrypted = manager.encrypt_string(plaintext, None).await.unwrap();
        let decrypted = manager.decrypt_string(&encrypted).await.unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[tokio::test]
    async fn test_context_prefix() {
        let provider = Arc::new(MockKmsProvider);
        let manager = SecretManager::new(provider, "test-key".to_string())
            .with_context_prefix("fraiseql-prod".to_string());

        assert!(manager.encrypt(b"data", None).await.is_ok());
    }
}
