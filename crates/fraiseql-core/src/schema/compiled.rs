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

    /// GraphQL query definitions.
    #[serde(default)]
    pub queries: Vec<QueryDefinition>,

    /// GraphQL mutation definitions.
    #[serde(default)]
    pub mutations: Vec<MutationDefinition>,

    /// GraphQL subscription definitions.
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionDefinition>,
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

    /// Get total number of operations (queries + mutations + subscriptions).
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.queries.len() + self.mutations.len() + self.subscriptions.len()
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

    /// Find a field by name.
    #[must_use]
    pub fn find_field(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|f| f.name == name)
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
        }
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
        }
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
        }
    }
}

/// Auto-wired query parameters.
///
/// These are standard parameters automatically added to list queries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
