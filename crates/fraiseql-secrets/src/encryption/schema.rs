//! Schema detection for automatically identifying and managing encrypted fields.
//!
//! Supports multiple encryption marks (`#[encrypted]`, `#[sensitive]`, `#[encrypt(key="...")]`),
//! key reference management, and schema evolution tracking.

use std::collections::HashMap;

use crate::secrets_manager::SecretsError;

/// Encryption mark type used in struct annotations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncryptionMark {
    /// Basic `#[encrypted]` mark
    Encrypted,
    /// Alternative `#[sensitive]` mark
    Sensitive,
    /// Explicit `#[encrypt(...)]` with configuration
    Encrypt,
}

impl std::fmt::Display for EncryptionMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encrypted => write!(f, "encrypted"),
            Self::Sensitive => write!(f, "sensitive"),
            Self::Encrypt => write!(f, "encrypt"),
        }
    }
}

/// Metadata about an encrypted field in schema
#[derive(Debug, Clone)]
pub struct SchemaFieldInfo {
    /// Field name in struct
    pub field_name:    String,
    /// Field type (e.g., "String", "Uuid", "`DateTime<Utc>`")
    pub field_type:    String,
    /// Whether field is marked for encryption
    pub is_encrypted:  bool,
    /// Key reference path for encryption (e.g., "encryption/email")
    pub key_reference: String,
    /// Encryption algorithm hint
    pub algorithm:     String,
    /// Whether field can be NULL
    pub nullable:      bool,
    /// Which encryption mark was used
    pub mark:          Option<EncryptionMark>,
}

impl SchemaFieldInfo {
    /// Create new field info
    pub fn new(
        field_name: impl Into<String>,
        field_type: impl Into<String>,
        is_encrypted: bool,
        key_reference: impl Into<String>,
    ) -> Self {
        Self {
            field_name: field_name.into(),
            field_type: field_type.into(),
            is_encrypted,
            key_reference: key_reference.into(),
            algorithm: "aes256-gcm".to_string(),
            nullable: false,
            mark: None,
        }
    }

    /// Set algorithm hint
    pub fn with_algorithm(mut self, algorithm: impl Into<String>) -> Self {
        self.algorithm = algorithm.into();
        self
    }

    /// Mark as nullable
    #[must_use]
    pub const fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Set encryption mark
    #[must_use]
    pub const fn with_mark(mut self, mark: EncryptionMark) -> Self {
        self.mark = Some(mark);
        self
    }
}

/// Schema information for a struct type
#[derive(Debug, Clone)]
pub struct StructSchema {
    /// Type name (e.g., "User")
    pub type_name:        String,
    /// All fields in struct (including non-encrypted)
    pub all_fields:       Vec<SchemaFieldInfo>,
    /// Only encrypted fields (subset of `all_fields`)
    pub encrypted_fields: Vec<SchemaFieldInfo>,
    /// Schema version for evolution tracking
    pub version:          u32,
}

impl StructSchema {
    /// Create new struct schema
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name:        type_name.into(),
            all_fields:       Vec::new(),
            encrypted_fields: Vec::new(),
            version:          1,
        }
    }

    /// Add field to schema
    pub fn add_field(&mut self, field: SchemaFieldInfo) {
        if field.is_encrypted {
            self.encrypted_fields.push(field.clone());
        }
        self.all_fields.push(field);
    }

    /// Add multiple fields
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<SchemaFieldInfo>) -> Self {
        for field in fields {
            self.add_field(field);
        }
        self
    }

    /// Set schema version for evolution tracking
    #[must_use]
    pub const fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// Get field by name
    #[must_use]
    pub fn get_field(&self, field_name: &str) -> Option<&SchemaFieldInfo> {
        self.all_fields.iter().find(|f| f.field_name == field_name)
    }

    /// Get encrypted field by name
    #[must_use]
    pub fn get_encrypted_field(&self, field_name: &str) -> Option<&SchemaFieldInfo> {
        self.encrypted_fields.iter().find(|f| f.field_name == field_name)
    }

    /// Check if field is encrypted
    #[must_use]
    pub fn is_field_encrypted(&self, field_name: &str) -> bool {
        self.encrypted_fields.iter().any(|f| f.field_name == field_name)
    }

    /// Get list of encrypted field names
    #[must_use]
    pub fn encrypted_field_names(&self) -> Vec<&str> {
        self.encrypted_fields.iter().map(|f| f.field_name.as_str()).collect()
    }

    /// Internal filter helper to reduce duplication
    fn filter_fields<F>(&self, predicate: F) -> Vec<&SchemaFieldInfo>
    where
        F: Fn(&&SchemaFieldInfo) -> bool,
    {
        self.all_fields.iter().filter(predicate).collect()
    }

    /// Get fields that are marked as nullable
    #[must_use]
    pub fn nullable_encrypted_fields(&self) -> Vec<&SchemaFieldInfo> {
        self.filter_fields(|f| f.is_encrypted && f.nullable)
    }

    /// Get fields requiring specific encryption key
    #[must_use]
    pub fn fields_for_key(&self, key_ref: &str) -> Vec<&SchemaFieldInfo> {
        self.filter_fields(|f| f.key_reference == key_ref)
    }

    /// Count encrypted fields
    #[must_use]
    pub const fn encrypted_field_count(&self) -> usize {
        self.encrypted_fields.len()
    }

    /// Count total fields
    #[must_use]
    pub const fn total_field_count(&self) -> usize {
        self.all_fields.len()
    }

    /// Validate schema configuration
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::ValidationError`] if the type name is empty or any encrypted
    /// field is missing its key reference.
    pub fn validate(&self) -> Result<(), SecretsError> {
        if self.type_name.is_empty() {
            return Err(SecretsError::ValidationError(
                "Schema type name cannot be empty".to_string(),
            ));
        }

        // Validate each encrypted field has key reference
        for field in &self.encrypted_fields {
            if field.key_reference.is_empty() {
                return Err(SecretsError::ValidationError(format!(
                    "Encrypted field '{}' missing key reference",
                    field.field_name
                )));
            }
        }

        Ok(())
    }
}

/// Registry for managing schemas of different types
pub struct SchemaRegistry {
    /// Map of type name to schema
    schemas:               HashMap<String, StructSchema>,
    /// Default key reference for fields without explicit key
    default_key_reference: String,
}

impl SchemaRegistry {
    /// Create new schema registry
    #[must_use]
    pub fn new() -> Self {
        Self {
            schemas:               HashMap::new(),
            default_key_reference: "encryption/default".to_string(),
        }
    }

    /// Set default key reference
    pub fn with_default_key(mut self, key_reference: impl Into<String>) -> Self {
        self.default_key_reference = key_reference.into();
        self
    }

    /// Register schema
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::ValidationError`] if the schema fails validation (see
    /// [`StructSchema::validate`]).
    pub fn register(&mut self, schema: StructSchema) -> Result<(), SecretsError> {
        schema.validate()?;
        self.schemas.insert(schema.type_name.clone(), schema);
        Ok(())
    }

    /// Get schema by type name
    #[must_use]
    pub fn get(&self, type_name: &str) -> Option<&StructSchema> {
        self.schemas.get(type_name)
    }

    /// Get encrypted fields for type
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::ValidationError` if the type name is not registered.
    pub fn get_encrypted_fields(
        &self,
        type_name: &str,
    ) -> Result<Vec<&SchemaFieldInfo>, SecretsError> {
        self.get(type_name)
            .map(|schema| schema.encrypted_fields.iter().collect())
            .ok_or_else(|| {
                SecretsError::ValidationError(format!("Schema '{}' not registered", type_name))
            })
    }

    /// Check if type has encrypted fields
    #[must_use]
    pub fn has_encrypted_fields(&self, type_name: &str) -> bool {
        self.get(type_name).is_some_and(|schema| !schema.encrypted_fields.is_empty())
    }

    /// Get list of all registered types
    #[must_use]
    pub fn list_types(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
    }

    /// Get list of all types that have encrypted fields
    #[must_use]
    pub fn types_with_encryption(&self) -> Vec<&str> {
        self.schemas
            .iter()
            .filter(|(_, schema)| !schema.encrypted_fields.is_empty())
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Get all encryption keys used across all schemas
    #[must_use]
    pub fn all_encryption_keys(&self) -> Vec<String> {
        let mut keys = std::collections::HashSet::new();
        for schema in self.schemas.values() {
            for field in &schema.encrypted_fields {
                keys.insert(field.key_reference.clone());
            }
        }
        let mut sorted: Vec<_> = keys.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Validate all registered schemas
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::ValidationError`] if any registered schema fails validation.
    pub fn validate_all(&self) -> Result<(), SecretsError> {
        for schema in self.schemas.values() {
            schema.validate()?;
        }
        Ok(())
    }

    /// Unregister schema
    pub fn unregister(&mut self, type_name: &str) -> Option<StructSchema> {
        self.schemas.remove(type_name)
    }

    /// Clear all schemas
    pub fn clear(&mut self) {
        self.schemas.clear();
    }

    /// Count registered schemas
    #[must_use]
    pub fn count(&self) -> usize {
        self.schemas.len()
    }

    /// Count total encrypted fields across all schemas
    #[must_use]
    pub fn total_encrypted_fields(&self) -> usize {
        self.schemas.values().map(|schema| schema.encrypted_fields.len()).sum()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
