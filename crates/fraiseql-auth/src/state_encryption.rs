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
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{AuthError, error::Result};

/// Encrypted state container with nonce
#[derive(Debug, Clone)]
pub struct EncryptedState {
    /// Ciphertext with authentication tag appended
    pub ciphertext: Vec<u8>,
    /// 96-bit nonce used for encryption
    pub nonce: [u8; 12],
}

impl EncryptedState {
    /// Create new encrypted state
    #[must_use]
    pub const fn new(ciphertext: Vec<u8>, nonce: [u8; 12]) -> Self {
        Self { ciphertext, nonce }
    }

    /// Serialize to bytes for storage
    /// Format: [12-byte nonce][ciphertext with auth tag]
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12 + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidState`] if `bytes` is shorter than 12 bytes
    /// (minimum nonce size).
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
        rand::rng().fill_bytes(&mut nonce_bytes);
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

    /// Encrypt state and serialize to bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if AEAD encryption fails (essentially never).
    pub fn encrypt_to_bytes(&self, state: &str) -> Result<Vec<u8>> {
        let encrypted = self.encrypt(state)?;
        Ok(encrypted.to_bytes())
    }

    /// Decrypt state from serialized bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidState`] if `bytes` is too short, if AEAD
    /// authentication fails (tampered or wrong key), or if decrypted bytes are
    /// not valid UTF-8.
    pub fn decrypt_from_bytes(&self, bytes: &[u8]) -> Result<String> {
        let encrypted = EncryptedState::from_bytes(bytes)?;
        self.decrypt(&encrypted)
    }
}

/// Generate a cryptographically random encryption key
#[must_use]
pub fn generate_state_encryption_key() -> Zeroizing<[u8; 32]> {
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    pub enabled: bool,
    /// AEAD algorithm to use.
    pub algorithm: EncryptionAlgorithm,
    /// Name of the environment variable holding the 64-char hex key.
    pub key_env: Option<String>,
}

impl Default for StateEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: EncryptionAlgorithm::default(),
            key_env: Some("STATE_ENCRYPTION_KEY".to_string()),
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
    key: [u8; 32],
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
    #[must_use]
    pub const fn from_raw_key(key: &[u8; 32], algorithm: EncryptionAlgorithm) -> Self {
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
            None | Some(serde_json::Value::Null) => return Ok(None),
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
        const NONCE_SIZE: usize = 12;
        if encoded.is_empty() {
            return Err(DecryptionError::InvalidInput("empty input".into()));
        }
        let combined = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| DecryptionError::InvalidInput("invalid base64".into()))?;

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
