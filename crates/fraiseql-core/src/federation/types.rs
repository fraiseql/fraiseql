//! Federation types and metadata structures.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// Phase 1, Cycle 1: Field-level directive metadata structures

/// Federation metadata attached to compiled schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMetadata {
    /// Is federation enabled for this schema?
    pub enabled: bool,

    /// Federation specification version (e.g., "v2")
    pub version: String,

    /// Federation metadata per type
    pub types: Vec<FederatedType>,
}

impl Default for FederationMetadata {
    fn default() -> Self {
        Self {
            enabled: false,
            version: "v2".to_string(),
            types: Vec::new(),
        }
    }
}

/// Field-level federation directives (@requires, @provides, @shareable, @external)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldFederationDirectives {
    /// @requires directive - fields that must be present for this field to resolve
    pub requires: Vec<FieldPathSelection>,

    /// @provides directive - fields this resolver provides
    pub provides: Vec<FieldPathSelection>,

    /// @external directive - field is owned by another subgraph
    pub external: bool,

    /// @shareable directive - field is shareable across subgraphs
    pub shareable: bool,
}

impl FieldFederationDirectives {
    /// Create a new empty set of field directives
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the @requires directive
    pub fn with_requires(mut self, requires: Vec<FieldPathSelection>) -> Self {
        self.requires = requires;
        self
    }

    /// Add a single @requires dependency
    pub fn add_requires(mut self, field_path: FieldPathSelection) -> Self {
        self.requires.push(field_path);
        self
    }

    /// Set the @provides directive
    pub fn with_provides(mut self, provides: Vec<FieldPathSelection>) -> Self {
        self.provides = provides;
        self
    }

    /// Add a single @provides dependency
    pub fn add_provides(mut self, field_path: FieldPathSelection) -> Self {
        self.provides.push(field_path);
        self
    }

    /// Mark as @external
    pub fn external(mut self) -> Self {
        self.external = true;
        self
    }

    /// Mark as @shareable
    pub fn shareable(mut self) -> Self {
        self.shareable = true;
        self
    }
}

/// Field path selection for @requires/@provides (e.g., ["profile", "age"] for "profile.age")
/// Note: This is distinct from selection_parser::FieldSelection which represents requested fields
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldPathSelection {
    /// Path components: ["profile", "age"] for "profile.age"
    pub path: Vec<String>,

    /// The type this field belongs to (for context)
    pub typename: String,
}

/// Federated type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedType {
    /// Type name (e.g., "User")
    pub name: String,

    /// Keys that define the entity (@key directive)
    pub keys: Vec<KeyDirective>,

    /// Is this type extended from another subgraph?
    pub is_extends: bool,

    /// Fields that are external (owned by other subgraph)
    pub external_fields: Vec<String>,

    /// Fields that are shareable across subgraphs
    pub shareable_fields: Vec<String>,

    /// Field-level federation directives (Phase 1: Field-Level Metadata)
    pub field_directives: HashMap<String, FieldFederationDirectives>,
}

impl FederatedType {
    /// Create a new federated type with the given name
    #[must_use]
    pub fn new(name: String) -> Self {
        Self {
            name,
            keys: Vec::new(),
            is_extends: false,
            external_fields: Vec::new(),
            shareable_fields: Vec::new(),
            field_directives: HashMap::new(),
        }
    }

    /// Get field-level directives for a field, if they exist
    pub fn get_field_directives(&self, field_name: &str) -> Option<&FieldFederationDirectives> {
        self.field_directives.get(field_name)
    }

    /// Set field-level directives for a field
    pub fn set_field_directives(
        &mut self,
        field_name: String,
        directives: FieldFederationDirectives,
    ) {
        self.field_directives.insert(field_name, directives);
    }

    /// Check if a field has the @requires directive
    pub fn field_has_requires(&self, field_name: &str) -> bool {
        self.get_field_directives(field_name)
            .map(|d| !d.requires.is_empty())
            .unwrap_or(false)
    }

    /// Check if a field has the @provides directive
    pub fn field_has_provides(&self, field_name: &str) -> bool {
        self.get_field_directives(field_name)
            .map(|d| !d.provides.is_empty())
            .unwrap_or(false)
    }

    /// Check if a field is marked as @shareable
    pub fn field_is_shareable(&self, field_name: &str) -> bool {
        self.get_field_directives(field_name)
            .map(|d| d.shareable)
            .unwrap_or(false)
    }

    /// Check if a field is marked as @external
    pub fn field_is_external(&self, field_name: &str) -> bool {
        self.get_field_directives(field_name)
            .map(|d| d.external)
            .unwrap_or(false)
    }
}

/// @key directive for entity identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDirective {
    /// Field names comprising the key (space-separated or array)
    pub fields: Vec<String>,

    /// Whether this key is resolvable by this subgraph
    pub resolvable: bool,
}

/// Entity representation from _entities query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRepresentation {
    /// Type name (from __typename)
    pub typename: String,

    /// Key field values for this entity
    pub key_fields: HashMap<String, Value>,

    /// All fields in the representation
    pub all_fields: HashMap<String, Value>,
}

impl EntityRepresentation {
    /// Parse from _Any scalar input
    pub fn from_any(value: &Value) -> Result<Self, String> {
        let obj = value.as_object()
            .ok_or_else(|| "Entity representation must be object".to_string())?;

        let typename = obj.get("__typename")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "__typename field required".to_string())?
            .to_string();

        // Convert object to HashMap for easier access
        let mut all_fields = HashMap::new();
        for (key, val) in obj.iter() {
            all_fields.insert(key.clone(), val.clone());
        }

        Ok(EntityRepresentation {
            typename,
            key_fields: HashMap::new(), // Populated by resolver
            all_fields,
        })
    }

    /// Extract key fields based on key directive
    pub fn extract_key_fields(
        &mut self,
        key_fields_list: &[String],
    ) {
        for key_field in key_fields_list {
            if let Some(value) = self.all_fields.get(key_field) {
                self.key_fields.insert(key_field.clone(), value.clone());
            }
        }
    }
}

/// Resolution strategy for entity
#[derive(Debug, Clone)]
pub enum ResolutionStrategy {
    /// Entity is owned by this subgraph, resolve locally
    Local {
        /// View or table name to query
        view_name: String,
        /// Columns that form the key
        key_columns: Vec<String>,
    },

    /// Resolve via direct database connection to another subgraph
    DirectDatabase {
        /// Connection string or identifier
        connection_string: String,
        /// Key columns for WHERE clause
        key_columns: Vec<String>,
    },

    /// Resolve via HTTP to external subgraph
    Http {
        /// URL of the remote subgraph's GraphQL endpoint
        subgraph_url: String,
    },
}

impl std::fmt::Display for ResolutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionStrategy::Local { view_name, .. } => {
                write!(f, "Local({})", view_name)
            }
            ResolutionStrategy::DirectDatabase { connection_string, .. } => {
                write!(f, "DirectDB({})", connection_string)
            }
            ResolutionStrategy::Http { subgraph_url } => {
                write!(f, "Http({})", subgraph_url)
            }
        }
    }
}

/// Federation resolver - orchestrates entity resolution
pub struct FederationResolver {
    /// Federation metadata for the schema
    pub metadata: FederationMetadata,

    /// Cached resolution strategies
    pub strategy_cache: std::sync::Mutex<HashMap<String, ResolutionStrategy>>,
}

impl FederationResolver {
    /// Create new federation resolver
    pub fn new(metadata: FederationMetadata) -> Self {
        Self {
            metadata,
            strategy_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Get or determine resolution strategy for type
    pub fn get_or_determine_strategy(
        &self,
        typename: &str,
    ) -> Result<ResolutionStrategy, String> {
        // Check cache
        {
            let cache = self.strategy_cache.lock()
                .map_err(|e| format!("Lock error: {}", e))?;
            if let Some(strategy) = cache.get(typename) {
                return Ok(strategy.clone());
            }
        }

        // Find type metadata
        let fed_type = self.metadata.types.iter()
            .find(|t| t.name == typename)
            .ok_or_else(|| format!("Type {} not found in federation metadata", typename))?;

        // Determine strategy
        let strategy = if fed_type.is_extends {
            // Extended type - needs external resolution
            // For now, default to HTTP (will be improved in next cycle)
            ResolutionStrategy::Http {
                subgraph_url: "http://localhost:4000".to_string(),
            }
        } else {
            // Owned type - resolve locally
            let key_cols = fed_type.keys.first()
                .map(|k| k.fields.clone())
                .unwrap_or_default();

            ResolutionStrategy::Local {
                view_name: format!("{}_federation_view", typename),
                key_columns: key_cols,
            }
        };

        // Cache the strategy
        {
            let mut cache = self.strategy_cache.lock()
                .map_err(|e| format!("Lock error: {}", e))?;
            cache.insert(typename.to_string(), strategy.clone());
        }

        Ok(strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_representation_from_any() {
        let input = serde_json::json!({
            "__typename": "User",
            "id": "123",
            "email": "test@example.com"
        });

        let rep = EntityRepresentation::from_any(&input).unwrap();
        assert_eq!(rep.typename, "User");
        assert_eq!(rep.all_fields.len(), 3);
    }

    #[test]
    fn test_entity_representation_missing_typename() {
        let input = serde_json::json!({
            "id": "123"
        });

        let result = EntityRepresentation::from_any(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_key_fields() {
        let input = serde_json::json!({
            "__typename": "User",
            "id": "123",
            "email": "test@example.com"
        });

        let mut rep = EntityRepresentation::from_any(&input).unwrap();
        rep.extract_key_fields(&["id".to_string()]);

        assert_eq!(rep.key_fields.len(), 1);
        assert_eq!(rep.key_fields["id"], "123");
    }

    #[test]
    fn test_federation_metadata_default() {
        let meta = FederationMetadata::default();
        assert!(!meta.enabled);
        assert_eq!(meta.version, "v2");
        assert!(meta.types.is_empty());
    }
}
