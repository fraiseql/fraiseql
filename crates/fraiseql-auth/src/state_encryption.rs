//! State encryption for PKCE and OAuth state parameter protection.
//!
//! Encrypts OAuth `state` (and PKCE) blobs with AEAD ciphers so that the
//! outbound token sent to the identity provider cannot be deciphered or
//! tampered with by an attacker who intercepts the redirect.
//!
//! Supports two algorithms selectable at runtime:
//! - [`EncryptionAlgorithm::Chacha20Poly1305`] (default, constant-time in software)
//! - [`EncryptionAlgorithm::Aes256Gcm`] (hardware-accelerated on modern CPUs)

use std::{fmt, sync::Arc};

// aes_gcm and chacha20poly1305 both re-export the same underlying `aead` traits.
// We import them once from chacha20poly1305 and reuse for both cipher types.
use aes_gcm::Aes256Gcm;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{AuthError, error::Result};

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
pub fn generate_state_encryption_key() -> Zeroizing<[u8; 32]> {
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    Zeroizing::new(key)
}

// ── StateEncryptionService ────────────────────────────────────────────────────
//
// A higher-level service that wraps the low-level `StateEncryption` struct.
// Differences from `StateEncryption`:
//   - Supports both ChaCha20-Poly1305 AND AES-256-GCM (runtime-selectable)
//   - Wire format: URL-safe base64 of `[12-byte nonce || ciphertext || tag]`
//   - Accepts keys as 64-char hex strings or env-var names
//   - Can be constructed from the compiled schema JSON
//   - Key never appears in `Debug` output
//
// This is the PKCE state encryption service wired into `Server`.

/// Errors that can occur during decryption by `StateEncryptionService`.
#[derive(Debug, thiserror::Error)]
pub enum DecryptionError {
    /// Ciphertext was tampered with or encrypted with a different key.
    #[error("authentication failed — ciphertext may be tampered or key is wrong")]
    AuthenticationFailed,
    /// Input is malformed (empty, too short, bad base64, etc.).
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Errors that can occur when constructing a `StateEncryptionService` key.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// Hex string was not 64 characters (32 bytes).
    #[error("hex key must be 64 chars (32 bytes); got {0} chars")]
    WrongLength(usize),
    /// Hex string contained a non-hex character.
    #[error("invalid hex character in key")]
    InvalidHex,
}

/// AEAD algorithm selection for `StateEncryptionService`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// ChaCha20-Poly1305 (recommended — constant-time, software-friendly).
    #[default]
    #[serde(rename = "chacha20-poly1305")]
    Chacha20Poly1305,
    /// AES-256-GCM (hardware-accelerated on modern CPUs).
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm,
}

impl fmt::Display for EncryptionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chacha20Poly1305 => f.write_str("chacha20-poly1305"),
            Self::Aes256Gcm => f.write_str("aes-256-gcm"),
        }
    }
}

/// Deserialized from `compiled.security.state_encryption`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct StateEncryptionConfig {
    /// Enable the service; when `false`, `from_compiled_schema` returns `None`.
    pub enabled:   bool,
    /// AEAD algorithm to use.
    pub algorithm: EncryptionAlgorithm,
    /// Name of the environment variable holding the 64-char hex key.
    pub key_env:   Option<String>,
}

impl Default for StateEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            algorithm: EncryptionAlgorithm::default(),
            key_env:   Some("STATE_ENCRYPTION_KEY".to_string()),
        }
    }
}

/// AEAD encryption service for OAuth state and PKCE blobs.
///
/// Wire format: URL-safe base64 of `[12-byte nonce || ciphertext || 16-byte tag]`.
///
/// The 32-byte key is never printed in [`fmt::Debug`] output.
pub struct StateEncryptionService {
    algorithm: EncryptionAlgorithm,
    key:       [u8; 32],
}

impl fmt::Debug for StateEncryptionService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateEncryptionService")
            .field("algorithm", &self.algorithm)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl StateEncryptionService {
    /// Construct from a raw 32-byte key slice.
    pub fn from_raw_key(key: &[u8; 32], algorithm: EncryptionAlgorithm) -> Self {
        Self {
            algorithm,
            key: *key,
        }
    }

    /// Construct from a 64-character hex string (= 32 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::WrongLength`] if `hex` is not 64 chars.
    /// Returns [`KeyError::InvalidHex`] if `hex` contains non-hex chars.
    pub fn from_hex_key(
        hex: &str,
        algorithm: EncryptionAlgorithm,
    ) -> std::result::Result<Self, KeyError> {
        if hex.len() != 64 {
            return Err(KeyError::WrongLength(hex.len()));
        }
        let bytes = hex::decode(hex).map_err(|_| KeyError::InvalidHex)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self { algorithm, key })
    }

    /// Load the key from an environment variable containing a 64-char hex string.
    ///
    /// # Errors
    ///
    /// Returns an error if the env var is absent or the value is not valid hex/length.
    pub fn new_from_env(
        var: &str,
        algorithm: EncryptionAlgorithm,
    ) -> std::result::Result<Self, anyhow::Error> {
        let hex = std::env::var(var).map_err(|_| anyhow::anyhow!("env var {var} not set"))?;
        Ok(Self::from_hex_key(&hex, algorithm)?)
    }

    /// Build from the `security` blob of a compiled schema, if enabled.
    ///
    /// Returns `Ok(None)` when the `state_encryption` key is absent or `enabled = false`.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `enabled = true` but the key environment variable is absent
    /// or contains an invalid value.  The server must refuse to start in this case.
    pub fn from_compiled_schema(
        security_json: &serde_json::Value,
    ) -> std::result::Result<Option<Arc<Self>>, anyhow::Error> {
        let cfg: StateEncryptionConfig = match security_json.get("state_encryption") {
            None => return Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|e| anyhow::anyhow!("invalid state_encryption config: {e}"))?,
        };

        if !cfg.enabled {
            return Ok(None);
        }

        let key_env = cfg.key_env.as_deref().unwrap_or("STATE_ENCRYPTION_KEY");
        Self::new_from_env(key_env, cfg.algorithm)
            .map(|svc| Some(Arc::new(svc)))
            .map_err(|e| {
                anyhow::anyhow!(
                    "state_encryption enabled but key env var '{}' failed: {e}",
                    key_env
                )
            })
    }

    /// Encrypt `plaintext` to a URL-safe base64 string.
    ///
    /// A fresh random nonce is generated on every call.
    ///
    /// # Errors
    ///
    /// Returns an error only on internal cipher failure (essentially never).
    pub fn encrypt(&self, plaintext: &[u8]) -> std::result::Result<String, anyhow::Error> {
        let combined = match self.algorithm {
            EncryptionAlgorithm::Chacha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
                    .map_err(|_| anyhow::anyhow!("invalid key for ChaCha20-Poly1305"))?;
                let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
                let ct = cipher
                    .encrypt(&nonce, plaintext)
                    .map_err(|_| anyhow::anyhow!("ChaCha20-Poly1305 encryption failed"))?;
                let mut out = nonce.to_vec();
                out.extend_from_slice(&ct);
                out
            },
            EncryptionAlgorithm::Aes256Gcm => {
                let cipher = Aes256Gcm::new_from_slice(&self.key)
                    .map_err(|_| anyhow::anyhow!("invalid key for AES-256-GCM"))?;
                let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
                let ct = cipher
                    .encrypt(&nonce, plaintext)
                    .map_err(|_| anyhow::anyhow!("AES-256-GCM encryption failed"))?;
                let mut out = nonce.to_vec();
                out.extend_from_slice(&ct);
                out
            },
        };
        Ok(URL_SAFE_NO_PAD.encode(&combined))
    }

    /// Decrypt a URL-safe base64 string produced by [`Self::encrypt`].
    ///
    /// # Errors
    ///
    /// - [`DecryptionError::InvalidInput`] — empty / too-short / bad base64
    /// - [`DecryptionError::AuthenticationFailed`] — tampered or wrong-key
    pub fn decrypt(&self, encoded: &str) -> std::result::Result<Vec<u8>, DecryptionError> {
        if encoded.is_empty() {
            return Err(DecryptionError::InvalidInput("empty input".into()));
        }
        let combined = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| DecryptionError::InvalidInput("invalid base64".into()))?;

        const NONCE_SIZE: usize = 12;
        if combined.len() < NONCE_SIZE {
            return Err(DecryptionError::InvalidInput(format!(
                "too short: {} bytes (minimum {NONCE_SIZE})",
                combined.len()
            )));
        }
        let (nonce_bytes, ct) = combined.split_at(NONCE_SIZE);

        match self.algorithm {
            EncryptionAlgorithm::Chacha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
                    .map_err(|_| DecryptionError::InvalidInput("invalid key".into()))?;
                let nonce = chacha20poly1305::Nonce::from_slice(nonce_bytes);
                cipher.decrypt(nonce, ct).map_err(|_| DecryptionError::AuthenticationFailed)
            },
            EncryptionAlgorithm::Aes256Gcm => {
                let cipher = Aes256Gcm::new_from_slice(&self.key)
                    .map_err(|_| DecryptionError::InvalidInput("invalid key".into()))?;
                let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
                cipher.decrypt(nonce, ct).map_err(|_| DecryptionError::AuthenticationFailed)
            },
        }
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod service_tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    fn chacha_svc() -> StateEncryptionService {
        StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305)
    }
    fn aes_svc() -> StateEncryptionService {
        StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm)
    }

    #[test]
    fn test_chacha_encrypt_decrypt_roundtrip() {
        let svc = chacha_svc();
        let pt = b"oauth_state_nonce_12345";
        assert_eq!(svc.decrypt(&svc.encrypt(pt).unwrap()).unwrap(), pt);
    }

    #[test]
    fn test_chacha_two_encryptions_differ() {
        let svc = chacha_svc();
        assert_ne!(svc.encrypt(b"hello").unwrap(), svc.encrypt(b"hello").unwrap());
    }

    #[test]
    fn test_chacha_tampered_fails() {
        let svc = chacha_svc();
        let ct = svc.encrypt(b"secret").unwrap();
        let mut bytes = URL_SAFE_NO_PAD.decode(&ct).unwrap();
        bytes[15] ^= 0xFF;
        let tampered = URL_SAFE_NO_PAD.encode(&bytes);
        assert!(matches!(svc.decrypt(&tampered), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_chacha_wrong_key_fails() {
        let a =
            StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let b =
            StateEncryptionService::from_raw_key(&[1u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let ct = a.encrypt(b"secret").unwrap();
        assert!(matches!(b.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_aes_encrypt_decrypt_roundtrip() {
        let svc = aes_svc();
        let pt = b"pkce_code_challenge";
        assert_eq!(svc.decrypt(&svc.encrypt(pt).unwrap()).unwrap(), pt);
    }

    #[test]
    fn test_aes_two_encryptions_differ() {
        let svc = aes_svc();
        assert_ne!(svc.encrypt(b"hello").unwrap(), svc.encrypt(b"hello").unwrap());
    }

    #[test]
    fn test_aes_tampered_fails() {
        let svc = aes_svc();
        let ct = svc.encrypt(b"secret").unwrap();
        let mut bytes = URL_SAFE_NO_PAD.decode(&ct).unwrap();
        bytes[15] ^= 0xFF;
        let tampered = URL_SAFE_NO_PAD.encode(&bytes);
        assert!(matches!(svc.decrypt(&tampered), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_aes_wrong_key_fails() {
        let a = StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let b = StateEncryptionService::from_raw_key(&[1u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let ct = a.encrypt(b"secret").unwrap();
        assert!(matches!(b.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_empty_ciphertext_invalid_input() {
        assert!(matches!(chacha_svc().decrypt(""), Err(DecryptionError::InvalidInput(_))));
    }

    #[test]
    fn test_too_short_invalid_input() {
        let short = URL_SAFE_NO_PAD.encode([0u8; 11]);
        assert!(matches!(chacha_svc().decrypt(&short), Err(DecryptionError::InvalidInput(_))));
    }

    #[test]
    fn test_bad_base64_invalid_input() {
        assert!(matches!(
            chacha_svc().decrypt("not!valid@base64#"),
            Err(DecryptionError::InvalidInput(_))
        ));
    }

    #[test]
    fn test_from_hex_key_valid() {
        let hex = "00".repeat(32);
        StateEncryptionService::from_hex_key(&hex, EncryptionAlgorithm::Chacha20Poly1305)
            .unwrap_or_else(|e| panic!("expected Ok for valid 64-char hex key: {e}"));
    }

    #[test]
    fn test_from_hex_key_wrong_length() {
        assert!(matches!(
            StateEncryptionService::from_hex_key("deadbeef", EncryptionAlgorithm::Chacha20Poly1305),
            Err(KeyError::WrongLength(_))
        ));
    }

    #[test]
    fn test_from_hex_key_invalid_hex() {
        let bad = "zz".repeat(32);
        assert!(matches!(
            StateEncryptionService::from_hex_key(&bad, EncryptionAlgorithm::Chacha20Poly1305),
            Err(KeyError::InvalidHex)
        ));
    }

    #[test]
    fn test_debug_redacts_key() {
        let svc = chacha_svc();
        let s = format!("{svc:?}");
        assert!(!s.contains("00000000"), "key bytes must not appear in debug output");
        assert!(s.contains("REDACTED"));
    }

    #[test]
    fn test_from_compiled_schema_enabled_missing_key_returns_error() {
        // Use a unique env var name that is guaranteed absent
        std::env::remove_var("FRAISEQL_TEST_MISSING_ENC_KEY_B1");
        let json = serde_json::json!({
            "state_encryption": {
                "enabled": true,
                "algorithm": "chacha20-poly1305",
                "key_env": "FRAISEQL_TEST_MISSING_ENC_KEY_B1"
            }
        });
        let result = StateEncryptionService::from_compiled_schema(&json);
        assert!(result.is_err(), "should error when enabled=true but env var absent");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("FRAISEQL_TEST_MISSING_ENC_KEY_B1"));
    }

    #[test]
    fn test_from_compiled_schema_enabled() {
        let key_hex = "aa".repeat(32);
        std::env::set_var("TEST_SVC_ENC_KEY_P3", &key_hex);
        let json = serde_json::json!({
            "state_encryption": {
                "enabled": true,
                "algorithm": "chacha20-poly1305",
                "key_env": "TEST_SVC_ENC_KEY_P3"
            }
        });
        let svc = StateEncryptionService::from_compiled_schema(&json)
            .expect("should succeed when env var is set");
        assert!(svc.is_some());
        std::env::remove_var("TEST_SVC_ENC_KEY_P3");
    }

    #[test]
    fn test_from_compiled_schema_disabled() {
        let json = serde_json::json!({"state_encryption": {"enabled": false}});
        assert!(
            StateEncryptionService::from_compiled_schema(&json)
                .expect("disabled should be ok")
                .is_none()
        );
    }

    #[test]
    fn test_from_compiled_schema_missing() {
        assert!(
            StateEncryptionService::from_compiled_schema(&serde_json::json!({}))
                .expect("missing should be ok")
                .is_none()
        );
    }

    #[test]
    fn test_cross_algorithm_fails() {
        let chacha =
            StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Chacha20Poly1305);
        let aes = StateEncryptionService::from_raw_key(&[0u8; 32], EncryptionAlgorithm::Aes256Gcm);
        let ct = chacha.encrypt(b"cross").unwrap();
        assert!(matches!(aes.decrypt(&ct), Err(DecryptionError::AuthenticationFailed)));
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test modules use wildcard imports for conciseness
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
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for wrong-key decryption, got: {result:?}"
        );
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
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for tampered ciphertext, got: {result:?}"
        );
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
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for tampered nonce, got: {result:?}"
        );
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
        assert!(
            matches!(result, Err(AuthError::InvalidState)),
            "expected InvalidState for truncated ciphertext, got: {result:?}"
        );
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
