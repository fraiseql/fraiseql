#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

use crate::secrets_manager::{SecretsBackend, SecretsError as SmError, SecretsManager};

/// In-memory secrets backend for testing (avoids `set_var` which is unsafe).
struct TestSecretsBackend {
    secrets: HashMap<String, String>,
}

impl TestSecretsBackend {
    fn new(secrets: HashMap<String, String>) -> Self {
        Self { secrets }
    }
}

#[async_trait::async_trait]
impl SecretsBackend for TestSecretsBackend {
    fn name(&self) -> &'static str {
        "test"
    }

    async fn health_check(&self) -> Result<(), SmError> {
        Ok(())
    }

    async fn get_secret(&self, name: &str) -> Result<String, SmError> {
        self.secrets
            .get(name)
            .cloned()
            .ok_or_else(|| SmError::NotFound(format!("Secret '{}' not found", name)))
    }

    async fn get_secret_with_expiry(
        &self,
        name: &str,
    ) -> Result<(String, chrono::DateTime<chrono::Utc>), SmError> {
        let value = self.get_secret(name).await?;
        Ok((value, chrono::Utc::now() + chrono::Duration::hours(1)))
    }

    async fn rotate_secret(&self, _name: &str) -> Result<String, SmError> {
        Err(SmError::NotFound("rotation not supported".to_string()))
    }
}

/// Create a test `DatabaseFieldAdapter` with an in-memory secrets backend.
fn create_test_database_adapter() -> DatabaseFieldAdapter {
    // The key must be exactly 32 bytes for AES-256.
    let key = "01234567890123456789012345678901".to_string(); // 32 ASCII chars
    let mut secrets = HashMap::new();
    secrets.insert("test/email_key".to_string(), key);

    let backend = Arc::new(TestSecretsBackend::new(secrets));
    let secrets_manager = Arc::new(SecretsManager::new(backend));

    let mut field_keys = HashMap::new();
    field_keys.insert("email".to_string(), "test/email_key".to_string());

    DatabaseFieldAdapter::new(secrets_manager, field_keys)
}

/// Build a test service with one encrypted type (User.email).
fn test_service() -> FieldEncryptionService {
    let mut encrypted_fields = HashMap::new();
    encrypted_fields.insert("User".to_string(), vec!["email".to_string()]);

    let mut operation_types = HashMap::new();
    operation_types.insert("users".to_string(), "User".to_string());
    operation_types.insert("createUser".to_string(), "User".to_string());

    FieldEncryptionService {
        encrypted_fields,
        operation_types,
        adapter: Arc::new(create_test_database_adapter()),
        rotation_manager: None,
    }
}

#[test]
fn test_has_encrypted_fields() {
    let service = test_service();
    assert!(service.has_encrypted_fields());
}

#[test]
fn test_has_no_encrypted_fields() {
    let service = FieldEncryptionService::new(
        HashMap::new(),
        HashMap::new(),
        Arc::new(create_test_database_adapter()),
    );
    assert!(!service.has_encrypted_fields());
}

#[test]
fn test_get_return_type() {
    let service = test_service();
    assert_eq!(service.get_return_type("users"), Some("User"));
    assert_eq!(service.get_return_type("createUser"), Some("User"));
    assert_eq!(service.get_return_type("unknown"), None);
}

#[tokio::test]
async fn test_decrypt_response_single_object() {
    let service = test_service();

    // Encrypt a value using the same key the adapter will use for decryption
    let adapter = create_test_database_adapter();
    let encrypted = adapter.encrypt_value("email", "user@example.com").await.unwrap();
    let encoded = BASE64.encode(&encrypted);

    let mut response = serde_json::json!({
        "data": {
            "users": {
                "id": 1,
                "email": encoded
            }
        }
    });

    service.decrypt_response(&mut response).await.unwrap();

    assert_eq!(response["data"]["users"]["email"], "user@example.com");
    assert_eq!(response["data"]["users"]["id"], 1); // Unencrypted field unchanged
}

#[tokio::test]
async fn test_decrypt_response_array() {
    let service = test_service();
    let adapter = create_test_database_adapter();

    let enc1 = BASE64.encode(adapter.encrypt_value("email", "a@test.com").await.unwrap());
    let enc2 = BASE64.encode(adapter.encrypt_value("email", "b@test.com").await.unwrap());

    let mut response = serde_json::json!({
        "data": {
            "users": [
                {"id": 1, "email": enc1},
                {"id": 2, "email": enc2}
            ]
        }
    });

    service.decrypt_response(&mut response).await.unwrap();

    assert_eq!(response["data"]["users"][0]["email"], "a@test.com");
    assert_eq!(response["data"]["users"][1]["email"], "b@test.com");
}

#[tokio::test]
async fn test_decrypt_response_no_data_key() {
    let service = test_service();
    let mut response = serde_json::json!({"errors": [{"message": "not found"}]});
    // Should not error — just no-op
    service.decrypt_response(&mut response).await.unwrap();
}

#[tokio::test]
async fn test_decrypt_response_unknown_operation() {
    let service = test_service();
    let mut response = serde_json::json!({
        "data": {
            "products": [{"name": "Widget"}]
        }
    });
    // Unknown operation — should be a no-op
    service.decrypt_response(&mut response).await.unwrap();
    assert_eq!(response["data"]["products"][0]["name"], "Widget");
}

#[tokio::test]
async fn test_encrypt_variables() {
    let service = test_service();

    let mut variables = serde_json::json!({
        "email": "user@example.com",
        "name": "Test User"
    });

    service.encrypt_variables(&mut variables, "User").await.unwrap();

    // email should now be base64-encoded ciphertext
    let encoded = variables["email"].as_str().unwrap();
    assert_ne!(encoded, "user@example.com");

    // Verify it decrypts back
    let ciphertext = BASE64.decode(encoded).unwrap();
    let adapter = create_test_database_adapter();
    let decrypted = adapter.decrypt_value("email", &ciphertext).await.unwrap();
    assert_eq!(decrypted, "user@example.com");

    // name should be unchanged (not encrypted)
    assert_eq!(variables["name"], "Test User");
}

#[tokio::test]
async fn test_encrypt_variables_unknown_type() {
    let service = test_service();
    let mut variables = serde_json::json!({"name": "test"});
    // Unknown type — should be a no-op
    service.encrypt_variables(&mut variables, "Product").await.unwrap();
    assert_eq!(variables["name"], "test");
}

#[tokio::test]
async fn test_roundtrip_encrypt_then_decrypt() {
    let service = test_service();

    // Simulate mutation: encrypt variables
    let mut variables = serde_json::json!({"email": "roundtrip@test.com"});
    service.encrypt_variables(&mut variables, "User").await.unwrap();

    let encrypted_email = variables["email"].as_str().unwrap().to_string();

    // Simulate query response with the encrypted value
    let mut response = serde_json::json!({
        "data": {
            "users": {"id": 1, "email": encrypted_email}
        }
    });

    service.decrypt_response(&mut response).await.unwrap();
    assert_eq!(response["data"]["users"]["email"], "roundtrip@test.com");
}

#[tokio::test]
async fn test_decrypt_skips_null_fields() {
    let service = test_service();

    let mut response = serde_json::json!({
        "data": {
            "users": {"id": 1, "email": null}
        }
    });

    // Should not error on null values
    service.decrypt_response(&mut response).await.unwrap();
    assert!(response["data"]["users"]["email"].is_null());
}

#[tokio::test]
async fn test_decrypt_skips_empty_string() {
    let service = test_service();

    let mut response = serde_json::json!({
        "data": {
            "users": {"id": 1, "email": ""}
        }
    });

    service.decrypt_response(&mut response).await.unwrap();
    assert_eq!(response["data"]["users"]["email"], "");
}

// =========================================================================
// Key Rotation Tests
// =========================================================================

fn test_service_with_rotation() -> FieldEncryptionService {
    use super::super::credential_rotation::{CredentialRotationManager, RotationConfig};

    let mut encrypted_fields = HashMap::new();
    encrypted_fields.insert("User".to_string(), vec!["email".to_string()]);

    let mut operation_types = HashMap::new();
    operation_types.insert("users".to_string(), "User".to_string());

    let config = RotationConfig::new().with_ttl_days(365);
    let rotation_manager = Arc::new(CredentialRotationManager::new(config));

    FieldEncryptionService {
        encrypted_fields,
        operation_types,
        adapter: Arc::new(create_test_database_adapter()),
        rotation_manager: Some(rotation_manager),
    }
}

#[test]
fn test_rotation_manager_accessible() {
    let service = test_service_with_rotation();
    assert!(service.rotation_manager().is_some());
}

#[test]
fn test_rotation_manager_none_without_config() {
    let service = test_service();
    assert!(service.rotation_manager().is_none());
}

#[test]
fn test_needs_rotation_false_when_fresh() {
    let service = test_service_with_rotation();
    let rm = service.rotation_manager().unwrap();
    rm.initialize_key().unwrap();
    // Freshly initialized key should not need rotation
    assert!(!service.needs_rotation());
}

#[test]
fn test_needs_rotation_false_without_manager() {
    let service = test_service();
    assert!(!service.needs_rotation());
}

#[tokio::test]
async fn test_rotate_key_increments_version() {
    let service = test_service_with_rotation();
    let rm = service.rotation_manager().unwrap();
    let v1 = rm.initialize_key().unwrap();
    let v2 = service.rotate_key().unwrap().unwrap();
    assert!(v2 > v1);
    assert_eq!(rm.get_current_version().unwrap(), v2);
}

#[tokio::test]
async fn test_rotate_key_returns_none_without_manager() {
    let service = test_service();
    let result = service.rotate_key().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_emergency_rotation_marks_compromised() {
    use super::super::credential_rotation::KeyVersionStatus;

    let service = test_service_with_rotation();
    let rm = service.rotation_manager().unwrap();
    let v1 = rm.initialize_key().unwrap();
    let v2 = rm.emergency_rotate("test breach").unwrap();

    // Old version should be compromised
    let v1_meta =
        rm.get_version_history().unwrap().into_iter().find(|m| m.version == v1).unwrap();
    assert_eq!(v1_meta.status, KeyVersionStatus::Compromised);

    // New version should be current
    assert_eq!(rm.get_current_version().unwrap(), v2);
}
