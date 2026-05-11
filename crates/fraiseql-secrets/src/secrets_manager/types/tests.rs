#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// Test Secret wrapper redacts in Debug output
#[test]
fn test_secret_debug_redaction() {
    let secret = Secret::new("my_secret_password".to_string());
    let debug_str = format!("{:?}", secret);

    assert!(debug_str.contains("***"), "Debug should redact secret");
    assert!(
        !debug_str.contains("my_secret_password"),
        "Debug should not contain actual value"
    );
    assert_eq!(debug_str, "Secret(***)");
}

/// Test Secret wrapper redacts in Display output
#[test]
fn test_secret_display_redaction() {
    let secret = Secret::new("api_key_12345".to_string());
    let display_str = format!("{}", secret);

    assert_eq!(display_str, "***", "Display should only show ***");
}

/// Test `Secret.expose()` returns actual value
#[test]
fn test_secret_expose() {
    let value = "actual_secret_value".to_string();
    let secret = Secret::new(value.clone());

    assert_eq!(secret.expose(), &value);
}

/// Test `Secret.into_exposed()` consumes and returns value
#[test]
fn test_secret_into_exposed() {
    let value = "test_secret".to_string();
    let secret = Secret::new(value.clone());

    let exposed = secret.into_exposed();
    assert_eq!(exposed, value);
}

/// Test Secret equality based on actual value
#[test]
fn test_secret_equality() {
    let secret1 = Secret::new("same_value".to_string());
    let secret2 = Secret::new("same_value".to_string());
    let secret3 = Secret::new("different_value".to_string());

    assert_eq!(secret1, secret2, "Secrets with same value should be equal");
    assert_ne!(secret1, secret3, "Secrets with different values should not be equal");
}

/// Test Secret length and `is_empty`
#[test]
fn test_secret_properties() {
    let secret = Secret::new("test".to_string());
    assert_eq!(secret.len(), 4);
    assert!(!secret.is_empty());

    let empty = Secret::new(String::new());
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
}

/// Test `SecretsBackend` trait requirements
#[test]
fn test_secrets_backend_trait_definition() {
    // Trait should require:
    // 1. Send + Sync for thread safety
    // 2. get_secret(&self, name: &str) -> Future<Result<String>>
    // 3. get_secret_with_expiry(&self, name: &str) -> Future<Result<(String, DateTime<Utc>)>>
    // 4. rotate_secret(&self, name: &str) -> Future<Result<String>>
    // All methods async for I/O operations
}
