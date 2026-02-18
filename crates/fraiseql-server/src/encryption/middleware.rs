//! Encryption middleware for transparent field-level encryption/decryption.
//!
//! Integrates with the GraphQL execution pipeline to:
//! - Decrypt encrypted fields in query responses (after executor)
//! - Encrypt fields in mutation variables (before executor)
//!
//! Uses the compiled schema's `FieldEncryptionConfig` metadata to determine
//! which fields require encryption, and `DatabaseFieldAdapter` for the
//! actual AES-256-GCM operations via SecretsManager.

use std::{collections::HashMap, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use super::{
    credential_rotation::CredentialRotationManager,
    database_adapter::{DatabaseFieldAdapter, EncryptedFieldAdapter},
};
use crate::secrets_manager::SecretsError;

/// Middleware service for transparent field-level encryption in GraphQL pipelines.
///
/// Built from the compiled schema at server startup. Knows which fields on which
/// types are encrypted and maps operation names to their return types so it can
/// walk response JSON and encrypt/decrypt the right values.
///
/// Optionally supports key versioning via `CredentialRotationManager`. When rotation
/// is configured, ciphertext is prefixed with a 2-byte big-endian version tag:
/// ```text
/// [2-byte version][12-byte nonce][ciphertext][16-byte GCM tag]
/// ```
pub struct FieldEncryptionService {
    /// Map: type_name -> list of encrypted field names
    encrypted_fields: HashMap<String, Vec<String>>,
    /// Map: operation name (query/mutation) -> return type name
    operation_types:  HashMap<String, String>,
    /// Adapter for encrypt/decrypt operations (fetches keys from SecretsManager)
    adapter:          Arc<DatabaseFieldAdapter>,
    /// Optional key rotation manager for versioned encryption
    rotation_manager: Option<Arc<CredentialRotationManager>>,
}

impl FieldEncryptionService {
    /// Create a new encryption service with explicit configuration.
    pub fn new(
        encrypted_fields: HashMap<String, Vec<String>>,
        operation_types: HashMap<String, String>,
        adapter: Arc<DatabaseFieldAdapter>,
    ) -> Self {
        Self {
            encrypted_fields,
            operation_types,
            adapter,
            rotation_manager: None,
        }
    }

    /// Create a new encryption service with key rotation support.
    pub fn with_rotation(
        encrypted_fields: HashMap<String, Vec<String>>,
        operation_types: HashMap<String, String>,
        adapter: Arc<DatabaseFieldAdapter>,
        rotation_manager: Arc<CredentialRotationManager>,
    ) -> Self {
        Self {
            encrypted_fields,
            operation_types,
            adapter,
            rotation_manager: Some(rotation_manager),
        }
    }

    /// Build from a compiled schema.
    ///
    /// Scans all type definitions for fields with `encryption` config,
    /// and maps query/mutation names to their return types.
    pub fn from_schema(
        schema: &fraiseql_core::schema::CompiledSchema,
        adapter: Arc<DatabaseFieldAdapter>,
    ) -> Self {
        let mut encrypted_fields: HashMap<String, Vec<String>> = HashMap::new();
        let mut operation_types: HashMap<String, String> = HashMap::new();

        // Scan types for encrypted fields
        for type_def in &schema.types {
            let enc: Vec<String> = type_def
                .fields
                .iter()
                .filter(|f| f.encryption.is_some())
                .map(|f| f.name.clone())
                .collect();
            if !enc.is_empty() {
                encrypted_fields.insert(type_def.name.clone(), enc);
            }
        }

        // Map query names to return types
        for query in &schema.queries {
            operation_types.insert(query.name.clone(), query.return_type.clone());
        }
        for mutation in &schema.mutations {
            operation_types.insert(mutation.name.clone(), mutation.return_type.clone());
        }

        Self {
            encrypted_fields,
            operation_types,
            adapter,
            rotation_manager: None,
        }
    }

    /// Check if any fields in the schema require encryption.
    pub fn has_encrypted_fields(&self) -> bool {
        !self.encrypted_fields.is_empty()
    }

    /// Get the return type for an operation name.
    pub fn get_return_type(&self, operation_name: &str) -> Option<&str> {
        self.operation_types.get(operation_name).map(String::as_str)
    }

    /// Decrypt encrypted fields in a GraphQL response.
    ///
    /// Walks the `{"data": { "<operation>": ... }}` structure, resolves each
    /// operation to its return type, and decrypts base64-encoded ciphertext
    /// back to plaintext for encrypted fields.
    pub async fn decrypt_response(
        &self,
        response: &mut serde_json::Value,
    ) -> Result<(), SecretsError> {
        let data = match response.get_mut("data") {
            Some(d) => d,
            None => return Ok(()),
        };

        let data_obj = match data.as_object_mut() {
            Some(obj) => obj,
            None => return Ok(()),
        };

        for (operation_name, value) in data_obj.iter_mut() {
            if let Some(type_name) = self.operation_types.get(operation_name) {
                if let Some(enc_fields) = self.encrypted_fields.get(type_name) {
                    self.decrypt_value(value, enc_fields).await?;
                }
            }
        }

        Ok(())
    }

    /// Recursively decrypt fields in a JSON value.
    ///
    /// Handles both single objects and arrays of objects.
    async fn decrypt_value(
        &self,
        value: &mut serde_json::Value,
        encrypted_fields: &[String],
    ) -> Result<(), SecretsError> {
        match value {
            serde_json::Value::Object(obj) => {
                for field_name in encrypted_fields {
                    if let Some(field_value) = obj.get_mut(field_name.as_str()) {
                        if let Some(encoded) = field_value.as_str() {
                            if encoded.is_empty() {
                                continue;
                            }
                            let ciphertext = BASE64.decode(encoded).map_err(|e| {
                                SecretsError::EncryptionError(format!(
                                    "Failed to base64-decode field '{}': {}",
                                    field_name, e
                                ))
                            })?;
                            let plaintext =
                                self.adapter.decrypt_value(field_name, &ciphertext).await?;
                            *field_value = serde_json::Value::String(plaintext);
                        }
                    }
                }
            },
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Box::pin(self.decrypt_value(item, encrypted_fields)).await?;
                }
            },
            _ => {},
        }

        Ok(())
    }

    /// Encrypt fields in mutation variables before execution.
    ///
    /// Walks the variables object and encrypts fields that match the
    /// target type's encrypted field list, base64-encoding the ciphertext.
    pub async fn encrypt_variables(
        &self,
        variables: &mut serde_json::Value,
        target_type: &str,
    ) -> Result<(), SecretsError> {
        let enc_fields = match self.encrypted_fields.get(target_type) {
            Some(fields) => fields,
            None => return Ok(()),
        };

        self.encrypt_value(variables, enc_fields).await
    }

    /// Encrypt matching fields in a JSON value.
    async fn encrypt_value(
        &self,
        value: &mut serde_json::Value,
        encrypted_fields: &[String],
    ) -> Result<(), SecretsError> {
        if let Some(obj) = value.as_object_mut() {
            for field_name in encrypted_fields {
                if let Some(field_value) = obj.get_mut(field_name.as_str()) {
                    if let Some(plaintext) = field_value.as_str() {
                        let ciphertext = self.adapter.encrypt_value(field_name, plaintext).await?;
                        *field_value = serde_json::Value::String(BASE64.encode(&ciphertext));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if key rotation is needed (80% of TTL consumed).
    pub fn needs_rotation(&self) -> bool {
        self.rotation_manager
            .as_ref()
            .and_then(|rm| rm.needs_refresh().ok())
            .unwrap_or(false)
    }

    /// Get the rotation manager if configured.
    pub fn rotation_manager(&self) -> Option<&CredentialRotationManager> {
        self.rotation_manager.as_deref()
    }

    /// Trigger key rotation if a rotation manager is configured.
    ///
    /// Returns the new key version, or `None` if rotation is not configured.
    pub fn rotate_key(&self) -> Result<Option<u16>, SecretsError> {
        match &self.rotation_manager {
            Some(rm) => {
                let version = rm.rotate_key().map_err(|e| {
                    SecretsError::EncryptionError(format!("Key rotation failed: {}", e))
                })?;
                // Invalidate cipher cache so new key is fetched on next operation
                let adapter = Arc::clone(&self.adapter);
                tokio::spawn(async move {
                    adapter.invalidate_cache().await;
                });
                Ok(Some(version))
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn test_version_extraction_from_ciphertext() {
        use super::super::credential_rotation::CredentialRotationManager;

        // Version 42 as big-endian u16: [0x00, 0x2A]
        let ciphertext = vec![0x00, 0x2A, 0xFF, 0xFF];
        let version =
            CredentialRotationManager::extract_version_from_ciphertext(&ciphertext).unwrap();
        assert_eq!(version, 42);
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
}
