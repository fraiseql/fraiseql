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
    use crate::secrets_manager::{SecretsError, types::SecretsBackend};

    /// In-memory secrets backend for testing
    struct MockSecretsBackend {
        secrets: HashMap<String, String>,
    }

    impl MockSecretsBackend {
        fn new(secrets: HashMap<String, String>) -> Self {
            Self { secrets }
        }
    }

    #[async_trait::async_trait]
    impl SecretsBackend for MockSecretsBackend {
        async fn get_secret(&self, name: &str) -> Result<String, SecretsError> {
            self.secrets
                .get(name)
                .cloned()
                .ok_or_else(|| SecretsError::NotFound(format!("Secret '{}' not found", name)))
        }

        async fn get_secret_with_expiry(
            &self,
            name: &str,
        ) -> Result<(String, chrono::DateTime<chrono::Utc>), SecretsError> {
            let secret = self.get_secret(name).await?;
            Ok((secret, chrono::Utc::now() + chrono::Duration::hours(24)))
        }

        async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
            self.get_secret(name).await
        }
    }

    /// Helper: build a SecretsManager with mock backend containing 32-byte keys
    fn mock_secrets_manager(keys: Vec<(&str, &str)>) -> Arc<SecretsManager> {
        let mut secrets = HashMap::new();
        for (name, value) in keys {
            secrets.insert(name.to_string(), value.to_string());
        }
        Arc::new(SecretsManager::new(Arc::new(MockSecretsBackend::new(secrets))))
    }

    /// A 32-byte string key for testing
    const KEY_32B_A: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const KEY_32B_B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

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
    async fn test_adapter_get_cipher_caching() {
        let sm = mock_secrets_manager(vec![("db/email_key", KEY_32B_A)]);
        let mut field_keys = HashMap::new();
        field_keys.insert("email".to_string(), "db/email_key".to_string());
        let adapter = DatabaseFieldAdapter::new(sm, field_keys);

        // First access fetches from backend and caches
        assert_eq!(adapter.cache_size().await, 0);
        let _ = adapter.encrypt_value("email", "test@example.com").await.unwrap();
        assert_eq!(adapter.cache_size().await, 1);

        // Second access uses cached cipher (cache size stays at 1)
        let _ = adapter.encrypt_value("email", "other@example.com").await.unwrap();
        assert_eq!(adapter.cache_size().await, 1);
    }

    #[tokio::test]
    async fn test_adapter_multiple_keys() {
        let sm =
            mock_secrets_manager(vec![("db/email_key", KEY_32B_A), ("db/phone_key", KEY_32B_B)]);
        let mut field_keys = HashMap::new();
        field_keys.insert("email".to_string(), "db/email_key".to_string());
        field_keys.insert("phone".to_string(), "db/phone_key".to_string());
        let adapter = DatabaseFieldAdapter::new(sm, field_keys);

        // Encrypt with different keys
        let email_ct = adapter.encrypt_value("email", "hello").await.unwrap();
        let phone_ct = adapter.encrypt_value("phone", "hello").await.unwrap();

        // Same plaintext encrypted with different keys produces different ciphertexts
        assert_ne!(email_ct, phone_ct);

        // Each field decrypts correctly with its own key
        let email_pt = adapter.decrypt_value("email", &email_ct).await.unwrap();
        let phone_pt = adapter.decrypt_value("phone", &phone_ct).await.unwrap();
        assert_eq!(email_pt, "hello");
        assert_eq!(phone_pt, "hello");

        // Cross-decryption fails (email ciphertext with phone key)
        assert!(adapter.decrypt_value("phone", &email_ct).await.is_err());
    }

    #[tokio::test]
    async fn test_adapter_cache_invalidation() {
        let sm = mock_secrets_manager(vec![("db/email_key", KEY_32B_A)]);
        let mut field_keys = HashMap::new();
        field_keys.insert("email".to_string(), "db/email_key".to_string());
        let adapter = DatabaseFieldAdapter::new(sm, field_keys);

        // Populate cache
        let _ = adapter.encrypt_value("email", "test").await.unwrap();
        assert_eq!(adapter.cache_size().await, 1);

        // Invalidate all caches
        adapter.invalidate_cache().await;
        assert_eq!(adapter.cache_size().await, 0);

        // Next access re-fetches from backend
        let _ = adapter.encrypt_value("email", "test").await.unwrap();
        assert_eq!(adapter.cache_size().await, 1);

        // Invalidate single field
        adapter.invalidate_field_cache("email").await;
        assert_eq!(adapter.cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_adapter_missing_key_error() {
        let sm = mock_secrets_manager(vec![("db/email_key", KEY_32B_A)]);
        let mut field_keys = HashMap::new();
        field_keys.insert("email".to_string(), "db/email_key".to_string());
        let adapter = DatabaseFieldAdapter::new(sm, field_keys);

        // Encrypt for unregistered field returns error
        let result = adapter.encrypt_value("ssn", "123-45-6789").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("ssn"), "Error should mention the missing field");
    }

    #[tokio::test]
    async fn test_adapter_is_encrypted_check() {
        let sm = mock_secrets_manager(vec![("db/email_key", KEY_32B_A)]);
        let mut field_keys = HashMap::new();
        field_keys.insert("email".to_string(), "db/email_key".to_string());
        field_keys.insert("phone".to_string(), "db/phone_key".to_string());
        let adapter = DatabaseFieldAdapter::new(sm, field_keys);

        // Registered fields are encrypted
        assert!(adapter.is_encrypted("email"));
        assert!(adapter.is_encrypted("phone"));

        // Unregistered fields are not encrypted
        assert!(!adapter.is_encrypted("name"));
        assert!(!adapter.is_encrypted("address"));
    }
}
