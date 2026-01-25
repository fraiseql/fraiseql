//! Base KMS provider trait with template method pattern.
//!
//! Provides public async methods with common logic and abstract hooks
//! for provider-specific implementations.

use crate::security::kms::error::{KmsError, KmsResult};
use crate::security::kms::models::{
    DataKeyPair, EncryptedData, KeyPurpose, KeyReference, RotationPolicy,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current Unix timestamp.
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Abstract base class for KMS providers.
///
/// Implements the Template Method pattern:
/// - Public methods (encrypt, decrypt, etc.) handle common logic
/// - Protected abstract methods (_do_encrypt, _do_decrypt, etc.) are
///   implemented by concrete providers
#[async_trait::async_trait]
pub trait BaseKmsProvider: Send + Sync {
    /// Unique provider identifier (e.g., 'vault', 'aws', 'gcp').
    fn provider_name(&self) -> &str;

    // ─────────────────────────────────────────────────────────────
    // Template Methods (public API)
    // ─────────────────────────────────────────────────────────────

    /// Encrypt data using the specified key.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `key_id` - Key identifier
    /// * `context` - Additional authenticated data (AAD)
    ///
    /// # Returns
    /// EncryptedData with ciphertext and metadata
    ///
    /// # Errors
    /// Returns KmsError::EncryptionFailed if encryption fails
    async fn encrypt(
        &self,
        plaintext: &[u8],
        key_id: &str,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<EncryptedData> {
        let ctx = context.unwrap_or_default();

        let (ciphertext, algorithm) = self
            ._do_encrypt(plaintext, key_id, &ctx)
            .await
            .map_err(|e| KmsError::EncryptionFailed {
                message: format!("Provider encryption failed: {}", e),
            })?;

        Ok(EncryptedData::new(
            ciphertext,
            KeyReference::new(
                self.provider_name().to_string(),
                key_id.to_string(),
                KeyPurpose::EncryptDecrypt,
                current_timestamp(),
            ),
            algorithm,
            current_timestamp(),
            ctx,
        ))
    }

    /// Decrypt data.
    ///
    /// # Arguments
    /// * `encrypted` - EncryptedData to decrypt
    /// * `context` - Override context (uses encrypted.context if not provided)
    ///
    /// # Returns
    /// Decrypted plaintext bytes
    ///
    /// # Errors
    /// Returns KmsError::DecryptionFailed if decryption fails
    async fn decrypt(
        &self,
        encrypted: &EncryptedData,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<Vec<u8>> {
        let ctx = context.unwrap_or_else(|| encrypted.context.clone());
        let key_id = &encrypted.key_reference.key_id;

        self._do_decrypt(&encrypted.ciphertext, key_id, &ctx)
            .await
            .map_err(|e| KmsError::DecryptionFailed {
                message: format!("Provider decryption failed: {}", e),
            })
    }

    /// Generate a data encryption key (envelope encryption).
    ///
    /// # Arguments
    /// * `key_id` - Master key to wrap the data key
    /// * `context` - Additional authenticated data
    ///
    /// # Returns
    /// DataKeyPair with plaintext and encrypted data key
    async fn generate_data_key(
        &self,
        key_id: &str,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<DataKeyPair> {
        let ctx = context.unwrap_or_default();

        let (plaintext_key, encrypted_key_bytes) = self
            ._do_generate_data_key(key_id, &ctx)
            .await
            .map_err(|e| KmsError::EncryptionFailed {
                message: format!("Data key generation failed: {}", e),
            })?;

        let key_ref = KeyReference::new(
            self.provider_name().to_string(),
            key_id.to_string(),
            KeyPurpose::EncryptDecrypt,
            current_timestamp(),
        );

        Ok(DataKeyPair::new(
            plaintext_key,
            EncryptedData::new(
                encrypted_key_bytes,
                key_ref.clone(),
                "data-key".to_string(),
                current_timestamp(),
                ctx,
            ),
            key_ref,
        ))
    }

    /// Rotate the specified key.
    ///
    /// # Errors
    /// Returns KmsError::RotationFailed if rotation fails
    async fn rotate_key(&self, key_id: &str) -> KmsResult<KeyReference> {
        self._do_rotate_key(key_id)
            .await
            .map_err(|e| KmsError::RotationFailed {
                message: format!("Provider rotation failed: {}", e),
            })?;

        self.get_key_info(key_id).await
    }

    /// Get key metadata.
    ///
    /// # Errors
    /// Returns KmsError::KeyNotFound if key does not exist
    async fn get_key_info(&self, key_id: &str) -> KmsResult<KeyReference> {
        let info = self
            ._do_get_key_info(key_id)
            .await
            .map_err(|_e| KmsError::KeyNotFound {
                key_id: key_id.to_string(),
            })?;

        Ok(KeyReference::new(
            self.provider_name().to_string(),
            key_id.to_string(),
            KeyPurpose::EncryptDecrypt,
            info.created_at,
        )
        .with_alias(info.alias.unwrap_or_default()))
    }

    /// Get key rotation policy.
    ///
    /// # Errors
    /// Returns KmsError::KeyNotFound if key does not exist
    async fn get_rotation_policy(&self, key_id: &str) -> KmsResult<RotationPolicy> {
        let policy = self
            ._do_get_rotation_policy(key_id)
            .await
            .map_err(|_e| KmsError::KeyNotFound {
                key_id: key_id.to_string(),
            })?;

        Ok(RotationPolicy {
            enabled: policy.enabled,
            rotation_period_days: policy.rotation_period_days,
            last_rotation: policy.last_rotation,
            next_rotation: policy.next_rotation,
        })
    }

    // ─────────────────────────────────────────────────────────────
    // Abstract Methods (provider-specific hooks)
    // ─────────────────────────────────────────────────────────────

    /// Provider-specific encryption.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `key_id` - Key identifier
    /// * `context` - AAD context (never empty)
    ///
    /// # Returns
    /// Tuple of (ciphertext, algorithm_name) on success
    async fn _do_encrypt(
        &self,
        plaintext: &[u8],
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<(String, String)>;

    /// Provider-specific decryption.
    ///
    /// # Arguments
    /// * `ciphertext` - Data to decrypt
    /// * `key_id` - Key identifier
    /// * `context` - AAD context (never empty)
    ///
    /// # Returns
    /// Decrypted plaintext bytes
    async fn _do_decrypt(
        &self,
        ciphertext: &str,
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<Vec<u8>>;

    /// Provider-specific data key generation.
    ///
    /// # Arguments
    /// * `key_id` - Master key identifier
    /// * `context` - AAD context (never empty)
    ///
    /// # Returns
    /// Tuple of (plaintext_key, encrypted_key_hex)
    async fn _do_generate_data_key(
        &self,
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<(Vec<u8>, String)>;

    /// Provider-specific key rotation.
    async fn _do_rotate_key(&self, key_id: &str) -> KmsResult<()>;

    /// Provider-specific key info retrieval.
    ///
    /// Returns KeyInfo struct with alias and created_at
    async fn _do_get_key_info(&self, key_id: &str) -> KmsResult<KeyInfo>;

    /// Provider-specific rotation policy retrieval.
    async fn _do_get_rotation_policy(&self, key_id: &str) -> KmsResult<RotationPolicyInfo>;
}

/// Key information returned by provider.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub alias: Option<String>,
    pub created_at: i64,
}

/// Rotation policy info returned by provider.
#[derive(Debug, Clone)]
pub struct RotationPolicyInfo {
    pub enabled: bool,
    pub rotation_period_days: u32,
    pub last_rotation: Option<i64>,
    pub next_rotation: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_current_timestamp_is_positive() {
        let ts = current_timestamp();
        assert!(ts > 0);
    }
}
