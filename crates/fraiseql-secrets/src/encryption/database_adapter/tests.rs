#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
