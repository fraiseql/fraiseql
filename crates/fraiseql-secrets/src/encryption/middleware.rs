//! Encryption middleware for transparent field-level encryption/decryption.
//!
//! Integrates with the GraphQL execution pipeline to:
//! - Decrypt encrypted fields in query responses (after executor)
//! - Encrypt fields in mutation variables (before executor)
//!
//! Uses the compiled schema's `FieldEncryptionConfig` metadata to determine
//! which fields require encryption, and `DatabaseFieldAdapter` for the
//! actual AES-256-GCM operations via `SecretsManager`.

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
    /// Map: `type_name` -> list of encrypted field names
    encrypted_fields: HashMap<String, Vec<String>>,
    /// Map: operation name (query/mutation) -> return type name
    operation_types:  HashMap<String, String>,
    /// Adapter for encrypt/decrypt operations (fetches keys from `SecretsManager`)
    adapter:          Arc<DatabaseFieldAdapter>,
    /// Optional key rotation manager for versioned encryption
    rotation_manager: Option<Arc<CredentialRotationManager>>,
}

impl FieldEncryptionService {
    /// Create a new encryption service with explicit configuration.
    #[must_use] 
    pub const fn new(
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
    #[must_use] 
    pub const fn with_rotation(
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
    #[must_use] 
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
                .map(|f| f.name.to_string())
                .collect();
            if !enc.is_empty() {
                encrypted_fields.insert(type_def.name.to_string(), enc);
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
    #[must_use] 
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
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::EncryptionError` if base64 decoding or decryption fails.
    pub async fn decrypt_response(
        &self,
        response: &mut serde_json::Value,
    ) -> Result<(), SecretsError> {
        let Some(data) = response.get_mut("data") else {
            return Ok(());
        };

        let Some(data_obj) = data.as_object_mut() else {
            return Ok(());
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
    ///
    /// # Errors
    ///
    /// Returns `SecretsError::EncryptionError` if encryption of any field fails.
    pub async fn encrypt_variables(
        &self,
        variables: &mut serde_json::Value,
        target_type: &str,
    ) -> Result<(), SecretsError> {
        let Some(enc_fields) = self.encrypted_fields.get(target_type) else {
            return Ok(());
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
    #[must_use] 
    pub fn needs_rotation(&self) -> bool {
        self.rotation_manager
            .as_ref()
            .and_then(|rm| rm.needs_refresh().ok())
            .unwrap_or(false)
    }

    /// Get the rotation manager if configured.
    #[must_use] 
    pub fn rotation_manager(&self) -> Option<&CredentialRotationManager> {
        self.rotation_manager.as_deref()
    }

    /// Trigger key rotation if a rotation manager is configured.
    ///
    /// Returns the new key version, or `None` if rotation is not configured.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError::EncryptionError`] if the underlying rotation manager fails.
    pub fn rotate_key(&self) -> Result<Option<u16>, SecretsError> {
        match &self.rotation_manager {
            Some(rm) => {
                let version = rm.rotate_key().map_err(|e| {
                    SecretsError::EncryptionError(format!("Key rotation failed: {}", e))
                })?;
                // Invalidate cipher cache so new key is fetched on next operation.
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
mod tests;
