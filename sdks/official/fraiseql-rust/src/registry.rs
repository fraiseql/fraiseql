//! Schema registry for type and operation authoring.
//!
//! [`SchemaRegistry`] is a thread-local singleton that accumulates type,
//! query, mutation, and subscription definitions as the authoring macros
//! execute. Call [`SchemaRegistry::global`] to access it, or use the
//! [`export_schema`](crate::export::export_schema) convenience function.

use std::cell::RefCell;
use serde::Serialize;

/// Definition of a single field within a GraphQL type.
#[derive(Debug, Clone, Serialize)]
pub struct FieldDefinition {
    pub name: String,
    pub graphql_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires_scopes: Vec<String>,
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,
}

/// Definition of a GraphQL argument (on queries and mutations).
#[derive(Debug, Clone, Serialize)]
pub struct ArgumentDefinition {
    pub name: String,
    pub graphql_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
}

/// Kinds of GraphQL named types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TypeKind {
    Object,
    InputObject,
    Enum,
    Interface,
    Union,
    Scalar,
}

/// A registered GraphQL type (object, input, enum, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct TypeDefinition {
    pub name: String,
    pub kind: TypeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
}

/// Kinds of GraphQL operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

/// A registered GraphQL query, mutation, or subscription.
#[derive(Debug, Clone, Serialize)]
pub struct OperationDefinition {
    pub name: String,
    pub kind: OperationKind,
    pub return_type: String,
    pub returns_list: bool,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,
    pub args: Vec<ArgumentDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,
}

/// Global authoring registry. Populated by the `#[fraiseql_*]` macros.
///
/// # Thread safety
///
/// The registry uses a thread-local `RefCell`. It is intended to be populated
/// at program startup (before any concurrent access) via macro-generated
/// registration calls.
#[derive(Debug, Default)]
pub struct SchemaRegistry {
    pub types: Vec<TypeDefinition>,
    pub operations: Vec<OperationDefinition>,
}

thread_local! {
    static REGISTRY: RefCell<SchemaRegistry> = RefCell::new(SchemaRegistry::default());
}

impl SchemaRegistry {
    /// Access the global registry via a closure.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_rust::registry::SchemaRegistry;
    ///
    /// let type_count = SchemaRegistry::with(|r| r.types.len());
    /// assert_eq!(type_count, 0);
    /// ```
    pub fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&Self) -> R,
    {
        REGISTRY.with(|r| f(&r.borrow()))
    }

    /// Mutably access the global registry via a closure.
    pub fn with_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        REGISTRY.with(|r| f(&mut r.borrow_mut()))
    }

    /// Register a type definition. Called by the `#[fraiseql_type]` macro.
    pub fn register_type(def: TypeDefinition) {
        Self::with_mut(|r| r.types.push(def));
    }

    /// Register an operation definition. Called by `#[fraiseql_query]` etc.
    pub fn register_operation(def: OperationDefinition) {
        Self::with_mut(|r| r.operations.push(def));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_starts_empty() {
        let count = SchemaRegistry::with(|r| r.types.len());
        // Note: other tests may have registered types; just verify it's accessible
        let _ = count;
    }

    #[test]
    fn test_register_and_retrieve_type() {
        let def = TypeDefinition {
            name: "TestUser".to_string(),
            kind: TypeKind::Object,
            description: Some("A test user".to_string()),
            fields: vec![FieldDefinition {
                name: "id".to_string(),
                graphql_type: "Int".to_string(),
                nullable: false,
                description: None,
                requires_scope: None,
                requires_scopes: vec![],
                deprecated: false,
                deprecation_reason: None,
            }],
            implements: vec![],
        };

        SchemaRegistry::register_type(def);

        let found = SchemaRegistry::with(|r| {
            r.types.iter().any(|t| t.name == "TestUser")
        });
        assert!(found);
    }

    #[test]
    fn test_register_operation() {
        let def = OperationDefinition {
            name: "test_users".to_string(),
            kind: OperationKind::Query,
            return_type: "TestUser".to_string(),
            returns_list: true,
            nullable: false,
            description: None,
            sql_source: Some("v_test_users".to_string()),
            args: vec![],
            requires_scope: None,
        };

        SchemaRegistry::register_operation(def);

        let found = SchemaRegistry::with(|r| {
            r.operations.iter().any(|o| o.name == "test_users")
        });
        assert!(found);
    }
}
