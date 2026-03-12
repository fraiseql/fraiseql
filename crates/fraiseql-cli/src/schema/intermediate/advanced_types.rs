//! Interfaces, unions, and inputs: `IntermediateInterface`, `IntermediateUnion`,
//! `IntermediateInputObject`, `IntermediateInputField`.

use serde::{Deserialize, Serialize};

use super::types::{IntermediateDeprecation, IntermediateField};

// =============================================================================
// Interface Definitions (GraphQL Spec §3.7)
// =============================================================================

/// GraphQL interface type definition in intermediate format.
///
/// Interfaces define a common set of fields that multiple object types can implement.
/// Per GraphQL spec §3.7, interfaces enable polymorphic queries.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "Node",
///   "fields": [
///     {"name": "id", "type": "ID", "nullable": false}
///   ],
///   "description": "An object with a globally unique ID"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInterface {
    /// Interface name (e.g., "Node")
    pub name: String,

    /// Interface fields (all implementing types must have these fields)
    pub fields: Vec<IntermediateField>,

    /// Interface description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// =============================================================================
// Union Definitions (GraphQL Spec §3.10)
// =============================================================================

/// GraphQL union type definition in intermediate format.
///
/// Unions represent a type that could be one of several object types.
/// Per GraphQL spec §3.10, unions are abstract types with member types.
/// Unlike interfaces, unions don't define common fields.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "SearchResult",
///   "member_types": ["User", "Post", "Comment"],
///   "description": "A result from a search query"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateUnion {
    /// Union type name (e.g., "SearchResult")
    pub name: String,

    /// Member types (object type names that belong to this union)
    pub member_types: Vec<String>,

    /// Union description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// =============================================================================
// Input Object Definitions
// =============================================================================

/// GraphQL input object type definition in intermediate format.
///
/// Input objects are used for complex query arguments like filters,
/// ordering, and mutation inputs.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "UserFilter",
///   "fields": [
///     {"name": "name", "type": "String", "nullable": true},
///     {"name": "email", "type": "String", "nullable": true},
///     {"name": "active", "type": "Boolean", "nullable": true, "default": true}
///   ],
///   "description": "Filter criteria for users"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInputObject {
    /// Input object type name (e.g., "UserFilter")
    pub name: String,

    /// Input fields
    pub fields: Vec<IntermediateInputField>,

    /// Input type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A field within an input object type.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "email",
///   "type": "String!",
///   "description": "User's email address",
///   "default": "user@example.com"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInputField {
    /// Field name
    pub name: String,

    /// Field type name (e.g., `"String!"`, `"[Int]"`, `"UserFilter"`)
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Field description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value (as JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Deprecation info (if field is deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}
