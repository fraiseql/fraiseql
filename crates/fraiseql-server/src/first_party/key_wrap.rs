//! Sealing a signing key's private half with the key-encryption key.

use fraiseql_auth::{AuthError, KeyWrap};
use fraiseql_secrets::FieldEncryption;

/// [`KeyWrap`] over AES-256-GCM ([`FieldEncryption`]), under the 32-byte
/// key-encryption key.
///
/// Only the encryption primitive is used: this is not schema-field encryption.
pub struct SealedWithFieldEncryption {
    cipher: FieldEncryption,
}

impl std::fmt::Debug for SealedWithFieldEncryption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedWithFieldEncryption").finish_non_exhaustive()
    }
}

impl SealedWithFieldEncryption {
    /// Seal under `key_encryption_key`.
    ///
    /// # Errors
    ///
    /// [`AuthError::ConfigError`] if the cipher refuses the key.
    pub fn new(key_encryption_key: &[u8; 32]) -> Result<Self, AuthError> {
        let cipher =
            FieldEncryption::new(key_encryption_key).map_err(|e| AuthError::ConfigError {
                message: format!("the key-encryption key was refused: {e}"),
            })?;
        Ok(Self { cipher })
    }
}

impl KeyWrap for SealedWithFieldEncryption {
    fn seal(&self, plaintext: &str, aad: &str) -> fraiseql_auth::Result<Vec<u8>> {
        self.cipher
            .encrypt_with_context(plaintext, aad)
            .map_err(|e| AuthError::Internal {
                message: format!("failed to seal a signing key: {e}"),
            })
    }

    fn open(&self, ciphertext: &[u8], aad: &str) -> fraiseql_auth::Result<String> {
        self.cipher
            .decrypt_with_context(ciphertext, aad)
            .map_err(|e| AuthError::Internal {
                message: format!("failed to open a signing key: {e}"),
            })
    }
}
