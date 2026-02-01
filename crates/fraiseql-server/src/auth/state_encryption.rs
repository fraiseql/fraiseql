// State encryption for PKCE protection
// Encrypts OAuth state parameters using ChaCha20-Poly1305 AEAD
// Phase 7, Cycle 4: GREEN phase - Implementation

use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngCore;

use crate::auth::{AuthError, error::Result};

/// Encrypted state container with nonce
#[derive(Debug, Clone)]
pub struct EncryptedState {
    /// Ciphertext with authentication tag appended
    pub ciphertext: Vec<u8>,
    /// 96-bit nonce used for encryption
    pub nonce:      [u8; 12],
}

impl EncryptedState {
    /// Create new encrypted state
    pub fn new(ciphertext: Vec<u8>, nonce: [u8; 12]) -> Self {
        Self { ciphertext, nonce }
    }

    /// Serialize to bytes for storage
    /// Format: [12-byte nonce][ciphertext with auth tag]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12 + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 {
            return Err(AuthError::InvalidState);
        }

        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&bytes[0..12]);
        let ciphertext = bytes[12..].to_vec();

        Ok(Self::new(ciphertext, nonce))
    }
}

/// State encryption using ChaCha20-Poly1305 AEAD
///
/// Provides authenticated encryption for OAuth state parameters.
/// Uses a fixed encryption key for the deployment lifetime.
/// Each encryption uses a random nonce for security.
///
/// # Security Properties
/// - **Confidentiality**: State values are encrypted with ChaCha20
/// - **Authenticity**: Authentication tag prevents tampering detection
/// - **Replay Prevention**: Random nonce in each encryption
/// - **Key Isolation**: Separate from signing keys, used only for state
pub struct StateEncryption {
    cipher: ChaCha20Poly1305,
}

impl StateEncryption {
    /// Create a new state encryption instance
    ///
    /// # Arguments
    /// * `key` - 32-byte encryption key (must be cryptographically random)
    ///
    /// # Errors
    /// Returns error if key is invalid
    pub fn new(key_bytes: &[u8; 32]) -> Result<Self> {
        let cipher =
            ChaCha20Poly1305::new_from_slice(key_bytes).map_err(|_| AuthError::ConfigError {
                message: "Invalid state encryption key".to_string(),
            })?;

        Ok(Self { cipher })
    }

    /// Encrypt a state value
    ///
    /// Generates a random 96-bit nonce and encrypts the state using ChaCha20-Poly1305.
    /// The authentication tag is appended to the ciphertext.
    ///
    /// # Arguments
    /// * `state` - The plaintext state value to encrypt
    ///
    /// # Returns
    /// EncryptedState containing ciphertext and nonce
    ///
    /// # Errors
    /// Returns error if encryption fails (should be rare)
    pub fn encrypt(&self, state: &str) -> Result<EncryptedState> {
        // Generate random 96-bit nonce
        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        // Encrypt with AEAD (includes authentication tag)
        let ciphertext =
            self.cipher.encrypt(&nonce, Payload::from(state.as_bytes())).map_err(|_| {
                AuthError::Internal {
                    message: "State encryption failed".to_string(),
                }
            })?;

        Ok(EncryptedState::new(ciphertext, nonce_bytes))
    }

    /// Decrypt and verify a state value
    ///
    /// Uses the nonce from EncryptedState to decrypt the ciphertext.
    /// Authentication tag verification is automatic - tampering is detected.
    ///
    /// # Arguments
    /// * `encrypted` - The encrypted state to decrypt
    ///
    /// # Returns
    /// The decrypted plaintext state value
    ///
    /// # Errors
    /// Returns error if:
    /// - Authentication tag verification fails (tampering detected)
    /// - Decryption fails
    /// - Result is not valid UTF-8
    pub fn decrypt(&self, encrypted: &EncryptedState) -> Result<String> {
        let nonce = Nonce::from(encrypted.nonce);

        // Decrypt and verify authentication tag
        let plaintext = self
            .cipher
            .decrypt(&nonce, Payload::from(encrypted.ciphertext.as_slice()))
            .map_err(|_| AuthError::InvalidState)?;

        // Convert bytes to UTF-8 string
        String::from_utf8(plaintext).map_err(|_| AuthError::InvalidState)
    }

    /// Encrypt state and serialize to bytes
    pub fn encrypt_to_bytes(&self, state: &str) -> Result<Vec<u8>> {
        let encrypted = self.encrypt(state)?;
        Ok(encrypted.to_bytes())
    }

    /// Decrypt state from serialized bytes
    pub fn decrypt_from_bytes(&self, bytes: &[u8]) -> Result<String> {
        let encrypted = EncryptedState::from_bytes(bytes)?;
        self.decrypt(&encrypted)
    }
}

/// Generate a cryptographically random encryption key
pub fn generate_state_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        // Use deterministic test key
        [42u8; 32]
    }

    #[test]
    fn test_encrypt_decrypt() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "oauth_state_test_value";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_encrypt_produces_ciphertext() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "test_state";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Ciphertext should be different from plaintext (due to ChaCha20 encryption)
        // Ciphertext should include auth tag, so typically longer than plaintext
        assert_ne!(encrypted.ciphertext, state.as_bytes());
    }

    #[test]
    fn test_empty_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_different_keys_fail_decryption() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let state = "secret_state";

        let encryption1 = StateEncryption::new(&key1).expect("Init 1 failed");
        let encrypted = encryption1.encrypt(state).expect("Encryption failed");

        let encryption2 = StateEncryption::new(&key2).expect("Init 2 failed");
        let result = encryption2.decrypt(&encrypted);

        // Different key should fail due to authentication tag mismatch
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "tamper_test";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Tamper with ciphertext
        if !encrypted.ciphertext.is_empty() {
            encrypted.ciphertext[0] ^= 0xFF;
        }

        // Should fail due to authentication tag verification
        let result = encryption.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_nonce_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "nonce_tamper";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Tamper with nonce
        encrypted.nonce[0] ^= 0xFF;

        // Should fail due to authentication tag verification
        let result = encryption.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_ciphertext_fails() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "truncation_test";

        let mut encrypted = encryption.encrypt(state).expect("Encryption failed");

        // Truncate (removes auth tag)
        if encrypted.ciphertext.len() > 1 {
            encrypted.ciphertext.truncate(encrypted.ciphertext.len() - 1);
        }

        // Should fail
        let result = encryption.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "serialization_test";

        // Encrypt and serialize
        let bytes = encryption.encrypt_to_bytes(state).expect("Encryption failed");

        // Deserialize and decrypt
        let decrypted = encryption.decrypt_from_bytes(&bytes).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_random_nonces() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "random_nonce_test";

        let encrypted1 = encryption.encrypt(state).expect("Encryption 1 failed");
        let encrypted2 = encryption.encrypt(state).expect("Encryption 2 failed");

        // Nonces should be different (extremely unlikely to collide)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);

        // Both should decrypt correctly
        let decrypted1 = encryption.decrypt(&encrypted1).expect("Decryption 1 failed");
        let decrypted2 = encryption.decrypt(&encrypted2).expect("Decryption 2 failed");

        assert_eq!(decrypted1, state);
        assert_eq!(decrypted2, state);
    }

    #[test]
    fn test_long_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "a".repeat(10_000);

        let encrypted = encryption.encrypt(&state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_special_characters() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state:with-special_chars.and/symbols!@#$%^&*()";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_unicode_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state_with_emoji_🔐_🔒_🔓_and_emoji";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_null_bytes_in_state() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "state_with\x00null\x00bytes\x00";

        let encrypted = encryption.encrypt(state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }

    #[test]
    fn test_key_generation() {
        let key1 = generate_state_encryption_key();
        let key2 = generate_state_encryption_key();

        // Keys should be different
        assert_ne!(key1, key2);

        // Both should be valid 32-byte keys
        assert_eq!(key1.len(), 32);
        assert_eq!(key2.len(), 32);

        // Both should work
        let enc1 = StateEncryption::new(&key1).expect("Init 1 failed");
        let enc2 = StateEncryption::new(&key2).expect("Init 2 failed");

        let state = "test";
        let encrypted1 = enc1.encrypt(state).expect("Encryption 1 failed");
        let encrypted2 = enc2.encrypt(state).expect("Encryption 2 failed");

        assert_eq!(enc1.decrypt(&encrypted1).expect("Decryption 1 failed"), state);
        assert_eq!(enc2.decrypt(&encrypted2).expect("Decryption 2 failed"), state);
    }

    #[test]
    fn test_large_ciphertext() {
        let encryption = StateEncryption::new(&test_key()).expect("Init failed");
        let state = "x".repeat(100_000);

        let encrypted = encryption.encrypt(&state).expect("Encryption failed");
        let decrypted = encryption.decrypt(&encrypted).expect("Decryption failed");

        assert_eq!(decrypted, state);
    }
}
