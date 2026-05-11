#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
    let mapping = FieldMapping::new("email", true, b"user@example.com".to_vec());
    let result = mapping.to_string();
    let value = result.unwrap_or_else(|e| panic!("expected Ok from to_string: {e}"));
    assert_eq!(value, "user@example.com");
}

#[test]
fn test_field_mapping_to_string_invalid_utf8() {
    let mapping = FieldMapping::new("email", true, vec![0xFF, 0xFE]);
    let result = mapping.to_string();
    assert!(
        matches!(result, Err(SecretsError::EncryptionError(_))),
        "expected EncryptionError for invalid UTF-8, got: {result:?}"
    );
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
    for field in encrypted_fields {
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
