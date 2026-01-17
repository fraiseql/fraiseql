//! Compiled schema types - pure Rust, no Python/TypeScript references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no `Py<T>` or foreign object references.
//!
//! # Schema Freeze Invariant
//!
//! After `CompiledSchema::from_json()`, the schema is frozen:
//! - All data is Rust-owned
//! - No Python/TypeScript callbacks
//! - No foreign object references
//! - Safe to use from any Tokio worker thread
//!
//! This enables the Axum server to handle requests without any
//! interaction with Python/TypeScript runtimes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::field_type::{FieldDefinition, FieldType};

/// Complete compiled schema - all type information for serving.
///
/// This is the central type that holds the entire GraphQL schema
/// after compilation from Python/TypeScript decorators.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::CompiledSchema;
///
/// let json = r#"{
///     "types": [],
///     "queries": [],
///     "mutations": [],
///     "subscriptions": []
/// }"#;
///
/// let schema = CompiledSchema::from_json(json).unwrap();
/// assert_eq!(schema.types.len(), 0);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompiledSchema {
    /// GraphQL object type definitions.
    #[serde(default)]
    pub types: Vec<TypeDefinition>,

    /// GraphQL enum type definitions.
    #[serde(default)]
    pub enums: Vec<EnumDefinition>,

    /// GraphQL input object type definitions.
    #[serde(default)]
    pub input_types: Vec<InputObjectDefinition>,

    /// GraphQL interface type definitions.
    #[serde(default)]
    pub interfaces: Vec<InterfaceDefinition>,

    /// GraphQL union type definitions.
    #[serde(default)]
    pub unions: Vec<UnionDefinition>,

    /// GraphQL query definitions.
    #[serde(default)]
    pub queries: Vec<QueryDefinition>,

    /// GraphQL mutation definitions.
    #[serde(default)]
    pub mutations: Vec<MutationDefinition>,

    /// GraphQL subscription definitions.
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionDefinition>,

    /// Custom directive definitions.
    /// These are user-defined directives beyond the built-in @skip, @include, @deprecated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<DirectiveDefinition>,

    /// Fact table metadata (for analytics queries).
    /// Key: table name (e.g., `tf_sales`)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fact_tables: HashMap<String, serde_json::Value>,
}

impl CompiledSchema {
    /// Create empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Deserialize from JSON string.
    ///
    /// This is the primary way to create a schema from Python/TypeScript.
    /// The authoring language compiles to JSON, Rust deserializes and owns it.
    ///
    /// # Errors
    ///
    /// Returns error if JSON is malformed or doesn't match schema structure.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
    /// let schema = CompiledSchema::from_json(json).unwrap();
    /// ```
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string.
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails (should not happen for valid schema).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON string (for debugging/config files).
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Find a type definition by name.
    #[must_use]
    pub fn find_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Find an enum definition by name.
    #[must_use]
    pub fn find_enum(&self, name: &str) -> Option<&EnumDefinition> {
        self.enums.iter().find(|e| e.name == name)
    }

    /// Find an input object definition by name.
    #[must_use]
    pub fn find_input_type(&self, name: &str) -> Option<&InputObjectDefinition> {
        self.input_types.iter().find(|i| i.name == name)
    }

    /// Find an interface definition by name.
    #[must_use]
    pub fn find_interface(&self, name: &str) -> Option<&InterfaceDefinition> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Find all types that implement a given interface.
    #[must_use]
    pub fn find_implementors(&self, interface_name: &str) -> Vec<&TypeDefinition> {
        self.types
            .iter()
            .filter(|t| t.implements.contains(&interface_name.to_string()))
            .collect()
    }

    /// Find a union definition by name.
    #[must_use]
    pub fn find_union(&self, name: &str) -> Option<&UnionDefinition> {
        self.unions.iter().find(|u| u.name == name)
    }

    /// Find a query definition by name.
    #[must_use]
    pub fn find_query(&self, name: &str) -> Option<&QueryDefinition> {
        self.queries.iter().find(|q| q.name == name)
    }

    /// Find a mutation definition by name.
    #[must_use]
    pub fn find_mutation(&self, name: &str) -> Option<&MutationDefinition> {
        self.mutations.iter().find(|m| m.name == name)
    }

    /// Find a subscription definition by name.
    #[must_use]
    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionDefinition> {
        self.subscriptions.iter().find(|s| s.name == name)
    }

    /// Find a custom directive definition by name.
    #[must_use]
    pub fn find_directive(&self, name: &str) -> Option<&DirectiveDefinition> {
        self.directives.iter().find(|d| d.name == name)
    }

    /// Get total number of operations (queries + mutations + subscriptions).
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.queries.len() + self.mutations.len() + self.subscriptions.len()
    }

    /// Register fact table metadata.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Fact table name (e.g., `tf_sales`)
    /// * `metadata` - Serialized `FactTableMetadata`
    pub fn add_fact_table(&mut self, table_name: String, metadata: serde_json::Value) {
        self.fact_tables.insert(table_name, metadata);
    }

    /// Get fact table metadata by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Fact table name
    ///
    /// # Returns
    ///
    /// Fact table metadata if found
    #[must_use]
    pub fn get_fact_table(&self, name: &str) -> Option<&serde_json::Value> {
        self.fact_tables.get(name)
    }

    /// List all fact table names.
    ///
    /// # Returns
    ///
    /// Vector of fact table names
    #[must_use]
    pub fn list_fact_tables(&self) -> Vec<&str> {
        self.fact_tables.keys().map(String::as_str).collect()
    }

    /// Check if schema contains any fact tables.
    #[must_use]
    pub fn has_fact_tables(&self) -> bool {
        !self.fact_tables.is_empty()
    }

    /// Validate the schema for internal consistency.
    ///
    /// Checks:
    /// - All type references resolve to defined types
    /// - No duplicate type/operation names
    /// - Required fields have valid types
    ///
    /// # Errors
    ///
    /// Returns list of validation errors if schema is invalid.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check for duplicate type names
        let mut type_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for type_def in &self.types {
            if !type_names.insert(&type_def.name) {
                errors.push(format!("Duplicate type name: {}", type_def.name));
            }
        }

        // Check for duplicate query names
        let mut query_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for query in &self.queries {
            if !query_names.insert(&query.name) {
                errors.push(format!("Duplicate query name: {}", query.name));
            }
        }

        // Check for duplicate mutation names
        let mut mutation_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for mutation in &self.mutations {
            if !mutation_names.insert(&mutation.name) {
                errors.push(format!("Duplicate mutation name: {}", mutation.name));
            }
        }

        // Check type references in queries
        for query in &self.queries {
            if !type_names.contains(query.return_type.as_str())
                && !is_builtin_type(&query.return_type)
            {
                errors.push(format!(
                    "Query '{}' references undefined type '{}'",
                    query.name, query.return_type
                ));
            }
        }

        // Check type references in mutations
        for mutation in &self.mutations {
            if !type_names.contains(mutation.return_type.as_str())
                && !is_builtin_type(&mutation.return_type)
            {
                errors.push(format!(
                    "Mutation '{}' references undefined type '{}'",
                    mutation.name, mutation.return_type
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Check if a type name is a built-in scalar type.
fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Int"
            | "Float"
            | "Boolean"
            | "ID"
            | "DateTime"
            | "Date"
            | "Time"
            | "JSON"
            | "UUID"
            | "Decimal"
    )
}

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
///     name: "User".to_string(),
///     sql_source: "v_user".to_string(),
///     jsonb_column: "data".to_string(),
///     fields: vec![
///         FieldDefinition::new("id", FieldType::Id),
///         FieldDefinition::new("email", FieldType::String),
///     ],
///     description: Some("A user in the system".to_string()),
///     sql_projection_hint: None,
///     implements: vec![],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDefinition {
    /// GraphQL type name (e.g., "User").
    pub name: String,

    /// SQL source table/view (e.g., `v_user`).
    pub sql_source: String,

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
}

/// SQL projection hint for database-specific field projection optimization.
///
/// When a type has a large JSONB payload, the compiler can generate
/// SQL that projects only the requested fields, reducing network payload
/// and JSON deserialization overhead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SqlProjectionHint {
    /// Database type (e.g., "postgresql", "mysql", "sqlite").
    pub database: String,

    /// The projection SQL template.
    /// Example for PostgreSQL:
    /// `jsonb_build_object('id', data->>'id', 'email', data->>'email')`
    pub projection_template: String,

    /// Estimated reduction in payload size (percentage 0-100).
    pub estimated_reduction_percent: u32,
}

fn default_jsonb_column() -> String {
    "data".to_string()
}

impl TypeDefinition {
    /// Create a new type definition.
    #[must_use]
    pub fn new(name: impl Into<String>, sql_source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sql_source: sql_source.into(),
            jsonb_column: "data".to_string(),
            fields: Vec::new(),
            description: None,
            sql_projection_hint: None,
            implements: Vec::new(),
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
    pub fn has_sql_projection(&self) -> bool {
        self.sql_projection_hint.is_some()
    }

    /// Get the `__typename` value for this type.
    ///
    /// Returns the GraphQL type name, used for type introspection in responses.
    /// Per GraphQL spec §2.7, `__typename` returns the name of the object type.
    #[must_use]
    pub fn typename(&self) -> &str {
        &self.name
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            name: name.into(),
            values: Vec::new(),
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumValueDefinition {
    /// Value name (e.g., "PENDING").
    pub name: String,

    /// Description of this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation information (if this value is deprecated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl EnumValueDefinition {
    /// Create a new enum value.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
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
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this value is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
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
}

impl InputObjectDefinition {
    /// Create a new input object definition.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
            description: None,
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

    /// Field type (e.g., "String!", "[Int]", "UserFilter").
    pub field_type: String,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value (as JSON string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,

    /// Deprecation information (if this field is deprecated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl InputFieldDefinition {
    /// Create a new input field.
    #[must_use]
    pub fn new(name: impl Into<String>, field_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: field_type.into(),
            description: None,
            default_value: None,
            deprecation: None,
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
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this field is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Check if this field is required (non-nullable without default).
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.field_type.ends_with('!') && self.default_value.is_none()
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
            name: name.into(),
            fields: Vec::new(),
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            name: name.into(),
            member_types: Vec::new(),
            description: None,
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

/// A query definition compiled from `@fraiseql.query`.
///
/// Queries are declarative bindings to database views/tables.
/// They describe *what* to fetch, not *how* to fetch it.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{QueryDefinition, AutoParams};
///
/// let query = QueryDefinition {
///     name: "users".to_string(),
///     return_type: "User".to_string(),
///     returns_list: true,
///     nullable: false,
///     arguments: vec![],
///     sql_source: Some("v_user".to_string()),
///     description: Some("Get all users".to_string()),
///     auto_params: AutoParams::default(),
///     deprecation: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryDefinition {
    /// Query name (e.g., "users").
    pub name: String,

    /// Return type name (e.g., "User").
    pub return_type: String,

    /// Does this query return a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Is the return value nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// SQL source table/view (for direct table queries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Auto-wired parameters (where, orderBy, limit, offset).
    #[serde(default)]
    pub auto_params: AutoParams,

    /// Deprecation information (from @deprecated directive).
    /// When set, this query is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl QueryDefinition {
    /// Create a new query definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            return_type: return_type.into(),
            returns_list: false,
            nullable: false,
            arguments: Vec::new(),
            sql_source: None,
            description: None,
            auto_params: AutoParams::default(),
            deprecation: None,
        }
    }

    /// Set this query to return a list.
    #[must_use]
    pub fn returning_list(mut self) -> Self {
        self.returns_list = true;
        self
    }

    /// Set the SQL source.
    #[must_use]
    pub fn with_sql_source(mut self, source: impl Into<String>) -> Self {
        self.sql_source = Some(source.into());
        self
    }

    /// Mark this query as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::QueryDefinition;
    ///
    /// let query = QueryDefinition::new("oldUsers", "User")
    ///     .deprecated(Some("Use 'users' instead".to_string()));
    /// assert!(query.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this query is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// A mutation definition compiled from `@fraiseql.mutation`.
///
/// Mutations are declarative bindings to database functions.
/// They describe *which function* to call, not arbitrary logic.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{MutationDefinition, MutationOperation};
///
/// let mutation = MutationDefinition {
///     name: "createUser".to_string(),
///     return_type: "User".to_string(),
///     arguments: vec![],
///     description: Some("Create a new user".to_string()),
///     operation: MutationOperation::Insert { table: "users".to_string() },
///     deprecation: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationDefinition {
    /// Mutation name (e.g., "createUser").
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Input arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL operation type.
    #[serde(default)]
    pub operation: MutationOperation,

    /// Deprecation information (from @deprecated directive).
    /// When set, this mutation is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl MutationDefinition {
    /// Create a new mutation definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            return_type: return_type.into(),
            arguments: Vec::new(),
            description: None,
            operation: MutationOperation::default(),
            deprecation: None,
        }
    }

    /// Mark this mutation as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::MutationDefinition;
    ///
    /// let mutation = MutationDefinition::new("oldCreateUser", "User")
    ///     .deprecated(Some("Use 'createUser' instead".to_string()));
    /// assert!(mutation.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this mutation is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Mutation operation types.
///
/// This enum describes what kind of database operation a mutation performs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MutationOperation {
    /// INSERT into a table.
    Insert {
        /// Target table name.
        table: String,
    },

    /// UPDATE a table.
    Update {
        /// Target table name.
        table: String,
    },

    /// DELETE from a table.
    Delete {
        /// Target table name.
        table: String,
    },

    /// Call a database function.
    Function {
        /// Function name.
        name: String,
    },

    /// Custom mutation (for complex operations).
    #[default]
    Custom,
}

/// A subscription definition.
///
/// Subscriptions are declarative bindings to event topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionDefinition {
    /// Subscription name.
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Event topic to subscribe to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, this subscription is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl SubscriptionDefinition {
    /// Create a new subscription definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            return_type: return_type.into(),
            arguments: Vec::new(),
            description: None,
            topic: None,
            deprecation: None,
        }
    }

    /// Set the event topic for this subscription.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::SubscriptionDefinition;
    ///
    /// let subscription = SubscriptionDefinition::new("orderCreated", "Order")
    ///     .with_topic("order_created");
    /// assert_eq!(subscription.topic, Some("order_created".to_string()));
    /// ```
    #[must_use]
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set the description for this subscription.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to this subscription.
    #[must_use]
    pub fn with_argument(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Mark this subscription as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::SubscriptionDefinition;
    ///
    /// let subscription = SubscriptionDefinition::new("oldUserEvents", "User")
    ///     .deprecated(Some("Use 'userEvents' instead".to_string()));
    /// assert!(subscription.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this subscription is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Query/mutation/subscription argument definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArgumentDefinition {
    /// Argument name.
    pub name: String,

    /// Argument type.
    pub arg_type: FieldType,

    /// Is this argument optional?
    #[serde(default)]
    pub nullable: bool,

    /// Default value (JSON representation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, this argument is marked as deprecated in the schema.
    /// Per GraphQL spec, deprecated arguments should still be accepted but
    /// clients are encouraged to migrate to alternatives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

impl ArgumentDefinition {
    /// Create a new required argument.
    #[must_use]
    pub fn new(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: false,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Create a new optional argument.
    #[must_use]
    pub fn optional(name: impl Into<String>, arg_type: FieldType) -> Self {
        Self {
            name: name.into(),
            arg_type,
            nullable: true,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    /// Mark this argument as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{ArgumentDefinition, FieldType};
    ///
    /// let arg = ArgumentDefinition::optional("oldLimit", FieldType::Int)
    ///     .deprecated(Some("Use 'first' instead".to_string()));
    /// assert!(arg.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this argument is deprecated.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}

/// Auto-wired query parameters.
///
/// These are standard parameters automatically added to list queries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // These are intentional feature flags
pub struct AutoParams {
    /// Enable `where` filtering.
    #[serde(default)]
    pub has_where: bool,

    /// Enable `orderBy` sorting.
    #[serde(default)]
    pub has_order_by: bool,

    /// Enable `limit` pagination.
    #[serde(default)]
    pub has_limit: bool,

    /// Enable `offset` pagination.
    #[serde(default)]
    pub has_offset: bool,
}

impl AutoParams {
    /// Create with all auto-params enabled (common for list queries).
    #[must_use]
    pub fn all() -> Self {
        Self {
            has_where: true,
            has_order_by: true,
            has_limit: true,
            has_offset: true,
        }
    }

    /// Create with no auto-params (common for single-item queries).
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

// =============================================================================
// Custom Directive Definitions
// =============================================================================

/// A custom directive definition for schema extension.
///
/// Allows defining custom directives beyond the built-in `@skip`, `@include`,
/// and `@deprecated` directives. Custom directives are exposed via introspection
/// and can be evaluated at runtime via registered handlers.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{DirectiveDefinition, DirectiveLocationKind, ArgumentDefinition, FieldType};
///
/// let rate_limit = DirectiveDefinition {
///     name: "rateLimit".to_string(),
///     description: Some("Apply rate limiting to this field".to_string()),
///     locations: vec![DirectiveLocationKind::FieldDefinition],
///     arguments: vec![
///         ArgumentDefinition::new("limit", FieldType::Int),
///         ArgumentDefinition::optional("window", FieldType::String),
///     ],
///     is_repeatable: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectiveDefinition {
    /// Directive name (e.g., "rateLimit", "auth").
    pub name: String,

    /// Description of what this directive does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Valid locations where this directive can be applied.
    pub locations: Vec<DirectiveLocationKind>,

    /// Arguments this directive accepts.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Whether this directive can be applied multiple times to the same location.
    #[serde(default)]
    pub is_repeatable: bool,
}

impl DirectiveDefinition {
    /// Create a new directive definition.
    #[must_use]
    pub fn new(name: impl Into<String>, locations: Vec<DirectiveLocationKind>) -> Self {
        Self {
            name: name.into(),
            description: None,
            locations,
            arguments: Vec::new(),
            is_repeatable: false,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to this directive.
    #[must_use]
    pub fn with_argument(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Add multiple arguments to this directive.
    #[must_use]
    pub fn with_arguments(mut self, args: Vec<ArgumentDefinition>) -> Self {
        self.arguments = args;
        self
    }

    /// Mark this directive as repeatable.
    #[must_use]
    pub fn repeatable(mut self) -> Self {
        self.is_repeatable = true;
        self
    }

    /// Check if this directive can be applied at the given location.
    #[must_use]
    pub fn valid_at(&self, location: DirectiveLocationKind) -> bool {
        self.locations.contains(&location)
    }

    /// Find an argument by name.
    #[must_use]
    pub fn find_argument(&self, name: &str) -> Option<&ArgumentDefinition> {
        self.arguments.iter().find(|a| a.name == name)
    }
}

/// Directive location kinds for custom directive definitions.
///
/// This mirrors `DirectiveLocation` in introspection but is used for
/// compiled schema definitions. The two types can be converted between
/// each other for introspection purposes.
///
/// Per GraphQL spec §3.13, directive locations fall into two categories:
/// - Executable locations (operations, fields, fragments)
/// - Type system locations (schema definitions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectiveLocationKind {
    // Executable directive locations
    /// Directive on query operation.
    Query,
    /// Directive on mutation operation.
    Mutation,
    /// Directive on subscription operation.
    Subscription,
    /// Directive on field selection.
    Field,
    /// Directive on fragment definition.
    FragmentDefinition,
    /// Directive on fragment spread.
    FragmentSpread,
    /// Directive on inline fragment.
    InlineFragment,
    /// Directive on variable definition.
    VariableDefinition,

    // Type system directive locations
    /// Directive on schema definition.
    Schema,
    /// Directive on scalar type definition.
    Scalar,
    /// Directive on object type definition.
    Object,
    /// Directive on field definition.
    FieldDefinition,
    /// Directive on argument definition.
    ArgumentDefinition,
    /// Directive on interface definition.
    Interface,
    /// Directive on union definition.
    Union,
    /// Directive on enum definition.
    Enum,
    /// Directive on enum value definition.
    EnumValue,
    /// Directive on input object definition.
    InputObject,
    /// Directive on input field definition.
    InputFieldDefinition,
}

impl DirectiveLocationKind {
    /// Check if this is an executable directive location.
    #[must_use]
    pub fn is_executable(&self) -> bool {
        matches!(
            self,
            Self::Query
                | Self::Mutation
                | Self::Subscription
                | Self::Field
                | Self::FragmentDefinition
                | Self::FragmentSpread
                | Self::InlineFragment
                | Self::VariableDefinition
        )
    }

    /// Check if this is a type system directive location.
    #[must_use]
    pub fn is_type_system(&self) -> bool {
        !self.is_executable()
    }
}
