//! Encrypted secrets store for serverless functions.
//!
//! Provides a trait-based secrets API where values are encrypted at rest using
//! AES-256-GCM (feature `function-secrets`). An in-memory implementation ships
//! for unit tests; production use wires a PostgreSQL-backed store.
//!
//! # Design
//!
//! Each secret is identified by `(function_name, key)` and stored as an
//! AES-256-GCM ciphertext with a random 96-bit nonce prepended.  The nonce is
//! generated fresh per write so identical plaintext produces distinct ciphertexts.
//!
//! The store is intentionally narrow: **it never returns plaintext in bulk**.
//! `list_secret_keys` returns key names only; `get_secret` is the sole
//! decryption path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Storage and retrieval of encrypted per-function secrets.
///
/// Values are encrypted by the store before writing and decrypted on read,
/// so implementations never persist plaintext.
#[async_trait]
pub trait FunctionSecretsStore: Send + Sync {
    /// Encrypt and store `value` under `(function_name, key)`.
    ///
    /// Overwrites any existing value for the same pair.
    ///
    /// # Errors
    ///
    /// Returns `Err` if encryption or the underlying write fails.
    async fn set_secret(&self, function_name: &str, key: &str, value: &str) -> Result<()>;

    /// Decrypt and return the value for `(function_name, key)`.
    ///
    /// Returns `Ok(None)` if the key does not exist.
    ///
    /// # Errors
    ///
    /// Returns `Err` if decryption or the underlying read fails.
    async fn get_secret(&self, function_name: &str, key: &str) -> Result<Option<String>>;

    /// Delete the secret at `(function_name, key)`.
    ///
    /// Returns `true` if a secret was found and deleted, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the underlying write fails.
    async fn delete_secret(&self, function_name: &str, key: &str) -> Result<bool>;

    /// List all secret key names for `function_name`.
    ///
    /// Values are never included — only key names.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the underlying read fails.
    async fn list_secret_keys(&self, function_name: &str) -> Result<Vec<String>>;
}

// ── Encryption helpers (feature-gated) ───────────────────────────────────────

#[cfg(feature = "function-secrets")]
mod crypto {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };
    use fraiseql_error::{FraiseQLError, Result};
    use rand::RngCore;

    /// Size of the AES-256-GCM nonce in bytes.
    const NONCE_BYTES: usize = 12;

    /// Encrypt `plaintext` with the given 32-byte key, prepending the nonce.
    ///
    /// Output layout: `[12-byte nonce || ciphertext]`, then base64-encoded.
    ///
    /// # Errors
    ///
    /// Returns `Err` if encryption fails.
    pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| FraiseQLError::Validation {
            message: format!("failed to create cipher: {e}"),
            path: None,
        })?;

        let mut nonce_bytes = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|e| FraiseQLError::Validation {
            message: format!("encryption failed: {e}"),
            path: None,
        })?;

        // Prepend nonce to ciphertext, then base64-encode the result
        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(base64_encode(&combined))
    }

    /// Decrypt `encoded` (base64 `[nonce || ciphertext]`) with the given key.
    ///
    /// # Errors
    ///
    /// Returns `Err` if base64 decoding, nonce extraction, or decryption fails.
    pub fn decrypt(key: &[u8; 32], encoded: &str) -> Result<Vec<u8>> {
        let combined = base64_decode(encoded)?;

        if combined.len() < NONCE_BYTES {
            return Err(FraiseQLError::Validation {
                message: "ciphertext too short (missing nonce)".to_string(),
                path: None,
            });
        }

        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_BYTES);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| FraiseQLError::Validation {
            message: format!("failed to create cipher: {e}"),
            path: None,
        })?;

        cipher.decrypt(nonce, ciphertext).map_err(|e| FraiseQLError::Validation {
            message: format!("decryption failed: {e}"),
            path: None,
        })
    }

    fn base64_encode(data: &[u8]) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    fn base64_decode(encoded: &str) -> Result<Vec<u8>> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|e| FraiseQLError::Validation {
                message: format!("base64 decode failed: {e}"),
                path: None,
            })
    }
}

// ── In-memory store ──────────────────────────────────────────────────────────

/// In-memory secrets store backed by AES-256-GCM encryption.
///
/// Suitable for unit tests. Secrets are encrypted at rest even in memory;
/// the encryption key is generated once per store instance.
#[derive(Clone)]
pub struct InMemorySecretsStore {
    /// `(function_name, key)` → encrypted ciphertext.
    store: Arc<Mutex<HashMap<(String, String), String>>>,
    #[cfg(feature = "function-secrets")]
    encryption_key: Arc<[u8; 32]>,
}

impl std::fmt::Debug for InMemorySecretsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemorySecretsStore")
            .field("entry_count", &{
                self.store.lock().map(|g| g.len()).unwrap_or(0)
            })
            // encryption_key intentionally omitted — never log cryptographic key material
            .finish_non_exhaustive()
    }
}

impl InMemorySecretsStore {
    /// Create a new store with a randomly generated encryption key.
    ///
    /// # Panics
    ///
    /// Panics if the random key cannot be generated (only possible if the OS
    /// RNG is unavailable — effectively never in practice).
    #[must_use]
    #[cfg(feature = "function-secrets")]
    pub fn new() -> Self {
        use rand::RngCore;

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            encryption_key: Arc::new(key),
        }
    }

    /// Create a plaintext in-memory store (no encryption) for unit tests that
    /// do not have the `function-secrets` feature enabled.
    #[cfg(not(feature = "function-secrets"))]
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySecretsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FunctionSecretsStore for InMemorySecretsStore {
    async fn set_secret(&self, function_name: &str, key: &str, value: &str) -> Result<()> {
        let stored = {
            #[cfg(feature = "function-secrets")]
            {
                crypto::encrypt(&self.encryption_key, value.as_bytes())?
            }
            #[cfg(not(feature = "function-secrets"))]
            {
                value.to_string()
            }
        };

        let mut map = self.store.lock().map_err(|_| FraiseQLError::Validation {
            message: "secrets store mutex poisoned".to_string(),
            path: None,
        })?;
        map.insert((function_name.to_string(), key.to_string()), stored);
        Ok(())
    }

    async fn get_secret(&self, function_name: &str, key: &str) -> Result<Option<String>> {
        let map = self.store.lock().map_err(|_| FraiseQLError::Validation {
            message: "secrets store mutex poisoned".to_string(),
            path: None,
        })?;

        let Some(encoded) = map.get(&(function_name.to_string(), key.to_string())) else {
            return Ok(None);
        };

        let plaintext = {
            #[cfg(feature = "function-secrets")]
            {
                let bytes = crypto::decrypt(&self.encryption_key, encoded)?;
                String::from_utf8(bytes).map_err(|e| FraiseQLError::Validation {
                    message: format!("decrypted secret is not valid UTF-8: {e}"),
                    path: None,
                })?
            }
            #[cfg(not(feature = "function-secrets"))]
            {
                encoded.clone()
            }
        };

        Ok(Some(plaintext))
    }

    async fn delete_secret(&self, function_name: &str, key: &str) -> Result<bool> {
        let mut map = self.store.lock().map_err(|_| FraiseQLError::Validation {
            message: "secrets store mutex poisoned".to_string(),
            path: None,
        })?;
        let removed = map.remove(&(function_name.to_string(), key.to_string())).is_some();
        Ok(removed)
    }

    async fn list_secret_keys(&self, function_name: &str) -> Result<Vec<String>> {
        let map = self.store.lock().map_err(|_| FraiseQLError::Validation {
            message: "secrets store mutex poisoned".to_string(),
            path: None,
        })?;

        let mut keys: Vec<String> = map
            .keys()
            .filter(|(fn_name, _)| fn_name == function_name)
            .map(|(_, k)| k.clone())
            .collect();
        keys.sort_unstable();
        Ok(keys)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    fn store() -> InMemorySecretsStore {
        InMemorySecretsStore::new()
    }

    #[tokio::test]
    async fn test_set_and_get_secret() {
        let s = store();
        s.set_secret("my_fn", "API_KEY", "super_secret").await.unwrap();
        let val = s.get_secret("my_fn", "API_KEY").await.unwrap();
        assert_eq!(val, Some("super_secret".to_string()));
    }

    #[tokio::test]
    async fn test_get_missing_secret_returns_none() {
        let s = store();
        let val = s.get_secret("my_fn", "MISSING").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn test_set_overwrites_existing_secret() {
        let s = store();
        s.set_secret("fn", "KEY", "v1").await.unwrap();
        s.set_secret("fn", "KEY", "v2").await.unwrap();
        let val = s.get_secret("fn", "KEY").await.unwrap();
        assert_eq!(val, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn test_delete_secret_returns_true_when_found() {
        let s = store();
        s.set_secret("fn", "KEY", "value").await.unwrap();
        let deleted = s.delete_secret("fn", "KEY").await.unwrap();
        assert!(deleted);
        let val = s.get_secret("fn", "KEY").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn test_delete_secret_returns_false_when_not_found() {
        let s = store();
        let deleted = s.delete_secret("fn", "GHOST").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_list_secret_keys_returns_names_only() {
        let s = store();
        s.set_secret("fn", "KEY_A", "val_a").await.unwrap();
        s.set_secret("fn", "KEY_B", "val_b").await.unwrap();
        s.set_secret("other_fn", "KEY_X", "val_x").await.unwrap();

        let keys = s.list_secret_keys("fn").await.unwrap();
        assert_eq!(keys, vec!["KEY_A", "KEY_B"]);
    }

    #[tokio::test]
    async fn test_list_secret_keys_empty_when_none_set() {
        let s = store();
        let keys = s.list_secret_keys("fn").await.unwrap();
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_secrets_scoped_per_function() {
        let s = store();
        s.set_secret("fn_a", "KEY", "value_a").await.unwrap();
        s.set_secret("fn_b", "KEY", "value_b").await.unwrap();

        assert_eq!(s.get_secret("fn_a", "KEY").await.unwrap(), Some("value_a".to_string()));
        assert_eq!(s.get_secret("fn_b", "KEY").await.unwrap(), Some("value_b".to_string()));
    }

    #[cfg(feature = "function-secrets")]
    #[tokio::test]
    async fn test_ciphertext_differs_on_each_write() {

        let s = InMemorySecretsStore::new();
        s.set_secret("fn", "KEY", "plaintext").await.unwrap();
        let ct1 = {
            let map = s.store.lock().unwrap();
            map[&("fn".to_string(), "KEY".to_string())].clone()
        };

        // Overwrite with the same value
        s.set_secret("fn", "KEY", "plaintext").await.unwrap();
        let ct2 = {
            let map = s.store.lock().unwrap();
            map[&("fn".to_string(), "KEY".to_string())].clone()
        };

        // Different nonce → different ciphertext even for identical plaintext
        assert_ne!(ct1, ct2, "ciphertext should differ due to random nonce");
    }
}
