//! GraphQL named type definitions: object types, enums, input objects, interfaces, and unions.

use serde::{Deserialize, Serialize};

use super::config_types::RelationshipDef;
use super::{
    domain_types::{SqlSource, TypeName},
    field_type::{DeprecationInfo, FieldDefinition},
};
pub use crate::types::SqlProjectionHint;
use crate::validation::ValidationRule;

// =============================================================================
// Object Type Definitions
// =============================================================================

/// A GraphQL type definition compiled from `@fraiseql.type`.
///
/// This represents a complete object type with its fields and database binding.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{TypeDefinition, FieldDefinition, FieldType};
///
/// let user_type = TypeDefinition {
///     name: "User".into(),
///     sql_source: "v_user".into(),
///     jsonb_column: "data".to_string(),
///     fields: vec![
///         FieldDefinition::new("id", FieldType::Id),
///         FieldDefinition::new("email", FieldType::String),
///     ],
///     description: Some("A user in the system".to_string()),
///     sql_projection_hint: None,
///     implements: vec![],
///     requires_role: None,
///     is_error: false,
///     relay: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDefinition {
    /// GraphQL type name (e.g., "User").
    pub name: TypeName,

    /// SQL source table/view (e.g., `v_user`).
    pub sql_source: SqlSource,

    /// JSONB column name (e.g., "data").
    #[serde(default = "default_jsonb_column")]
    pub jsonb_column: String,

    /// Field definitions.
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,

    /// Optional description (from docstring).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL projection hint for PostgreSQL optimization.
    /// Generated at compile time to reduce payload size for large JSONB objects.
    /// Example: `jsonb_build_object('id', data->>'id', 'email', data->>'email')`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_projection_hint: Option<SqlProjectionHint>,

    /// Interfaces this type implements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,

    /// Role required to see this type in introspection and access its queries.
    ///
    /// When set, only users with this role can see the type in the GraphQL schema
    /// and execute queries returning this type. Users without the role see neither
    /// the type nor its associated queries — preventing role enumeration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Whether this type is a mutation error type (tagged with `@fraiseql.error`).
    ///
    /// Error types are populated from `mutation_response.metadata` JSONB rather than
    /// the `entity` field.  Both scalar primitives and nested objects are supported.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,

    /// Whether this type implements the Relay Node interface.
    ///
    /// When `true`, the compiler auto-generates:
    /// - `implements: ["Node"]` in the type definition
    /// - A global `node(id: ID!)` query entry routing to this type
    /// - `XxxConnection` and `XxxEdge` wrapper types
    ///
    /// The global `id` field is the public UUID (`id` column).
    /// Cursor-based pagination uses `pk_{snake_case(name)}` (BIGINT) for keyset ordering.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,

    /// Relationships to other types (derived from FK conventions or explicit annotation).
    ///
    /// Used by the REST transport for nested resource embedding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<RelationshipDef>,
}

pub(super) fn default_jsonb_column() -> String {
    "data".to_string()
}

impl TypeDefinition {
    /// Create a new type definition.
    #[must_use]
    pub fn new(name: impl Into<String>, sql_source: impl Into<String>) -> Self {
        Self {
            name:                TypeName::new(name),
            sql_source:          SqlSource::new(sql_source),
            jsonb_column:        "data".to_string(),
            fields:              Vec::new(),
            description:         None,
            sql_projection_hint: None,
            implements:          Vec::new(),
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       Vec::new(),
        }
    }

    /// Add a field to this type.
    #[must_use]
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Set the JSONB column name.
    #[must_use]
    pub fn with_jsonb_column(mut self, column: impl Into<String>) -> Self {
        self.jsonb_column = column.into();
        self
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Find a field by name (JSONB key).
    #[must_use]
    pub fn find_field(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Find a field by its output name (alias if set, otherwise name).
    ///
    /// Useful for resolving field references in GraphQL queries where
    /// aliases may be used.
    #[must_use]
    pub fn find_field_by_output_name(&self, output_name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|f| f.output_name() == output_name)
    }

    /// Set SQL projection hint for optimization.
    #[must_use]
    pub fn with_sql_projection(mut self, hint: SqlProjectionHint) -> Self {
        self.sql_projection_hint = Some(hint);
        self
    }

    /// Check if type has SQL projection hint.
    #[must_use]
    pub const fn has_sql_projection(&self) -> bool {
        self.sql_projection_hint.is_some()
    }

    /// Get the `__typename` value for this type.
    ///
    /// Returns the GraphQL type name, used for type introspection in responses.
    /// Per GraphQL spec §2.7, `__typename` returns the name of the object type.
    #[must_use]
    pub fn typename(&self) -> &str {
        self.name.as_str()
    }

    /// Returns fields that are writable via mutations.
    ///
    /// Excludes primary keys, auto-generated, computed, and encrypted fields.
    /// Used by the REST transport for PUT/PATCH full-coverage detection and
    /// OpenAPI schema generation.
    #[must_use]
    pub fn writable_fields(&self) -> Vec<&FieldDefinition> {
        self.fields.iter().filter(|f| f.is_writable()).collect()
    }
}

// =============================================================================
// Enum Definitions
// =============================================================================

/// A GraphQL enum type definition.
///
/// Enums represent a finite set of possible values, useful for
/// categorization fields like status, role, or priority.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{EnumDefinition, EnumValueDefinition};
///
/// let status_enum = EnumDefinition {
///     name: "OrderStatus".to_string(),
///     values: vec![
///         EnumValueDefinition::new("PENDING"),
///         EnumValueDefinition::new("PROCESSING"),
///         EnumValueDefinition::new("SHIPPED"),
///         EnumValueDefinition::new("DELIVERED"),
///     ],
///     description: Some("Possible states of an order".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDefinition {
    /// Enum type name (e.g., "OrderStatus").
    pub name: String,

    /// Possible values for this enum.
    #[serde(default)]
    pub values: Vec<EnumValueDefinition>,

    /// Description of the enum type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl EnumDefinition {
    /// Create a new enum definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:        name.into(),
            values:      Vec::new(),
            description: None,
        }
    }

    /// Add a value to this enum.
    #[must_use]
    pub fn with_value(mut self, value: EnumValueDefinition) -> Self {
        self.values.push(value);
        self
    }

    /// Add multiple values to this enum.
    #[must_use]
    pub fn with_values(mut self, values: Vec<EnumValueDefinition>) -> Self {
        self.values = values;
        self
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Check if a value exists in this enum.
    #[must_use]
    pub fn has_value(&self, name: &str) -> bool {
        self.values.iter().any(|v| v.name == name)
    }

    /// Find a value by name.
    #[must_use]
    pub fn find_value(&self, name: &str) -> Option<&EnumValueDefinition> {
        self.values.iter().find(|v| v.name == name)
    }
}

/// A single value within a GraphQL enum type.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::EnumValueDefinition;
///
/// let value = EnumValueDefinition::new("ACTIVE")
///     .with_description("The item is currently active");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumValueDefinition {
    /// Value name (e.g., "PENDING").
    pub name: String,

    /// Description of this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation information (if this value is deprecated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,
}

impl EnumValueDefinition {
    /// Create a new enum value.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:        name.into(),
            description: None,
            deprecation: None,
        }
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark this value as deprecated.
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this value is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }
}

// =============================================================================
// Input Object Definitions
// =============================================================================

/// A GraphQL input object type definition.
///
/// Input objects are used for complex query arguments like filters,
/// ordering, and mutation inputs.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{InputObjectDefinition, InputFieldDefinition};
///
/// let user_filter = InputObjectDefinition {
///     name: "UserFilter".to_string(),
///     fields: vec![
///         InputFieldDefinition::new("name", "String"),
///         InputFieldDefinition::new("email", "String"),
///         InputFieldDefinition::new("active", "Boolean"),
///     ],
///     description: Some("Filter criteria for users".to_string()),
///     metadata: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputObjectDefinition {
    /// Input object type name (e.g., "UserFilter").
    pub name: String,

    /// Input fields.
    #[serde(default)]
    pub fields: Vec<InputFieldDefinition>,

    /// Description of the input type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional metadata for specialized input types (e.g., SQL templates for rich filters).
    /// Used by the compiler and runtime for code generation and query execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl InputObjectDefinition {
    /// Create a new input object definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:        name.into(),
            fields:      Vec::new(),
            description: None,
            metadata:    None,
        }
    }

    /// Add a field to this input object.
    #[must_use]
    pub fn with_field(mut self, field: InputFieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Add multiple fields to this input object.
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<InputFieldDefinition>) -> Self {
        self.fields = fields;
        self
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set metadata (for specialized input types like rich filters).
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Find a field by name.
    #[must_use]
    pub fn find_field(&self, name: &str) -> Option<&InputFieldDefinition> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// A field within a GraphQL input object type.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::InputFieldDefinition;
///
/// let field = InputFieldDefinition::new("email", "String!")
///     .with_description("User's email address")
///     .with_default_value("\"user@example.com\"");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputFieldDefinition {
    /// Field name.
    pub name: String,

    /// Field type (e.g., `"String!"`, `"[Int]"`, `"UserFilter"`).
    pub field_type: String,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value (as JSON string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,

    /// Deprecation information (if this field is deprecated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,

    /// Validation rules applied to this field (from @validate directives).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_rules: Vec<ValidationRule>,
}

impl InputFieldDefinition {
    /// Create a new input field.
    #[must_use]
    pub fn new(name: impl Into<String>, field_type: impl Into<String>) -> Self {
        Self {
            name:             name.into(),
            field_type:       field_type.into(),
            description:      None,
            default_value:    None,
            deprecation:      None,
            validation_rules: Vec::new(),
        }
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set default value.
    #[must_use]
    pub fn with_default_value(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }

    /// Mark this field as deprecated.
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this field is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Check if this field is required (non-nullable without default).
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.field_type.ends_with('!') && self.default_value.is_none()
    }

    /// Add a validation rule to this field.
    #[must_use]
    pub fn with_validation_rule(mut self, rule: ValidationRule) -> Self {
        self.validation_rules.push(rule);
        self
    }

    /// Add multiple validation rules to this field.
    #[must_use]
    pub fn with_validation_rules(mut self, rules: Vec<ValidationRule>) -> Self {
        self.validation_rules.extend(rules);
        self
    }

    /// Check if this field has validation rules.
    #[must_use]
    pub const fn has_validation_rules(&self) -> bool {
        !self.validation_rules.is_empty()
    }
}

// =============================================================================
// Interface Definitions
// =============================================================================

/// A GraphQL interface type definition.
///
/// Interfaces define a common set of fields that multiple types can implement.
/// They enable polymorphic queries where a field can return any type that
/// implements the interface.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{InterfaceDefinition, FieldDefinition, FieldType};
///
/// let node_interface = InterfaceDefinition {
///     name: "Node".to_string(),
///     fields: vec![
///         FieldDefinition::new("id", FieldType::Id),
///     ],
///     description: Some("An object with an ID".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceDefinition {
    /// Interface name (e.g., "Node").
    pub name: String,

    /// Fields that implementing types must define.
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,

    /// Description of the interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl InterfaceDefinition {
    /// Create a new interface definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:        name.into(),
            fields:      Vec::new(),
            description: None,
        }
    }

    /// Add a field to this interface.
    #[must_use]
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Add multiple fields to this interface.
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<FieldDefinition>) -> Self {
        self.fields = fields;
        self
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Find a field by name.
    #[must_use]
    pub fn find_field(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|f| f.name == name)
    }
}

// =============================================================================
// Union Definitions
// =============================================================================

/// A GraphQL union type definition.
///
/// Unions represent a type that can be one of several possible object types.
/// Unlike interfaces, union member types don't need to share any fields.
/// Per GraphQL spec §3.8, unions are useful for polymorphic returns.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::UnionDefinition;
///
/// let search_result = UnionDefinition {
///     name: "SearchResult".to_string(),
///     member_types: vec!["User".to_string(), "Post".to_string(), "Comment".to_string()],
///     description: Some("Possible search result types".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnionDefinition {
    /// Union name (e.g., "SearchResult").
    pub name: String,

    /// Member types that this union can represent.
    /// Order is significant for resolution.
    pub member_types: Vec<String>,

    /// Description of the union.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl UnionDefinition {
    /// Create a new union definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:         name.into(),
            member_types: Vec::new(),
            description:  None,
        }
    }

    /// Add a member type to this union.
    #[must_use]
    pub fn with_member(mut self, type_name: impl Into<String>) -> Self {
        self.member_types.push(type_name.into());
        self
    }

    /// Add multiple member types to this union.
    #[must_use]
    pub fn with_members(mut self, members: Vec<String>) -> Self {
        self.member_types = members;
        self
    }

    /// Set description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Check if a type is a member of this union.
    #[must_use]
    pub fn contains_type(&self, type_name: &str) -> bool {
        self.member_types.iter().any(|t| t == type_name)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use crate::schema::{FieldEncryptionConfig, FieldType};

    #[test]
    fn test_writable_fields_excludes_pk() {
        let type_def = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("pk_user", FieldType::Int))
            .with_field(FieldDefinition::new("email", FieldType::String));

        let writable: Vec<&str> = type_def.writable_fields().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(writable, vec!["email"]);
    }

    #[test]
    fn test_writable_fields_excludes_auto_generated() {
        let mut created = FieldDefinition::new("created_at", FieldType::DateTime);
        created.auto_generated = true;

        let type_def = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(created);

        let writable: Vec<&str> = type_def.writable_fields().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(writable, vec!["email"]);
    }

    #[test]
    fn test_writable_fields_excludes_computed() {
        let mut full_name = FieldDefinition::new("full_name", FieldType::String);
        full_name.computed = true;

        let type_def = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(full_name);

        let writable: Vec<&str> = type_def.writable_fields().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(writable, vec!["email"]);
    }

    #[test]
    fn test_writable_fields_excludes_encrypted() {
        let ssn = FieldDefinition::new("ssn", FieldType::String).with_encryption(
            FieldEncryptionConfig {
                key_reference: "keys/ssn".to_string(),
                algorithm:     "AES-256-GCM".to_string(),
            },
        );

        let type_def = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(ssn);

        let writable: Vec<&str> = type_def.writable_fields().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(writable, vec!["email"]);
    }

    #[test]
    fn test_writable_fields_mixed() {
        let mut auto = FieldDefinition::new("created_at", FieldType::DateTime);
        auto.auto_generated = true;
        let mut computed = FieldDefinition::new("full_name", FieldType::String);
        computed.computed = true;

        let type_def = TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("pk_user", FieldType::Int))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(auto)
            .with_field(computed);

        let writable: Vec<&str> = type_def.writable_fields().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(writable, vec!["email", "name"]);
    }
}
