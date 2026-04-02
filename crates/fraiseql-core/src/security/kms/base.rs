//! Base KMS provider trait with template method pattern.
//!
//! Provides public async methods with common logic and abstract hooks
//! for provider-specific implementations.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::{
    security::kms::{
        error::{KmsError, KmsResult},
        models::{DataKeyPair, EncryptedData, KeyPurpose, KeyReference, RotationPolicy},
    },
    utils::clock::{Clock, SystemClock},
};

/// Abstract base class for KMS providers.
///
/// Implements the Template Method pattern:
/// - Public methods (encrypt, decrypt, etc.) handle common logic
/// - Protected abstract methods (`do_encrypt`, `do_decrypt`, etc.) are implemented by concrete
///   providers
// Reason: used as dyn Trait (Arc<dyn BaseKmsProvider>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait BaseKmsProvider: Send + Sync {
    /// Unique provider identifier (e.g., 'vault', 'aws', 'gcp').
    fn provider_name(&self) -> &str;

    /// Return the current Unix timestamp in seconds as a signed integer.
    ///
    /// Override in tests to return a fixed timestamp, enabling deterministic
    /// testing of key-rotation scheduling without real-time delays.
    ///
    /// The default implementation delegates to [`SystemClock`].
    fn timestamp_secs(&self) -> i64 {
        SystemClock.now_secs_i64()
    }

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
    /// `EncryptedData` with ciphertext and metadata
    ///
    /// # Errors
    /// Returns `KmsError::EncryptionFailed` if encryption fails
    async fn encrypt(
        &self,
        plaintext: &[u8],
        key_id: &str,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<EncryptedData> {
        let ctx = context.unwrap_or_default();

        let (ciphertext, algorithm) =
            self.do_encrypt(plaintext, key_id, &ctx).await.map_err(|e| {
                KmsError::EncryptionFailed {
                    message: format!("Provider encryption failed: {}", e),
                }
            })?;

        Ok(EncryptedData::new(
            ciphertext,
            KeyReference::new(
                self.provider_name().to_string(),
                key_id.to_string(),
                KeyPurpose::EncryptDecrypt,
                self.timestamp_secs(),
            ),
            algorithm,
            self.timestamp_secs(),
            ctx,
        ))
    }

    /// Decrypt data.
    ///
    /// # Arguments
    /// * `encrypted` - `EncryptedData` to decrypt
    /// * `context` - Override context (uses encrypted.context if not provided)
    ///
    /// # Returns
    /// Decrypted plaintext bytes
    ///
    /// # Errors
    /// Returns `KmsError::DecryptionFailed` if decryption fails
    async fn decrypt(
        &self,
        encrypted: &EncryptedData,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<Vec<u8>> {
        let ctx = context.unwrap_or_else(|| encrypted.context.clone());
        let key_id = &encrypted.key_reference.key_id;

        self.do_decrypt(&encrypted.ciphertext, key_id, &ctx).await.map_err(|e| {
            KmsError::DecryptionFailed {
                message: format!("Provider decryption failed: {}", e),
            }
        })
    }

    /// Generate a data encryption key (envelope encryption).
    ///
    /// # Arguments
    /// * `key_id` - Master key to wrap the data key
    /// * `context` - Additional authenticated data
    ///
    /// # Returns
    /// `DataKeyPair` with plaintext and encrypted data key
    async fn generate_data_key(
        &self,
        key_id: &str,
        context: Option<HashMap<String, String>>,
    ) -> KmsResult<DataKeyPair> {
        let ctx = context.unwrap_or_default();

        let (plaintext_key, encrypted_key_bytes) = self
            .do_generate_data_key(key_id, &ctx)
            .await
            .map_err(|e| KmsError::EncryptionFailed {
                message: format!("Data key generation failed: {}", e),
            })?;

        let key_ref = KeyReference::new(
            self.provider_name().to_string(),
            key_id.to_string(),
            KeyPurpose::EncryptDecrypt,
            self.timestamp_secs(),
        );

        Ok(DataKeyPair::new(
            plaintext_key,
            EncryptedData::new(
                encrypted_key_bytes,
                key_ref.clone(),
                "data-key".to_string(),
                self.timestamp_secs(),
                ctx,
            ),
            key_ref,
        ))
    }

    /// Rotate the specified key.
    ///
    /// # Errors
    /// Returns `KmsError::RotationFailed` if rotation fails
    async fn rotate_key(&self, key_id: &str) -> KmsResult<KeyReference> {
        self.do_rotate_key(key_id).await.map_err(|e| KmsError::RotationFailed {
            message: format!("Provider rotation failed: {}", e),
        })?;

        self.get_key_info(key_id).await
    }

    /// Get key metadata.
    ///
    /// # Errors
    /// Returns `KmsError::KeyNotFound` if key does not exist
    async fn get_key_info(&self, key_id: &str) -> KmsResult<KeyReference> {
        let info = self.do_get_key_info(key_id).await.map_err(|_e| KmsError::KeyNotFound {
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
    /// Returns `KmsError::KeyNotFound` if key does not exist
    async fn get_rotation_policy(&self, key_id: &str) -> KmsResult<RotationPolicy> {
        let policy =
            self.do_get_rotation_policy(key_id).await.map_err(|_e| KmsError::KeyNotFound {
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
    /// Tuple of (ciphertext, `algorithm_name`) on success
    async fn do_encrypt(
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
    async fn do_decrypt(
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
    /// Tuple of (`plaintext_key`, `encrypted_key_hex`)
    async fn do_generate_data_key(
        &self,
        key_id: &str,
        context: &HashMap<String, String>,
    ) -> KmsResult<(Vec<u8>, String)>;

    /// Provider-specific key rotation.
    async fn do_rotate_key(&self, key_id: &str) -> KmsResult<()>;

    /// Provider-specific key info retrieval.
    ///
    /// Returns `KeyInfo` struct with alias and `created_at`
    async fn do_get_key_info(&self, key_id: &str) -> KmsResult<KeyInfo>;

    /// Provider-specific rotation policy retrieval.
    async fn do_get_rotation_policy(&self, key_id: &str) -> KmsResult<RotationPolicyInfo>;
}

/// Type alias for arc-wrapped dynamic KMS provider.
///
/// Used for thread-safe, reference-counted storage of KMS providers.
pub type ArcKmsProvider = std::sync::Arc<dyn BaseKmsProvider>;

/// Key information returned by provider.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    /// Human-readable alias for the key, if one is configured in the provider.
    pub alias: Option<String>,
    /// Unix timestamp (seconds) when the key was created.
    pub created_at: i64,
}

/// Rotation policy info returned by provider.
#[derive(Debug, Clone)]
pub struct RotationPolicyInfo {
    /// Whether automatic rotation is enabled for this key.
    pub enabled: bool,
    /// How often the key is rotated, expressed in days.
    pub rotation_period_days: u32,
    /// Unix timestamp (seconds) of the most recent rotation, if any.
    pub last_rotation: Option<i64>,
    /// Unix timestamp (seconds) when the next rotation is scheduled, if known.
    pub next_rotation: Option<i64>,
}

#[cfg(test)]
mod tests {
    use crate::utils::clock::{Clock as _, SystemClock};

    #[test]
    fn test_system_clock_timestamp_is_positive() {
        assert!(SystemClock.now_secs_i64() > 0);
    }
}
