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
#[allow(async_fn_in_trait)]
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

/// Encryption context for audit trail inclusion
///
/// Format: "user:{user_id}:field:{field_name}:timestamp:{timestamp}"
#[derive(Debug, Clone)]
pub struct EncryptionContext {
    /// User ID performing the operation
    pub user_id:    String,
    /// Field name being encrypted
    pub field_name: String,
    /// Operation type (insert, update, select)
    pub operation:  String,
    /// Timestamp of operation
    pub timestamp:  String,
}

impl EncryptionContext {
    /// Create new encryption context
    pub fn new(
        user_id: impl Into<String>,
        field_name: impl Into<String>,
        operation: impl Into<String>,
        timestamp: impl Into<String>,
    ) -> Self {
        Self {
            user_id:    user_id.into(),
            field_name: field_name.into(),
            operation:  operation.into(),
            timestamp:  timestamp.into(),
        }
    }

    /// Convert context to string for authenticated data
    pub fn to_aad_string(&self) -> String {
        format!(
            "user:{}:field:{}:op:{}:ts:{}",
            self.user_id, self.field_name, self.operation, self.timestamp
        )
    }
}

/// Cached encryption cipher for a field
#[derive(Clone)]
struct CachedEncryption {
    cipher:   FieldEncryption,
    /// Key name from Vault - kept for debugging and audit trail
    #[allow(dead_code)]
    key_name: String,
}

/// Basic implementation of EncryptedFieldAdapter
///
/// Uses SecretsManager to fetch encryption keys from Vault
/// and caches ciphers for performance.
pub struct DatabaseFieldAdapter {
    /// Secrets manager for fetching encryption keys
    secrets_manager: Arc<SecretsManager>,
    /// Mapping of field names to encryption key names in Vault
    field_keys:      HashMap<String, String>,
    /// Cached cipher instances per field
    ciphers:         Arc<RwLock<HashMap<String, CachedEncryption>>>,
}

impl DatabaseFieldAdapter {
    /// Create new database field adapter
    ///
    /// # Arguments
    ///
    /// * `secrets_manager` - SecretsManager for fetching encryption keys from Vault
    /// * `field_keys` - Mapping of database field names to Vault key names
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut field_keys = HashMap::new();
    /// field_keys.insert("email".to_string(), "db/email_key".to_string());
    /// field_keys.insert("phone".to_string(), "db/phone_key".to_string());
    ///
    /// let adapter = DatabaseFieldAdapter::new(secrets_manager, field_keys);
    /// ```
    pub fn new(secrets_manager: Arc<SecretsManager>, field_keys: HashMap<String, String>) -> Self {
        Self {
            secrets_manager,
            field_keys,
            ciphers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create cached cipher for field
    async fn get_cipher(&self, field_name: &str) -> Result<FieldEncryption, SecretsError> {
        // Check cache first
        let cache = self.ciphers.read().await;
        if let Some(cached) = cache.get(field_name) {
            return Ok(cached.cipher.clone());
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

        if key_bytes.len() != 32 {
            return Err(SecretsError::ValidationError(format!(
                "Encryption key for field '{}' must be 32 bytes, got {}",
                field_name,
                key_bytes.len()
            )));
        }

        let cipher = FieldEncryption::new(&key_bytes);

        // Cache for future use
        let mut cache = self.ciphers.write().await;
        cache.insert(
            field_name.to_string(),
            CachedEncryption {
                cipher:   cipher.clone(),
                key_name: key_name.clone(),
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

    /// Invalidate cipher cache, forcing fresh key retrieval from SecretsManager
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
        let ctx = EncryptionContext::new("user123", "email", "insert", "2024-01-01T00:00:00Z");
        assert_eq!(ctx.user_id, "user123");
        assert_eq!(ctx.field_name, "email");
        assert_eq!(ctx.operation, "insert");
    }

    #[test]
    fn test_encryption_context_aad_string() {
        let ctx = EncryptionContext::new("user456", "phone", "update", "2024-01-02T12:00:00Z");
        let aad = ctx.to_aad_string();
        assert!(aad.contains("user:user456"));
        assert!(aad.contains("field:phone"));
        assert!(aad.contains("op:update"));
        assert!(aad.contains("ts:2024-01-02T12:00:00Z"));
    }

    #[tokio::test]
    #[ignore = "Requires SecretsManager setup"]
    async fn test_adapter_get_cipher_caching() {
        // When cipher accessed multiple times for same field
        // Should return cached instance on subsequent calls
    }

    #[tokio::test]
    #[ignore = "Incomplete test: needs actual implementation"]
    async fn test_adapter_multiple_keys() {
        // When adapter configured with multiple fields and keys
        // Each field should use its own encryption key
        // Keys sourced from SecretsManager
    }

    #[tokio::test]
    #[ignore = "Incomplete test: needs actual implementation"]
    async fn test_adapter_cache_invalidation() {
        // When cache invalidated (e.g., after key rotation)
        // Next access should fetch fresh key from SecretsManager
        // Old cached ciphers discarded
    }

    #[tokio::test]
    #[ignore = "Incomplete test: needs actual implementation"]
    async fn test_adapter_missing_key_error() {
        // When field not registered in adapter
        // encrypt_value should return NotFound error
        // Should indicate which key missing
    }

    #[tokio::test]
    #[ignore = "Incomplete test: needs actual implementation"]
    async fn test_adapter_is_encrypted_check() {
        // When checking if field is encrypted
        // Should return true for registered fields
        // Should return false for unregistered fields
    }
}
