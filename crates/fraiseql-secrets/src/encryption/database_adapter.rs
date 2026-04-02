//! Database adapter for transparent field-level encryption/decryption
//!
//! Provides query-layer integration for automatic encryption on write
//! and decryption on read operations.
//!
//! # Features
//!
//! - Automatic encryption on INSERT/UPDATE operations
//! - Automatic decryption on SELECT operations
//! - Multi-field encryption with independent keys
//! - Cipher caching for performance
//! - Context-based authenticated encryption for audit trails
//! - Key rotation support via cache invalidation

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use super::FieldEncryption;
use crate::secrets_manager::{SecretsError, SecretsManager};

/// Trait for managing encrypted fields in database adapters
///
/// Enables automatic encryption/decryption at the query layer without
/// requiring manual encryption/decryption in application code.
// Reason: `EncryptedFieldAdapter` is used only as a static bound — there is no `dyn`
// usage in this codebase. The `async_fn_in_trait` warning about Send-unbounded futures
// is not applicable here. If `dyn` dispatch is ever needed, switch to explicit
// `-> impl Future<Output = ...> + Send` return types or add `#[async_trait]`.
#[allow(async_fn_in_trait)] // Reason: trait is internal and not object-safe; async fn in trait is acceptable
pub trait EncryptedFieldAdapter: Send + Sync {
    /// Get list of encrypted field names
    fn get_encrypted_fields(&self) -> Vec<String>;

    /// Check if a field is encrypted
    fn is_encrypted(&self, field_name: &str) -> bool {
        self.get_encrypted_fields().contains(&field_name.to_string())
    }

    /// Encrypt a plaintext value for the given field
    async fn encrypt_value(
        &self,
        field_name: &str,
        plaintext: &str,
    ) -> Result<Vec<u8>, SecretsError>;

    /// Decrypt an encrypted value for the given field
    async fn decrypt_value(
        &self,
        field_name: &str,
        ciphertext: &[u8],
    ) -> Result<String, SecretsError>;

    /// Encrypt with additional context for audit trail
    async fn encrypt_with_context(
        &self,
        field_name: &str,
        plaintext: &str,
        context: &str,
    ) -> Result<Vec<u8>, SecretsError>;

    /// Decrypt with context verification
    async fn decrypt_with_context(
        &self,
        field_name: &str,
        ciphertext: &[u8],
        context: &str,
    ) -> Result<String, SecretsError>;
}

/// Encryption context for audit trail inclusion in AES-GCM AAD.
///
/// The context string is included as Additional Authenticated Data (AAD)
/// during encryption and **must be reproduced exactly at decrypt time**.
/// For this reason the context only contains stable identifiers (user ID,
/// field name, operation) — never wall-clock timestamps or other mutable
/// values that would differ between the encrypt and decrypt calls.
///
/// Format produced by `to_aad_string()`: `"user:{id}:field:{name}:op:{op}"`
#[derive(Debug, Clone)]
pub struct EncryptionContext {
    /// User ID performing the operation
    pub user_id: String,
    /// Field name being encrypted
    pub field_name: String,
    /// Operation type (insert, update, select)
    pub operation: String,
}

impl EncryptionContext {
    /// Create new encryption context.
    ///
    /// The produced `to_aad_string()` must be stored or reconstructed
    /// identically when decrypting the corresponding ciphertext.
    pub fn new(
        user_id: impl Into<String>,
        field_name: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            field_name: field_name.into(),
            operation: operation.into(),
        }
    }

    /// Convert context to stable AAD string.
    ///
    /// This value is bound into the AES-GCM authentication tag. It must be
    /// supplied unchanged to `decrypt_with_context`; any difference causes
    /// authentication failure.
    pub fn to_aad_string(&self) -> String {
        format!("user:{}:field:{}:op:{}", self.user_id, self.field_name, self.operation)
    }
}

/// Cached encryption cipher for a field
///
/// Holds the cipher behind an `Arc` so all concurrent callers share a single
/// heap allocation. The key schedule is zeroed when the last `Arc` is dropped.
#[derive(Clone)]
struct CachedEncryption {
    cipher: Arc<FieldEncryption>,
}

/// Basic implementation of `EncryptedFieldAdapter`
///
/// Uses `SecretsManager` to fetch encryption keys from Vault
/// and caches ciphers for performance.
pub struct DatabaseFieldAdapter {
    /// Secrets manager for fetching encryption keys
    secrets_manager: Arc<SecretsManager>,
    /// Mapping of field names to encryption key names in Vault
    field_keys: HashMap<String, String>,
    /// Cached cipher instances per field
    ciphers: Arc<RwLock<HashMap<String, CachedEncryption>>>,
}

impl DatabaseFieldAdapter {
    /// Create new database field adapter
    ///
    /// # Arguments
    ///
    /// * `secrets_manager` - `SecretsManager` for fetching encryption keys from Vault
    /// * `field_keys` - Mapping of database field names to Vault key names
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: SecretsManager backed by Vault or another live secret store.
    /// use std::collections::HashMap;
    /// use std::sync::Arc;
    /// use fraiseql_secrets::encryption::database_adapter::DatabaseFieldAdapter;
    /// use fraiseql_secrets::secrets_manager::SecretsManager;
    /// # async fn example(secrets_manager: Arc<SecretsManager>) {
    /// let mut field_keys = HashMap::new();
    /// field_keys.insert("email".to_string(), "db/email_key".to_string());
    /// field_keys.insert("phone".to_string(), "db/phone_key".to_string());
    ///
    /// let adapter = DatabaseFieldAdapter::new(secrets_manager, field_keys);
    /// # }
    /// ```
    pub fn new(secrets_manager: Arc<SecretsManager>, field_keys: HashMap<String, String>) -> Self {
        Self {
            secrets_manager,
            field_keys,
            ciphers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create cached cipher for field.
    ///
    /// Returns an `Arc` so callers share the single heap allocation; no key
    /// bytes are duplicated on each request.
    async fn get_cipher(&self, field_name: &str) -> Result<Arc<FieldEncryption>, SecretsError> {
        // Check cache first
        let cache = self.ciphers.read().await;
        if let Some(cached) = cache.get(field_name) {
            return Ok(Arc::clone(&cached.cipher));
        }
        drop(cache);

        // Fetch key from SecretsManager
        let key_name = self.field_keys.get(field_name).ok_or_else(|| {
            SecretsError::NotFound(format!(
                "Encryption key for field '{}' not configured",
                field_name
            ))
        })?;

        let key_str = self.secrets_manager.get_secret(key_name).await?;
        let key_bytes = key_str.as_bytes().to_vec();

        // FieldEncryption::new() validates the key length and returns Err on mismatch.
        let cipher = Arc::new(FieldEncryption::new(&key_bytes).map_err(|e| {
            SecretsError::ValidationError(format!(
                "Invalid encryption key for field '{}': {}",
                field_name, e
            ))
        })?);

        // Cache for future use
        let mut cache = self.ciphers.write().await;
        cache.insert(
            field_name.to_string(),
            CachedEncryption {
                cipher: Arc::clone(&cipher),
            },
        );

        Ok(cipher)
    }

    /// Register new encrypted field with its encryption key
    ///
    /// # Arguments
    ///
    /// * `field_name` - Database field name to be encrypted
    /// * `key_name` - Vault secret name for the encryption key
    pub fn register_field(&mut self, field_name: impl Into<String>, key_name: impl Into<String>) {
        self.field_keys.insert(field_name.into(), key_name.into());
    }

    /// Invalidate cipher cache, forcing fresh key retrieval from `SecretsManager`
    ///
    /// Useful after key rotation in Vault. Next encryption/decryption
    /// will fetch the new key and create a new cipher.
    pub async fn invalidate_cache(&self) {
        let mut cache = self.ciphers.write().await;
        cache.clear();
    }

    /// Invalidate cache for specific field
    ///
    /// # Arguments
    ///
    /// * `field_name` - Field to invalidate cache for
    pub async fn invalidate_field_cache(&self, field_name: &str) {
        let mut cache = self.ciphers.write().await;
        cache.remove(field_name);
    }

    /// Get current cache size
    ///
    /// Returns number of cached ciphers (one per encrypted field being used).
    pub async fn cache_size(&self) -> usize {
        self.ciphers.read().await.len()
    }
}

impl EncryptedFieldAdapter for DatabaseFieldAdapter {
    fn get_encrypted_fields(&self) -> Vec<String> {
        self.field_keys.keys().cloned().collect()
    }

    async fn encrypt_value(
        &self,
        field_name: &str,
        plaintext: &str,
    ) -> Result<Vec<u8>, SecretsError> {
        let cipher = self.get_cipher(field_name).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to get encryption cipher for field '{}': {}",
                field_name, e
            ))
        })?;

        cipher.encrypt(plaintext).map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to encrypt value for field '{}': {}",
                field_name, e
            ))
        })
    }

    async fn decrypt_value(
        &self,
        field_name: &str,
        ciphertext: &[u8],
    ) -> Result<String, SecretsError> {
        let cipher = self.get_cipher(field_name).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to get decryption cipher for field '{}': {}",
                field_name, e
            ))
        })?;

        cipher.decrypt(ciphertext).map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to decrypt value for field '{}': {}",
                field_name, e
            ))
        })
    }

    async fn encrypt_with_context(
        &self,
        field_name: &str,
        plaintext: &str,
        context: &str,
    ) -> Result<Vec<u8>, SecretsError> {
        let cipher = self.get_cipher(field_name).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to get encryption cipher for field '{}': {}",
                field_name, e
            ))
        })?;

        cipher.encrypt_with_context(plaintext, context).map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to encrypt value with context for field '{}': {}",
                field_name, e
            ))
        })
    }

    async fn decrypt_with_context(
        &self,
        field_name: &str,
        ciphertext: &[u8],
        context: &str,
    ) -> Result<String, SecretsError> {
        let cipher = self.get_cipher(field_name).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to get decryption cipher for field '{}': {}",
                field_name, e
            ))
        })?;

        cipher.decrypt_with_context(ciphertext, context).map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to decrypt value with context for field '{}': {}",
                field_name, e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_context_creation() {
        let ctx = EncryptionContext::new("user123", "email", "insert");
        assert_eq!(ctx.user_id, "user123");
        assert_eq!(ctx.field_name, "email");
        assert_eq!(ctx.operation, "insert");
    }

    #[test]
    fn test_encryption_context_aad_string() {
        let ctx = EncryptionContext::new("user456", "phone", "update");
        let aad = ctx.to_aad_string();
        assert!(aad.contains("user:user456"));
        assert!(aad.contains("field:phone"));
        assert!(aad.contains("op:update"));
    }

    /// AAD must be stable across calls so decrypt can reproduce it
    #[test]
    fn test_encryption_context_aad_is_stable() {
        let ctx1 = EncryptionContext::new("u1", "email", "insert");
        let ctx2 = EncryptionContext::new("u1", "email", "insert");
        assert_eq!(ctx1.to_aad_string(), ctx2.to_aad_string());
    }

    // ── DatabaseFieldAdapter unit tests ─────────────────────────────────

    fn make_adapter_with_fields(fields: &[(&str, &str)]) -> DatabaseFieldAdapter {
        use crate::secrets_manager::EnvBackend;
        let sm = Arc::new(SecretsManager::new(Arc::new(EnvBackend)));
        let mut fk = HashMap::new();
        for (field, key) in fields {
            fk.insert((*field).to_string(), (*key).to_string());
        }
        DatabaseFieldAdapter::new(sm, fk)
    }

    #[test]
    fn test_get_encrypted_fields_returns_configured_fields() {
        let adapter =
            make_adapter_with_fields(&[("email", "vault/email_key"), ("phone", "vault/phone_key")]);
        let mut fields = adapter.get_encrypted_fields();
        fields.sort();
        assert_eq!(fields, vec!["email", "phone"]);
    }

    #[test]
    fn test_get_encrypted_fields_empty_when_no_fields() {
        let adapter = make_adapter_with_fields(&[]);
        assert!(adapter.get_encrypted_fields().is_empty());
    }

    #[test]
    fn test_is_encrypted_true_for_configured_field() {
        let adapter = make_adapter_with_fields(&[("email", "vault/email_key")]);
        assert!(adapter.is_encrypted("email"));
    }

    #[test]
    fn test_is_encrypted_false_for_unconfigured_field() {
        let adapter = make_adapter_with_fields(&[("email", "vault/email_key")]);
        assert!(!adapter.is_encrypted("phone"));
    }

    #[tokio::test]
    async fn test_cache_size_empty_initially() {
        let adapter = make_adapter_with_fields(&[("email", "vault/email_key")]);
        assert_eq!(adapter.cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_invalidate_cache_clears_all() {
        let adapter = make_adapter_with_fields(&[("email", "vault/email_key")]);
        // Cache starts empty, invalidate should be a no-op
        adapter.invalidate_cache().await;
        assert_eq!(adapter.cache_size().await, 0);
    }

    #[test]
    fn test_register_field_adds_new_field() {
        let mut adapter = make_adapter_with_fields(&[]);
        adapter.register_field("ssn", "vault/ssn_key");
        assert!(adapter.is_encrypted("ssn"));
        assert_eq!(adapter.get_encrypted_fields(), vec!["ssn"]);
    }
}
