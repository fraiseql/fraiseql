//! Encryption for sensitive database fields using AES-256-GCM
//!
//! Provides transparent encryption/decryption for:
//! - User emails
//! - Phone numbers
//! - SSN/tax IDs
//! - Credit card data
//! - API keys
//! - OAuth tokens

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngCore;

use crate::secrets_manager::SecretsError;

pub mod audit_logging;
pub mod compliance;
pub mod credential_rotation;
pub mod dashboard;
pub mod database_adapter;
pub mod error_recovery;
pub mod mapper;
pub mod middleware;
pub mod performance;
pub mod query_builder;
pub mod refresh_trigger;
pub mod rotation_api;
pub mod schema;
pub mod transaction;

pub use credential_rotation::KeyVersion;

const NONCE_SIZE: usize = 12; // 96 bits for GCM
const KEY_SIZE: usize = 32; // 256 bits for AES-256

/// Cipher for field-level encryption using AES-256-GCM
///
/// Encrypts sensitive database fields with authenticated encryption.
/// Each encryption uses a random nonce, preventing identical plaintexts
/// from producing identical ciphertexts.
///
/// `FieldEncryption` does not implement `Clone`. Shared access should use
/// `Arc<FieldEncryption>` so the key schedule is held in exactly one heap
/// allocation and zeroed on drop (requires `aes-gcm` `zeroize` feature,
/// which is enabled in this crate's `Cargo.toml`).
///
/// # Example
/// ```rust
/// use fraiseql_secrets::FieldEncryption;
/// // Key must be exactly 32 bytes for AES-256-GCM.
/// let key = b"12345678901234567890123456789012"; // 32 bytes
/// let cipher = FieldEncryption::new(key).unwrap();
/// let encrypted = cipher.encrypt("user@example.com").unwrap();
/// let decrypted = cipher.decrypt(&encrypted).unwrap();
/// assert_eq!(decrypted, "user@example.com");
/// ```
pub struct FieldEncryption {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for FieldEncryption {
    #[cfg_attr(test, mutants::skip)]
    // Reason: security-diagnostic Debug impl — outputs "Aes256Gcm(redacted)" to avoid
    // leaking key material; no test asserts on this string so mutations cannot be killed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldEncryption")
            .field("cipher", &"Aes256Gcm(redacted)")
            .finish()
    }
}

impl FieldEncryption {
    /// Create new field encryption cipher.
    ///
    /// # Arguments
    /// * `key` - Encryption key bytes (must be exactly 32 bytes for AES-256)
    ///
    /// # Errors
    /// Returns `SecretsError::ValidationError` if `key` is not exactly 32 bytes.
    pub fn new(key: &[u8]) -> Result<Self, SecretsError> {
        if key.len() != KEY_SIZE {
            return Err(SecretsError::ValidationError(format!(
                "Encryption key must be exactly {} bytes, got {}",
                KEY_SIZE,
                key.len()
            )));
        }

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| SecretsError::EncryptionError(format!("Failed to create cipher: {e}")))?;

        Ok(FieldEncryption { cipher })
    }

    /// Generate random nonce for encryption
    ///
    /// Uses cryptographically secure random number generation to ensure
    /// each encryption produces a unique nonce, preventing pattern analysis.
    fn generate_nonce() -> [u8; NONCE_SIZE] {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        nonce_bytes
    }

    /// Validate and extract nonce from encrypted data
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted data with nonce prefix
    ///
    /// # Returns
    /// Tuple of (nonce, ciphertext) or error if too short
    fn extract_nonce_and_ciphertext(
        encrypted: &[u8],
    ) -> Result<([u8; NONCE_SIZE], &[u8]), SecretsError> {
        if encrypted.len() < NONCE_SIZE {
            return Err(SecretsError::EncryptionError(format!(
                "Encrypted data too short (need ≥{} bytes, got {})",
                NONCE_SIZE,
                encrypted.len()
            )));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&encrypted[0..NONCE_SIZE]);
        let ciphertext = &encrypted[NONCE_SIZE..];

        Ok((nonce, ciphertext))
    }

    /// Convert bytes to UTF-8 string with context
    ///
    /// Provides clear error messages on encoding failures for debugging
    fn bytes_to_utf8(bytes: Vec<u8>, context: &str) -> Result<String, SecretsError> {
        String::from_utf8(bytes).map_err(|e| {
            SecretsError::EncryptionError(format!("Invalid UTF-8 in {}: {}", context, e))
        })
    }

    /// Encrypt plaintext field using AES-256-GCM
    ///
    /// Generates random 96-bit nonce, encrypts with authenticated encryption,
    /// and returns [nonce || ciphertext] format for decryption.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    ///
    /// # Returns
    /// Encrypted data in format: [12-byte nonce][ciphertext + 16-byte tag]
    ///
    /// # Errors
    /// Returns `EncryptionError` if encryption fails
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, SecretsError> {
        let nonce_bytes = Self::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| SecretsError::EncryptionError(format!("Encryption failed: {}", e)))?;

        // Return [nonce || ciphertext]
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt encrypted field using AES-256-GCM
    ///
    /// Expects data in format: [12-byte nonce][ciphertext + 16-byte tag]
    /// Extracts nonce, decrypts, and verifies authentication tag.
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted data from `encrypt()`
    ///
    /// # Returns
    /// Decrypted plaintext as String
    ///
    /// # Errors
    /// Returns `EncryptionError` if:
    /// - Data too short for nonce
    /// - Decryption fails (wrong key or corrupted data)
    /// - Plaintext is not valid UTF-8
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<String, SecretsError> {
        let (nonce_bytes, ciphertext) = Self::extract_nonce_and_ciphertext(encrypted)?;

        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext_bytes = self.cipher.decrypt(nonce, ciphertext).map_err(|_| {
            SecretsError::EncryptionError(
                "Decryption failed: authentication tag mismatch. \
                 Possible causes: wrong key, corrupted data, or data was encrypted \
                 with context (use decrypt_with_context instead)."
                    .to_string(),
            )
        })?;

        Self::bytes_to_utf8(plaintext_bytes, "decrypted data")
    }

    /// Encrypt field with additional context for audit/security
    ///
    /// Includes context (e.g., `user_id`, `field_name`) in authenticated data
    /// but not in ciphertext, providing audit trail without bloating storage.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `context` - Additional authenticated data (e.g., "user:123:email")
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::EncryptionError` if AES-GCM encryption fails.
    pub fn encrypt_with_context(
        &self,
        plaintext: &str,
        context: &str,
    ) -> Result<Vec<u8>, SecretsError> {
        let nonce_bytes = Self::generate_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let payload = Payload {
            msg: plaintext.as_bytes(),
            aad: context.as_bytes(),
        };

        let ciphertext = self.cipher.encrypt(nonce, payload).map_err(|e| {
            SecretsError::EncryptionError(format!("Encryption with context failed: {}", e))
        })?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt field with additional context verification
    ///
    /// Context must match the value used during encryption for verification to succeed.
    ///
    /// # Arguments
    /// * `encrypted` - Encrypted data from `encrypt_with_context()`
    /// * `context` - Context that was used during encryption
    ///
    /// # Returns
    /// Decrypted plaintext as String
    ///
    /// # Errors
    /// Returns `EncryptionError` if context doesn't match or decryption fails
    pub fn decrypt_with_context(
        &self,
        encrypted: &[u8],
        context: &str,
    ) -> Result<String, SecretsError> {
        let (nonce_bytes, ciphertext) = Self::extract_nonce_and_ciphertext(encrypted)?;

        let nonce = Nonce::from_slice(&nonce_bytes);
        let payload = Payload {
            msg: ciphertext,
            aad: context.as_bytes(),
        };

        let plaintext_bytes = self.cipher.decrypt(nonce, payload).map_err(|_| {
            SecretsError::EncryptionError(
                "Decryption with context failed: authentication tag mismatch. \
                 Ensure the context string supplied here exactly matches the one \
                 used during encryption. This can also indicate key mismatch \
                 or data corruption."
                    .to_string(),
            )
        })?;

        Self::bytes_to_utf8(plaintext_bytes, "decrypted data with context")
    }
}

/// Versioned ciphertext layout: `[version: 2 bytes LE][nonce: 12 bytes][ciphertext + 16-byte tag]`.
const VERSION_PREFIX_SIZE: usize = 2;

/// Multi-key cipher that supports decrypting data from rotated-out keys.
///
/// Ciphertexts are prefixed with a 2-byte key version number (little-endian)
/// so the correct key can be selected during decryption. New data is always
/// encrypted with the primary key; old ciphertexts encrypted with a
/// secondary/fallback key remain readable until data is migrated.
///
/// # Key rotation workflow
///
/// 1. Promote the new key: `VersionedFieldEncryption::new(new_version, new_key_bytes)`
/// 2. Register the old key as a fallback: `.with_fallback(old_version, old_key_bytes)`
/// 3. All new records are encrypted with the primary (new) key.
/// 4. Existing records encrypted with the old key are decrypted successfully via the fallback.
/// 5. Migrate old records by reading → decrypting → re-encrypting (see `reencrypt_from_fallback`).
/// 6. Once migration is complete, remove the fallback.
pub struct VersionedFieldEncryption {
    primary_version: KeyVersion,
    primary:         FieldEncryption,
    fallbacks:       Vec<(KeyVersion, FieldEncryption)>,
}

impl VersionedFieldEncryption {
    /// Create with a single primary key.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::ValidationError` if `key` is not 32 bytes.
    pub fn new(primary_version: KeyVersion, primary_key: &[u8]) -> Result<Self, SecretsError> {
        Ok(Self {
            primary_version,
            primary: FieldEncryption::new(primary_key)?,
            fallbacks: Vec::new(),
        })
    }

    /// Register an additional key that can be used for decryption only.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::ValidationError` if `key` is not 32 bytes.
    pub fn with_fallback(mut self, version: KeyVersion, key: &[u8]) -> Result<Self, SecretsError> {
        self.fallbacks.push((version, FieldEncryption::new(key)?));
        Ok(self)
    }

    /// Encrypt plaintext, embedding the primary key version as a 2-byte LE prefix.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::EncryptionError` on failure.
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, SecretsError> {
        let inner = self.primary.encrypt(plaintext)?;
        let mut out = Vec::with_capacity(VERSION_PREFIX_SIZE + inner.len());
        out.extend_from_slice(&self.primary_version.to_le_bytes());
        out.extend_from_slice(&inner);
        Ok(out)
    }

    /// Extract the key version from an encrypted blob.
    ///
    /// # Errors
    ///
    /// Returns error if `encrypted` is too short to contain the version prefix.
    pub fn extract_version(encrypted: &[u8]) -> Result<KeyVersion, SecretsError> {
        if encrypted.len() < VERSION_PREFIX_SIZE {
            return Err(SecretsError::EncryptionError(format!(
                "Versioned ciphertext too short (need ≥{VERSION_PREFIX_SIZE} bytes, got {})",
                encrypted.len()
            )));
        }
        Ok(u16::from_le_bytes([encrypted[0], encrypted[1]]))
    }

    /// Decrypt an encrypted blob by selecting the key matching the embedded version.
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::EncryptionError` if:
    /// - The blob is too short to contain the version prefix.
    /// - The version is unknown (not primary and not a registered fallback).
    /// - Decryption fails (wrong key, corrupted data).
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<String, SecretsError> {
        let version = Self::extract_version(encrypted)?;
        let inner = &encrypted[VERSION_PREFIX_SIZE..];

        if version == self.primary_version {
            return self.primary.decrypt(inner);
        }

        for (fb_version, fb_cipher) in &self.fallbacks {
            if *fb_version == version {
                return fb_cipher.decrypt(inner);
            }
        }

        Err(SecretsError::EncryptionError(format!(
            "Unknown key version {version}; known versions: primary={}, fallbacks=[{}]",
            self.primary_version,
            self.fallbacks.iter().map(|(v, _)| v.to_string()).collect::<Vec<_>>().join(", ")
        )))
    }

    /// Re-encrypt a ciphertext from a fallback key to the current primary key.
    ///
    /// Use this during key rotation to migrate old records without exposing the
    /// plaintext outside this function.
    ///
    /// # Errors
    ///
    /// Returns error if decryption or re-encryption fails.
    pub fn reencrypt_from_fallback(&self, old_ciphertext: &[u8]) -> Result<Vec<u8>, SecretsError> {
        let plaintext = self.decrypt(old_ciphertext)?;
        self.encrypt(&plaintext)
    }
}

#[cfg(test)]
mod tests;
