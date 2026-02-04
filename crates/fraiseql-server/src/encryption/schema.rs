// Phase 12.3 Cycle 6: Schema Detection (GREEN)
//! Schema detection for automatically identifying and managing encrypted fields.
//!
//! Supports multiple encryption marks (#[encrypted], #[sensitive], #[encrypt(key="...")]),
//! key reference management, and schema evolution tracking.

use crate::secrets_manager::SecretsError;
use std::collections::HashMap;

/// Encryption mark type used in struct annotations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMark {
    /// Basic #[encrypted] mark
    Encrypted,
    /// Alternative #[sensitive] mark
    Sensitive,
    /// Explicit #[encrypt(...)] with configuration
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
    pub field_name: String,
    /// Field type (e.g., "String", "Uuid", "DateTime<Utc>")
    pub field_type: String,
    /// Whether field is marked for encryption
    pub is_encrypted: bool,
    /// Key reference path for encryption (e.g., "encryption/email")
    pub key_reference: String,
    /// Encryption algorithm hint
    pub algorithm: String,
    /// Whether field can be NULL
    pub nullable: bool,
    /// Which encryption mark was used
    pub mark: Option<EncryptionMark>,
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
    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Set encryption mark
    pub fn with_mark(mut self, mark: EncryptionMark) -> Self {
        self.mark = Some(mark);
        self
    }
}

/// Schema information for a struct type
#[derive(Debug, Clone)]
pub struct StructSchema {
    /// Type name (e.g., "User")
    pub type_name: String,
    /// All fields in struct (including non-encrypted)
    pub all_fields: Vec<SchemaFieldInfo>,
    /// Only encrypted fields (subset of all_fields)
    pub encrypted_fields: Vec<SchemaFieldInfo>,
    /// Schema version for evolution tracking
    pub version: u32,
}

impl StructSchema {
    /// Create new struct schema
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            all_fields: Vec::new(),
            encrypted_fields: Vec::new(),
            version: 1,
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
    pub fn with_fields(mut self, fields: Vec<SchemaFieldInfo>) -> Self {
        for field in fields {
            self.add_field(field);
        }
        self
    }

    /// Set schema version for evolution tracking
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// Get field by name
    pub fn get_field(&self, field_name: &str) -> Option<&SchemaFieldInfo> {
        self.all_fields
            .iter()
            .find(|f| f.field_name == field_name)
    }

    /// Get encrypted field by name
    pub fn get_encrypted_field(&self, field_name: &str) -> Option<&SchemaFieldInfo> {
        self.encrypted_fields
            .iter()
            .find(|f| f.field_name == field_name)
    }

    /// Check if field is encrypted
    pub fn is_field_encrypted(&self, field_name: &str) -> bool {
        self.encrypted_fields
            .iter()
            .any(|f| f.field_name == field_name)
    }

    /// Get list of encrypted field names
    pub fn encrypted_field_names(&self) -> Vec<&str> {
        self.encrypted_fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect()
    }

    /// Validate schema configuration
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
    schemas: HashMap<String, StructSchema>,
    /// Default key reference for fields without explicit key
    default_key_reference: String,
}

impl SchemaRegistry {
    /// Create new schema registry
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
            default_key_reference: "encryption/default".to_string(),
        }
    }

    /// Set default key reference
    pub fn with_default_key(mut self, key_reference: impl Into<String>) -> Self {
        self.default_key_reference = key_reference.into();
        self
    }

    /// Register schema
    pub fn register(&mut self, schema: StructSchema) -> Result<(), SecretsError> {
        schema.validate()?;
        self.schemas.insert(schema.type_name.clone(), schema);
        Ok(())
    }

    /// Get schema by type name
    pub fn get(&self, type_name: &str) -> Option<&StructSchema> {
        self.schemas.get(type_name)
    }

    /// Get encrypted fields for type
    pub fn get_encrypted_fields(&self, type_name: &str) -> Result<Vec<&SchemaFieldInfo>, SecretsError> {
        self.get(type_name)
            .map(|schema| schema.encrypted_fields.iter().collect())
            .ok_or_else(|| {
                SecretsError::ValidationError(format!(
                    "Schema '{}' not registered",
                    type_name
                ))
            })
    }

    /// Check if type has encrypted fields
    pub fn has_encrypted_fields(&self, type_name: &str) -> bool {
        self.get(type_name)
            .map(|schema| !schema.encrypted_fields.is_empty())
            .unwrap_or(false)
    }

    /// Get list of all registered types
    pub fn list_types(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
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
    pub fn count(&self) -> usize {
        self.schemas.len()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_mark_display() {
        assert_eq!(EncryptionMark::Encrypted.to_string(), "encrypted");
        assert_eq!(EncryptionMark::Sensitive.to_string(), "sensitive");
        assert_eq!(EncryptionMark::Encrypt.to_string(), "encrypt");
    }

    #[test]
    fn test_field_info_creation() {
        let field = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        assert_eq!(field.field_name, "email");
        assert_eq!(field.field_type, "String");
        assert!(field.is_encrypted);
        assert_eq!(field.key_reference, "encryption/email");
        assert_eq!(field.algorithm, "aes256-gcm");
    }

    #[test]
    fn test_field_info_with_algorithm() {
        let field = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_algorithm("aes256-gcm");
        assert_eq!(field.algorithm, "aes256-gcm");
    }

    #[test]
    fn test_field_info_with_nullable() {
        let field = SchemaFieldInfo::new("email", "Option<String>", true, "encryption/email")
            .with_nullable(true);
        assert!(field.nullable);
    }

    #[test]
    fn test_field_info_with_mark() {
        let field = SchemaFieldInfo::new("email", "String", true, "encryption/email")
            .with_mark(EncryptionMark::Encrypted);
        assert_eq!(field.mark, Some(EncryptionMark::Encrypted));
    }

    #[test]
    fn test_struct_schema_creation() {
        let schema = StructSchema::new("User");
        assert_eq!(schema.type_name, "User");
        assert!(schema.all_fields.is_empty());
        assert!(schema.encrypted_fields.is_empty());
        assert_eq!(schema.version, 1);
    }

    #[test]
    fn test_struct_schema_add_field() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        assert_eq!(schema.all_fields.len(), 1);
        assert_eq!(schema.encrypted_fields.len(), 1);
    }

    #[test]
    fn test_struct_schema_mixed_fields() {
        let mut schema = StructSchema::new("User");
        let encrypted = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        let unencrypted = SchemaFieldInfo::new("name", "String", false, "");
        schema.add_field(encrypted);
        schema.add_field(unencrypted);
        assert_eq!(schema.all_fields.len(), 2);
        assert_eq!(schema.encrypted_fields.len(), 1);
    }

    #[test]
    fn test_struct_schema_get_field() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        assert!(schema.get_field("email").is_some());
        assert!(schema.get_field("phone").is_none());
    }

    #[test]
    fn test_struct_schema_is_field_encrypted() {
        let mut schema = StructSchema::new("User");
        let encrypted = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        let unencrypted = SchemaFieldInfo::new("name", "String", false, "");
        schema.add_field(encrypted);
        schema.add_field(unencrypted);
        assert!(schema.is_field_encrypted("email"));
        assert!(!schema.is_field_encrypted("name"));
    }

    #[test]
    fn test_struct_schema_encrypted_field_names() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        let phone = SchemaFieldInfo::new("phone", "String", true, "encryption/phone");
        let name = SchemaFieldInfo::new("name", "String", false, "");
        schema.add_field(email);
        schema.add_field(phone);
        schema.add_field(name);
        let names = schema.encrypted_field_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"email"));
        assert!(names.contains(&"phone"));
    }

    #[test]
    fn test_struct_schema_validate_success() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn test_struct_schema_validate_empty_type_name() {
        let schema = StructSchema::new("");
        let result = schema.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_struct_schema_validate_missing_key_reference() {
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "");
        schema.add_field(email);
        let result = schema.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_struct_schema_with_version() {
        let schema = StructSchema::new("User").with_version(2);
        assert_eq!(schema.version, 2);
    }

    #[test]
    fn test_schema_registry_creation() {
        let registry = SchemaRegistry::new();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_schema_registry_register() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        assert!(registry.register(schema).is_ok());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_schema_registry_get() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        registry.register(schema).unwrap();
        assert!(registry.get("User").is_some());
        assert!(registry.get("Product").is_none());
    }

    #[test]
    fn test_schema_registry_has_encrypted_fields() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        registry.register(schema).unwrap();
        assert!(registry.has_encrypted_fields("User"));
        assert!(!registry.has_encrypted_fields("Product"));
    }

    #[test]
    fn test_schema_registry_list_types() {
        let mut registry = SchemaRegistry::new();
        let mut user_schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        user_schema.add_field(email);
        registry.register(user_schema).unwrap();

        let mut product_schema = StructSchema::new("Product");
        let name = SchemaFieldInfo::new("name", "String", false, "");
        product_schema.add_field(name);
        registry.register(product_schema).unwrap();

        let types = registry.list_types();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&"User"));
        assert!(types.contains(&"Product"));
    }

    #[test]
    fn test_schema_registry_unregister() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        registry.register(schema).unwrap();
        assert_eq!(registry.count(), 1);

        registry.unregister("User");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_schema_registry_clear() {
        let mut registry = SchemaRegistry::new();
        let mut schema = StructSchema::new("User");
        let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
        schema.add_field(email);
        registry.register(schema).unwrap();
        assert_eq!(registry.count(), 1);

        registry.clear();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_schema_registry_default_instance() {
        let registry = SchemaRegistry::default();
        assert_eq!(registry.count(), 0);
    }
}
