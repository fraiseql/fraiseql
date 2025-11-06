/// GraphQL Schema Registry for Rust
///
/// This module provides a thread-safe registry for storing GraphQL schema metadata
/// that enables type resolution and transformation at runtime.
///
/// The registry is initialized once at application startup with schema data from Python
/// and then used for all subsequent query transformations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Global schema registry instance (initialized once at startup)
static REGISTRY: OnceLock<SchemaRegistry> = OnceLock::new();

/// Field metadata describing a GraphQL field's type information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldInfo {
    /// The GraphQL type name (e.g., "String", "Equipment", "User")
    pub type_name: String,

    /// Whether this field is a nested object (true) or scalar (false)
    pub is_nested_object: bool,

    /// Whether this field is a list type (e.g., [User])
    pub is_list: bool,

    /// Extension fields for future compatibility
    /// Fields added in future versions will be stored here without breaking deserialization
    #[serde(flatten)]
    pub extensions: HashMap<String, serde_json::Value>,
}

impl FieldInfo {
    /// Get the type name of this field
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Check if this is a nested object type
    pub fn is_nested_object(&self) -> bool {
        self.is_nested_object
    }

    /// Check if this is a list type
    pub fn is_list(&self) -> bool {
        self.is_list
    }
}

/// Type metadata describing a GraphQL object type's fields
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TypeInfo {
    /// Map of field names to their metadata
    pub fields: HashMap<String, FieldInfo>,
}

/// GraphQL Schema Registry
///
/// Stores type metadata from the GraphQL schema for use in runtime type resolution.
/// Initialized once at application startup and then accessed read-only from all threads.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchemaRegistry {
    /// Schema IR version (for forward compatibility)
    pub version: String,

    /// Feature flags (capabilities supported by this schema)
    pub features: Vec<String>,

    /// Map of type names to their metadata
    pub types: HashMap<String, TypeInfo>,
}

impl SchemaRegistry {
    /// Create a new SchemaRegistry from JSON schema IR
    ///
    /// # Arguments
    /// * `json` - JSON string containing the schema IR from Python
    ///
    /// # Returns
    /// * `Ok(SchemaRegistry)` - Successfully parsed schema
    /// * `Err(String)` - Parse error with description
    ///
    /// # Example
    /// ```ignore
    /// let schema_json = r#"{"version": "1.0", "features": [], "types": {}}"#;
    /// let registry = SchemaRegistry::from_json(schema_json)?;
    /// ```
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse schema JSON: {}", e))
    }

    /// Get the schema IR version
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Check if a feature is enabled in this schema
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(&feature.to_string())
    }

    /// Look up field type information
    ///
    /// # Arguments
    /// * `type_name` - The parent type name (e.g., "Assignment")
    /// * `field_name` - The field name (e.g., "equipment")
    ///
    /// # Returns
    /// * `Some(&FieldInfo)` - Field information if found
    /// * `None` - Type or field not found
    ///
    /// # Performance
    /// This is an O(1) HashMap lookup
    pub fn get_field_type(&self, type_name: &str, field_name: &str) -> Option<&FieldInfo> {
        self.types
            .get(type_name)
            .and_then(|type_info| type_info.fields.get(field_name))
    }

    /// Get the number of types in the registry
    pub fn type_count(&self) -> usize {
        self.types.len()
    }
}

/// Initialize the global schema registry
///
/// This should be called once at application startup.
/// Subsequent calls will be ignored (returns false).
///
/// # Arguments
/// * `registry` - The SchemaRegistry instance to install
///
/// # Returns
/// * `true` - Registry was successfully initialized
/// * `false` - Registry was already initialized
pub fn initialize_registry(registry: SchemaRegistry) -> bool {
    REGISTRY.set(registry).is_ok()
}

/// Get a reference to the global schema registry
///
/// # Returns
/// * `Some(&SchemaRegistry)` - If registry has been initialized
/// * `None` - If registry has not been initialized yet
pub fn get_registry() -> Option<&'static SchemaRegistry> {
    REGISTRY.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_info_deserialization() {
        let json = r#"{
            "type_name": "String",
            "is_nested_object": false,
            "is_list": false
        }"#;

        let field_info: FieldInfo = serde_json::from_str(json).unwrap();
        assert_eq!(field_info.type_name(), "String");
        assert!(!field_info.is_nested_object());
        assert!(!field_info.is_list());
    }

    #[test]
    fn test_field_info_with_extensions() {
        // Future schema might include additional fields
        let json = r#"{
            "type_name": "String",
            "is_nested_object": false,
            "is_list": false,
            "future_field": "ignored",
            "another_field": 123
        }"#;

        // Should deserialize without error (unknown fields in extensions)
        let field_info: FieldInfo = serde_json::from_str(json).unwrap();
        assert_eq!(field_info.type_name(), "String");
        assert_eq!(field_info.extensions.len(), 2);
    }

    #[test]
    fn test_schema_registry_basic() {
        let json = r#"{
            "version": "1.0",
            "features": ["type_resolution"],
            "types": {
                "User": {
                    "fields": {
                        "id": {
                            "type_name": "ID",
                            "is_nested_object": false,
                            "is_list": false
                        }
                    }
                }
            }
        }"#;

        let registry = SchemaRegistry::from_json(json).unwrap();
        assert_eq!(registry.version(), "1.0");
        assert!(registry.has_feature("type_resolution"));
        assert_eq!(registry.type_count(), 1);

        let field = registry.get_field_type("User", "id").unwrap();
        assert_eq!(field.type_name(), "ID");
    }
}
