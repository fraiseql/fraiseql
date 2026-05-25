#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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
    schema.validate().unwrap_or_else(|e| panic!("expected Ok from validate: {e}"));
}

#[test]
fn test_struct_schema_validate_empty_type_name() {
    let schema = StructSchema::new("");
    let result = schema.validate();
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "expected ValidationError for empty type name, got: {result:?}"
    );
}

#[test]
fn test_struct_schema_validate_missing_key_reference() {
    let mut schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "");
    schema.add_field(email);
    let result = schema.validate();
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "expected ValidationError for missing key reference, got: {result:?}"
    );
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
    registry
        .register(schema)
        .unwrap_or_else(|e| panic!("expected Ok from register: {e}"));
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

#[test]
fn test_struct_schema_nullable_encrypted_fields() {
    let mut schema = StructSchema::new("User");
    let email =
        SchemaFieldInfo::new("email", "String", true, "encryption/email").with_nullable(true);
    let phone =
        SchemaFieldInfo::new("phone", "String", true, "encryption/phone").with_nullable(false);
    let name = SchemaFieldInfo::new("name", "String", false, "").with_nullable(true);
    schema.add_field(email);
    schema.add_field(phone);
    schema.add_field(name);
    let nullable = schema.nullable_encrypted_fields();
    assert_eq!(nullable.len(), 1);
    assert_eq!(nullable[0].field_name, "email");
}

#[test]
fn test_struct_schema_fields_for_key() {
    let mut schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    let phone = SchemaFieldInfo::new("phone", "String", true, "encryption/email");
    let ssn = SchemaFieldInfo::new("ssn", "String", true, "encryption/ssn");
    schema.add_field(email);
    schema.add_field(phone);
    schema.add_field(ssn);
    let email_fields = schema.fields_for_key("encryption/email");
    assert_eq!(email_fields.len(), 2);
}

#[test]
fn test_struct_schema_encrypted_field_count() {
    let mut schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    let phone = SchemaFieldInfo::new("phone", "String", true, "encryption/phone");
    schema.add_field(email);
    schema.add_field(phone);
    assert_eq!(schema.encrypted_field_count(), 2);
}

#[test]
fn test_struct_schema_total_field_count() {
    let mut schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    let name = SchemaFieldInfo::new("name", "String", false, "");
    schema.add_field(email);
    schema.add_field(name);
    assert_eq!(schema.total_field_count(), 2);
}

#[test]
fn test_schema_registry_types_with_encryption() {
    let mut registry = SchemaRegistry::new();
    let mut user_schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    user_schema.add_field(email);
    registry.register(user_schema).unwrap();

    let mut product_schema = StructSchema::new("Product");
    let name = SchemaFieldInfo::new("name", "String", false, "");
    product_schema.add_field(name);
    registry.register(product_schema).unwrap();

    let encrypted_types = registry.types_with_encryption();
    assert_eq!(encrypted_types.len(), 1);
    assert_eq!(encrypted_types[0], "User");
}

#[test]
fn test_schema_registry_all_encryption_keys() {
    let mut registry = SchemaRegistry::new();
    let mut user_schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    let phone = SchemaFieldInfo::new("phone", "String", true, "encryption/phone");
    user_schema.add_field(email);
    user_schema.add_field(phone);
    registry.register(user_schema).unwrap();

    let keys = registry.all_encryption_keys();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"encryption/email".to_string()));
    assert!(keys.contains(&"encryption/phone".to_string()));
}

#[test]
fn test_schema_registry_validate_all() {
    let mut registry = SchemaRegistry::new();
    let mut schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    schema.add_field(email);
    registry.register(schema).unwrap();
    registry
        .validate_all()
        .unwrap_or_else(|e| panic!("expected Ok from validate_all: {e}"));
}

#[test]
fn test_schema_registry_total_encrypted_fields() {
    let mut registry = SchemaRegistry::new();
    let mut user_schema = StructSchema::new("User");
    let email = SchemaFieldInfo::new("email", "String", true, "encryption/email");
    let phone = SchemaFieldInfo::new("phone", "String", true, "encryption/phone");
    user_schema.add_field(email);
    user_schema.add_field(phone);
    registry.register(user_schema).unwrap();

    let mut product_schema = StructSchema::new("Product");
    let sku = SchemaFieldInfo::new("sku", "String", true, "encryption/sku");
    product_schema.add_field(sku);
    registry.register(product_schema).unwrap();

    assert_eq!(registry.total_encrypted_fields(), 3);
}
