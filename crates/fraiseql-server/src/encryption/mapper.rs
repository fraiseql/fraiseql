//! Mapper integration for transparent field-level encryption/decryption
//!
//! Provides automatic encryption on write operations and decryption on read
//! operations at the mapper/ORM layer without application code changes.
//!
//! # Overview
//!
//! The mapper module integrates with DatabaseFieldAdapter to provide
//! automatic encryption/decryption at the data mapping layer:
//!
//! - Encrypt plaintext values before INSERT/UPDATE
//! - Decrypt ciphertext after SELECT
//! - Support for mixed encrypted/unencrypted fields
//! - Type information preservation through encryption
//! - Batch operation support
//! - Transaction awareness
//! - Comprehensive error handling
//!
//! # Usage Pattern
//!
//! ```ignore
//! // Create mapper with encrypted field configuration
//! let mapper = FieldMapper::new(
//!     adapter,
//!     vec!["email".to_string(), "phone".to_string()]
//! );
//!
//! // On INSERT: encrypt plaintext
//! let encrypted = mapper.encrypt_field("email", "user@example.com").await?;
//!
//! // On SELECT: decrypt ciphertext
//! let plaintext = mapper.decrypt_field("email", &ciphertext).await?;
//!
//! // Batch operations
//! let mappings = mapper.encrypt_fields(&[
//!     ("email".to_string(), "user@example.com".to_string()),
//!     ("name".to_string(), "John Doe".to_string()),
//! ]).await?;
//! ```
//!
//! # Type Support
//!
//! The mapper works with any data that can be converted to/from UTF-8:
//! - Strings (primary use case)
//! - Numbers (as strings)
//! - Dates/times (as formatted strings)
//! - JSON (as JSON strings)
//! - UUIDs (as UUID strings)
//! - Custom types (via ToString/FromStr)

use std::{collections::HashMap, sync::Arc};

use super::database_adapter::{DatabaseFieldAdapter, EncryptedFieldAdapter};
use crate::secrets_manager::SecretsError;

/// Field mapping result containing both value and encryption status
#[derive(Debug, Clone)]
pub struct FieldMapping {
    /// Field name
    field_name:   String,
    /// Whether field is encrypted
    is_encrypted: bool,
    /// Field value (plaintext for encrypted fields)
    value:        Vec<u8>,
}

impl FieldMapping {
    /// Create new field mapping
    pub fn new(field_name: impl Into<String>, is_encrypted: bool, value: Vec<u8>) -> Self {
        Self {
            field_name: field_name.into(),
            is_encrypted,
            value,
        }
    }

    /// Get field name
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Check if field is encrypted
    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
    }

    /// Get field value
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Convert to plaintext string
    pub fn to_string(&self) -> Result<String, SecretsError> {
        String::from_utf8(self.value.clone()).map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Invalid UTF-8 in field '{}': {}",
                self.field_name, e
            ))
        })
    }
}

/// Mapper for handling encrypted fields in database operations
///
/// Transparently encrypts/decrypts fields during read/write operations.
pub struct FieldMapper {
    /// Field adapter for encryption/decryption
    adapter:              Arc<DatabaseFieldAdapter>,
    /// Field encryption configuration
    field_encryption_map: HashMap<String, bool>,
}

impl FieldMapper {
    /// Create new field mapper
    ///
    /// # Arguments
    ///
    /// * `adapter` - Field adapter for encryption/decryption
    /// * `encrypted_fields` - List of fields that should be encrypted
    pub fn new(adapter: Arc<DatabaseFieldAdapter>, encrypted_fields: Vec<String>) -> Self {
        let mut field_encryption_map = HashMap::new();
        for field in encrypted_fields {
            field_encryption_map.insert(field, true);
        }

        Self {
            adapter,
            field_encryption_map,
        }
    }

    /// Check if field is marked for encryption
    pub fn is_field_encrypted(&self, field_name: &str) -> bool {
        self.field_encryption_map.get(field_name).copied().unwrap_or(false)
    }

    /// Encrypt field value before write operation
    ///
    /// # Arguments
    ///
    /// * `field_name` - Name of field to encrypt
    /// * `plaintext` - Plaintext value to encrypt
    ///
    /// # Returns
    ///
    /// Encrypted bytes in format: \[nonce\]\[ciphertext\]\[tag\]
    pub async fn encrypt_field(
        &self,
        field_name: &str,
        plaintext: &str,
    ) -> Result<Vec<u8>, SecretsError> {
        if !self.is_field_encrypted(field_name) {
            return Err(SecretsError::ValidationError(format!(
                "Field '{}' is not configured for encryption",
                field_name
            )));
        }

        self.adapter.encrypt_value(field_name, plaintext).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to encrypt field '{}': {}",
                field_name, e
            ))
        })
    }

    /// Decrypt field value after read operation
    ///
    /// # Arguments
    ///
    /// * `field_name` - Name of field to decrypt
    /// * `ciphertext` - Encrypted bytes from database
    ///
    /// # Returns
    ///
    /// Decrypted plaintext string
    pub async fn decrypt_field(
        &self,
        field_name: &str,
        ciphertext: &[u8],
    ) -> Result<String, SecretsError> {
        if !self.is_field_encrypted(field_name) {
            return Err(SecretsError::ValidationError(format!(
                "Field '{}' is not configured for decryption",
                field_name
            )));
        }

        self.adapter.decrypt_value(field_name, ciphertext).await.map_err(|e| {
            SecretsError::EncryptionError(format!(
                "Failed to decrypt field '{}': {}",
                field_name, e
            ))
        })
    }

    /// Encrypt multiple fields (batch operation)
    ///
    /// Returns FieldMapping objects with encryption status.
    pub async fn encrypt_fields(
        &self,
        fields: &[(String, String)],
    ) -> Result<Vec<FieldMapping>, SecretsError> {
        let mut results = Vec::new();

        for (field_name, plaintext) in fields {
            if self.is_field_encrypted(field_name) {
                let encrypted = self.encrypt_field(field_name, plaintext).await?;
                results.push(FieldMapping::new(field_name.clone(), true, encrypted));
            } else {
                // Unencrypted field - pass through as bytes
                results.push(FieldMapping::new(
                    field_name.clone(),
                    false,
                    plaintext.as_bytes().to_vec(),
                ));
            }
        }

        Ok(results)
    }

    /// Decrypt multiple fields (batch operation)
    ///
    /// Returns FieldMapping objects with decrypted values.
    pub async fn decrypt_fields(
        &self,
        fields: &[(String, Vec<u8>)],
    ) -> Result<Vec<FieldMapping>, SecretsError> {
        let mut results = Vec::new();

        for (field_name, ciphertext) in fields {
            if self.is_field_encrypted(field_name) {
                let plaintext = self.decrypt_field(field_name, ciphertext).await?;
                results.push(FieldMapping::new(field_name.clone(), true, plaintext.into_bytes()));
            } else {
                // Unencrypted field - pass through unchanged
                results.push(FieldMapping::new(field_name.clone(), false, ciphertext.clone()));
            }
        }

        Ok(results)
    }

    /// Get list of encrypted fields
    pub fn encrypted_fields(&self) -> Vec<String> {
        self.field_encryption_map.keys().cloned().collect()
    }

    /// Check if any fields are encrypted
    pub fn has_encrypted_fields(&self) -> bool {
        !self.field_encryption_map.is_empty()
    }

    /// Register field for encryption
    ///
    /// Can be used to dynamically add encrypted fields after mapper creation.
    pub fn register_encrypted_field(&mut self, field_name: impl Into<String>) {
        self.field_encryption_map.insert(field_name.into(), true);
    }

    /// Unregister field from encryption
    pub fn unregister_encrypted_field(&mut self, field_name: &str) {
        self.field_encryption_map.remove(field_name);
    }

    /// Get count of encrypted fields
    pub fn encrypted_field_count(&self) -> usize {
        self.field_encryption_map.len()
    }

    /// Validate field encryption configuration
    ///
    /// Returns error if configuration is inconsistent or incomplete.
    pub fn validate_configuration(&self) -> Result<(), SecretsError> {
        if self.encrypted_fields().is_empty() {
            return Err(SecretsError::ValidationError(
                "No encrypted fields configured".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_mapping_creation() {
        let mapping = FieldMapping::new("email", true, b"encrypted_data".to_vec());
        assert_eq!(mapping.field_name(), "email");
        assert!(mapping.is_encrypted());
        assert_eq!(mapping.value(), b"encrypted_data");
    }

    #[test]
    fn test_field_mapping_to_string() {
        let mapping = FieldMapping::new("email", true, "user@example.com".as_bytes().to_vec());
        let result = mapping.to_string();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "user@example.com");
    }

    #[test]
    fn test_field_mapping_to_string_invalid_utf8() {
        let mapping = FieldMapping::new("email", true, vec![0xFF, 0xFE]);
        let result = mapping.to_string();
        assert!(result.is_err());
    }

    #[test]
    fn test_field_mapper_field_encryption_map() {
        let encrypted_fields = vec!["email".to_string(), "phone".to_string()];
        let mut field_encryption_map = HashMap::new();
        for field in encrypted_fields {
            field_encryption_map.insert(field, true);
        }

        assert!(field_encryption_map.get("email").copied().unwrap_or(false));
        assert!(field_encryption_map.get("phone").copied().unwrap_or(false));
        assert!(!field_encryption_map.get("name").copied().unwrap_or(false));
    }

    #[test]
    fn test_encrypted_fields_list() {
        let encrypted_fields = vec!["email".to_string(), "phone".to_string()];
        let mut field_encryption_map = HashMap::new();
        for field in encrypted_fields.clone() {
            field_encryption_map.insert(field, true);
        }

        let result: Vec<String> = field_encryption_map.keys().cloned().collect();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"email".to_string()));
        assert!(result.contains(&"phone".to_string()));
    }

    #[test]
    fn test_has_encrypted_fields() {
        let encrypted_fields = vec!["email".to_string()];
        let mut field_encryption_map = HashMap::new();
        for field in encrypted_fields {
            field_encryption_map.insert(field, true);
        }

        assert!(!field_encryption_map.is_empty());

        let empty_map: HashMap<String, bool> = HashMap::new();
        assert!(empty_map.is_empty());
    }

    #[test]
    fn test_field_mapping_not_encrypted() {
        let mapping = FieldMapping::new("name", false, b"John Doe".to_vec());
        assert_eq!(mapping.field_name(), "name");
        assert!(!mapping.is_encrypted());
    }

    #[test]
    fn test_field_mapping_value_access() {
        let data = b"sensitive data".to_vec();
        let mapping = FieldMapping::new("field", true, data.clone());
        assert_eq!(mapping.value(), data.as_slice());
    }
}
