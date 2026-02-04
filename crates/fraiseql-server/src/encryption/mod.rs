// Phase 12.3: Field-Level Encryption
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
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use rand::Rng;
use crate::secrets_manager::SecretsError;

mod field_encryption_tests;

const NONCE_SIZE: usize = 12;  // 96 bits for GCM
#[allow(dead_code)]
const TAG_SIZE: usize = 16;    // 128 bits authentication tag (used in Phase 12.3+ cycles)
const KEY_SIZE: usize = 32;    // 256 bits for AES-256

/// Cipher for field-level encryption using AES-256-GCM
///
/// Encrypts sensitive database fields with authenticated encryption.
/// Each encryption uses a random nonce, preventing identical plaintexts
/// from producing identical ciphertexts.
///
/// # Example
/// ```ignore
/// let cipher = FieldEncryption::new("encryption-key".as_bytes());
/// let encrypted = cipher.encrypt("user@example.com")?;
/// let decrypted = cipher.decrypt(&encrypted)?;
/// assert_eq!(decrypted, "user@example.com");
/// ```
#[derive(Clone)]
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
    /// Create new field encryption cipher
    ///
    /// # Arguments
    /// * `key` - Encryption key bytes (must be exactly 32 bytes for AES-256)
    ///
    /// # Returns
    /// FieldEncryption cipher ready for encrypt/decrypt operations
    ///
    /// # Panics
    /// If key length is not exactly 32 bytes
    pub fn new(key: &[u8]) -> Self {
        if key.len() != KEY_SIZE {
            panic!(
                "Encryption key must be exactly {} bytes, got {}",
                KEY_SIZE,
                key.len()
            );
        }

        let cipher = Aes256Gcm::new_from_slice(key)
            .expect("Key size already validated");

        FieldEncryption { cipher }
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
        let mut rng = rand::thread_rng();
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rng.fill(&mut nonce_bytes);

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
        if encrypted.len() < NONCE_SIZE {
            return Err(SecretsError::EncryptionError(
                "Encrypted data too short for nonce".to_string(),
            ));
        }

        let nonce_bytes = &encrypted[0..NONCE_SIZE];
        let ciphertext = &encrypted[NONCE_SIZE..];

        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext_bytes = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| SecretsError::EncryptionError(format!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext_bytes)
            .map_err(|e| SecretsError::EncryptionError(format!("Invalid UTF-8 in decrypted data: {}", e)))
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
    pub fn encrypt_with_context(&self, plaintext: &str, context: &str) -> Result<Vec<u8>, SecretsError> {
        let mut rng = rand::thread_rng();
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rng.fill(&mut nonce_bytes);

        let nonce = Nonce::from_slice(&nonce_bytes);
        let payload = Payload {
            msg: plaintext.as_bytes(),
            aad: context.as_bytes(),
        };

        let ciphertext = self
            .cipher
            .encrypt(nonce, payload)
            .map_err(|e| SecretsError::EncryptionError(format!("Encryption with context failed: {}", e)))?;

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
    pub fn decrypt_with_context(&self, encrypted: &[u8], context: &str) -> Result<String, SecretsError> {
        if encrypted.len() < NONCE_SIZE {
            return Err(SecretsError::EncryptionError(
                "Encrypted data too short for nonce".to_string(),
            ));
        }

        let nonce_bytes = &encrypted[0..NONCE_SIZE];
        let ciphertext = &encrypted[NONCE_SIZE..];

        let nonce = Nonce::from_slice(nonce_bytes);
        let payload = Payload {
            msg: ciphertext,
            aad: context.as_bytes(),
        };

        let plaintext_bytes = self
            .cipher
            .decrypt(nonce, payload)
            .map_err(|e| SecretsError::EncryptionError(format!("Decryption with context failed: {}", e)))?;

        String::from_utf8(plaintext_bytes)
            .map_err(|e| SecretsError::EncryptionError(format!("Invalid UTF-8 in decrypted data: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test FieldEncryption creation
    #[test]
    fn test_field_encryption_creation() {
        let key = [0u8; KEY_SIZE];
        let _cipher = FieldEncryption::new(&key);
        assert!(true);  // Just verify creation succeeds
    }

    /// Test basic encryption/decryption roundtrip
    #[test]
    fn test_field_encrypt_decrypt_roundtrip() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key);

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
        let cipher = FieldEncryption::new(&key);

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
        let cipher = FieldEncryption::new(&key);

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
        let cipher = FieldEncryption::new(&key);

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
        let cipher = FieldEncryption::new(&key);

        let test_cases = vec![
            "email@example.com",
            "+1-555-123-4567",
            "123-45-6789",
            "4532015112830366",
            "sk_live_abc123def456",
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "",  // Empty string
            "with\nspecial\nchars\t!@#$%",
            "unicode: 你好世界 🔐",
        ];

        for plaintext in test_cases {
            let encrypted = cipher.encrypt(plaintext).unwrap();
            let decrypted = cipher.decrypt(&encrypted).unwrap();
            assert_eq!(plaintext, decrypted);
        }
    }

    /// Test invalid key size panics
    #[test]
    #[should_panic(expected = "must be exactly 32 bytes")]
    fn test_field_encryption_invalid_key_size() {
        let invalid_key = [0u8; 16];  // Too short
        let _cipher = FieldEncryption::new(&invalid_key);
    }

    /// Test corrupted ciphertext fails to decrypt
    #[test]
    fn test_field_decrypt_corrupted_data_fails() {
        let key = [0u8; KEY_SIZE];
        let cipher = FieldEncryption::new(&key);

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
        let cipher = FieldEncryption::new(&key);

        let short_data = vec![0u8; 5];  // Too short for nonce
        let result = cipher.decrypt(&short_data);
        assert!(result.is_err());
    }
}
