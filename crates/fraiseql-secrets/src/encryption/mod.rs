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
    /// Returns EncryptionError if encryption fails
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
    /// * `encrypted` - Encrypted data from encrypt()
    ///
    /// # Returns
    /// Decrypted plaintext as String
    ///
    /// # Errors
    /// Returns EncryptionError if:
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
    /// Includes context (e.g., user_id, field_name) in authenticated data
    /// but not in ciphertext, providing audit trail without bloating storage.
    ///
    /// # Arguments
    /// * `plaintext` - Data to encrypt
    /// * `context` - Additional authenticated data (e.g., "user:123:email")
    ///
    /// # Returns
    /// Encrypted data in format: [12-byte nonce][ciphertext + 16-byte tag]
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
    /// * `encrypted` - Encrypted data from encrypt_with_context()
    /// * `context` - Context that was used during encryption
    ///
    /// # Returns
    /// Decrypted plaintext as String
    ///
    /// # Errors
    /// Returns EncryptionError if context doesn't match or decryption fails
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    /// Test FieldEncryption creation
    #[test]
    fn test_field_encryption_creation() {
        let key = [0u8; KEY_SIZE];
        let _cipher = FieldEncryption::new(&key).unwrap();
    }

    /// Test basic encryption/decryption roundtrip
    #[test]
    fn test_field_encrypt_decrypt_roundtrip() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let plaintext = "user@example.com";
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
        assert_ne!(plaintext.as_bytes(), &encrypted[NONCE_SIZE..]);
    }

    /// Test that same plaintext produces different ciphertexts
    #[test]
    fn test_field_encrypt_random_nonce() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let plaintext = "sensitive@data.com";
        let encrypted1 = cipher.encrypt(plaintext).unwrap();
        let encrypted2 = cipher.encrypt(plaintext).unwrap();

        // Different random nonces produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both decrypt to same plaintext
        assert_eq!(cipher.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(cipher.decrypt(&encrypted2).unwrap(), plaintext);
    }

    /// Test encryption with context
    #[test]
    fn test_field_encrypt_decrypt_with_context() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let plaintext = "secret123";
        let context = "user:456:password";

        let encrypted = cipher.encrypt_with_context(plaintext, context).unwrap();
        let decrypted = cipher.decrypt_with_context(&encrypted, context).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    /// Test context verification fails with wrong context
    #[test]
    fn test_field_decrypt_with_wrong_context_fails() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let plaintext = "secret123";
        let correct_context = "user:456:password";
        let wrong_context = "user:789:password";

        let encrypted = cipher.encrypt_with_context(plaintext, correct_context).unwrap();

        // Decryption with wrong context should fail
        let result = cipher.decrypt_with_context(&encrypted, wrong_context);
        assert!(result.is_err());
    }

    /// Test various data types
    #[test]
    fn test_field_encrypt_various_types() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let test_cases = vec![
            "email@example.com",
            "+1-555-123-4567",
            "123-45-6789",
            "4532015112830366",
            "sk_live_abc123def456",
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "", // Empty string
            "with\nspecial\nchars\t!@#$%",
            "unicode: 你好世界 🔐",
        ];

        for plaintext in test_cases {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(plaintext, decrypted);
        }
    }

    /// Test invalid key size returns Err
    #[test]
    fn test_field_encryption_invalid_key_size_returns_err() {
        let invalid_key = [0u8; 16]; // Too short
        let result = FieldEncryption::new(&invalid_key);
        assert!(result.is_err());
    }

    /// Test corrupted ciphertext fails to decrypt
    #[test]
    fn test_field_decrypt_corrupted_data_fails() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let plaintext = "data";
        let mut encrypted = cipher.encrypt(plaintext).unwrap();

        // Corrupt a byte in the ciphertext (not the nonce)
        if encrypted.len() > NONCE_SIZE {
            encrypted[NONCE_SIZE] ^= 0xFF;
        }

        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err());
    }

    /// Test short ciphertext fails gracefully
    #[test]
    fn test_field_decrypt_short_data_fails() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key).unwrap();

        let short_data = vec![0u8; 5]; // Too short for nonce
        let result = cipher.decrypt(&short_data);
        assert!(result.is_err());
    }

    // =========================================================================
    // Key management / VersionedFieldEncryption tests
    // =========================================================================

    /// Versioned encryption: same inputs produce different ciphertexts due to random nonce
    #[test]
    fn test_versioned_encrypt_not_deterministic() {
        let key = [1u8; KEY_SIZE];
        let ve = VersionedFieldEncryption::new(1, &key).unwrap();

        let ct1 = ve.encrypt("secret").unwrap();
        let ct2 = ve.encrypt("secret").unwrap();
        assert_ne!(ct1, ct2, "Versioned encryption must produce non-deterministic output");
    }

    /// Versioned encryption roundtrip with primary key
    #[test]
    fn test_versioned_encrypt_decrypt_roundtrip() {
        let key = [2u8; KEY_SIZE];
        let ve = VersionedFieldEncryption::new(1, &key).unwrap();

        let plaintext = "sensitive@example.com";
        let ct = ve.encrypt(plaintext).unwrap();
        let decrypted = ve.decrypt(&ct).unwrap();
        assert_eq!(decrypted, plaintext, "Versioned roundtrip must restore original plaintext");
    }

    /// Different key versions produce blobs with different version prefix
    #[test]
    fn test_versioned_different_versions_different_prefixes() {
        let key_v1 = [1u8; KEY_SIZE];
        let key_v2 = [2u8; KEY_SIZE];
        let ve1 = VersionedFieldEncryption::new(1, &key_v1).unwrap();
        let ve2 = VersionedFieldEncryption::new(2, &key_v2).unwrap();

        let ct1 = ve1.encrypt("data").unwrap();
        let ct2 = ve2.encrypt("data").unwrap();

        let ver1 = VersionedFieldEncryption::extract_version(&ct1).unwrap();
        let ver2 = VersionedFieldEncryption::extract_version(&ct2).unwrap();

        assert_ne!(ver1, ver2, "Different key versions must produce different version prefixes");
        assert_eq!(ver1, 1u16);
        assert_eq!(ver2, 2u16);
    }

    /// Fallback key allows decrypting data encrypted with old key version
    #[test]
    fn test_versioned_fallback_key_decrypts_old_data() {
        let key_v1 = [1u8; KEY_SIZE];
        let key_v2 = [2u8; KEY_SIZE];

        // Encrypt with v1
        let ve_old = VersionedFieldEncryption::new(1, &key_v1).unwrap();
        let old_ct = ve_old.encrypt("legacy data").unwrap();

        // Now switch primary to v2, keep v1 as fallback
        let ve_new = VersionedFieldEncryption::new(2, &key_v2)
            .unwrap()
            .with_fallback(1, &key_v1)
            .unwrap();

        // Can decrypt old ciphertext via fallback
        let decrypted = ve_new.decrypt(&old_ct).unwrap();
        assert_eq!(decrypted, "legacy data", "Fallback key must decrypt old ciphertexts");
    }

    /// Empty key material returns an error
    #[test]
    fn test_versioned_empty_key_returns_error() {
        let result = VersionedFieldEncryption::new(1, &[]);
        assert!(result.is_err(), "Empty key must return an error");
    }

    /// Key length too short (16 bytes instead of 32) must fail
    #[test]
    fn test_versioned_short_key_returns_error() {
        let short_key = [0u8; 16];
        let result = VersionedFieldEncryption::new(1, &short_key);
        assert!(result.is_err(), "Short key must return an error");
    }

    /// Derived key is not an identity function (output != input key)
    #[test]
    fn test_versioned_encrypt_is_not_identity() {
        let key = [5u8; KEY_SIZE];
        let ve = VersionedFieldEncryption::new(1, &key).unwrap();
        let ct = ve.encrypt("hello").unwrap();

        // The ciphertext must not equal the plaintext
        assert_ne!(ct, b"hello", "Encrypted output must differ from plaintext");
    }

    /// Reencrypt migrates ciphertext from fallback key to primary key
    #[test]
    fn test_versioned_reencrypt_from_fallback() {
        let key_v1 = [10u8; KEY_SIZE];
        let key_v2 = [20u8; KEY_SIZE];

        let ve_old = VersionedFieldEncryption::new(1, &key_v1).unwrap();
        let old_ct = ve_old.encrypt("migrate me").unwrap();

        let ve_new = VersionedFieldEncryption::new(2, &key_v2)
            .unwrap()
            .with_fallback(1, &key_v1)
            .unwrap();

        let new_ct = ve_new.reencrypt_from_fallback(&old_ct).unwrap();

        // New ciphertext uses version 2
        let ver = VersionedFieldEncryption::extract_version(&new_ct).unwrap();
        assert_eq!(ver, 2u16, "Re-encrypted blob must use the primary key version");

        // Plaintext is preserved
        let decrypted = ve_new.decrypt(&new_ct).unwrap();
        assert_eq!(decrypted, "migrate me", "Plaintext must be preserved after re-encryption");
    }
}
