//! Fragment/directive structs: `IntermediateFragment`, `IntermediateFragmentField`,
//! `IntermediateFragmentFieldDef`, `IntermediateDirective`, `IntermediateAppliedDirective`.

use serde::{Deserialize, Serialize};

use super::operations::IntermediateArgument;

// =============================================================================
// Fragment and Directive Definitions (GraphQL Spec §2.9-2.12)
// =============================================================================

/// Fragment definition in intermediate format.
///
/// Fragments are reusable field selections that can be spread into queries.
/// Per GraphQL spec §2.9-2.10, fragments have a type condition and field list.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "UserFields",
///   "on": "User",
///   "fields": ["id", "name", "email"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFragment {
    /// Fragment name (e.g., "UserFields")
    pub name: String,

    /// Type condition - the type this fragment applies to (e.g., "User")
    #[serde(rename = "on")]
    pub type_condition: String,

    /// Fields to select (can be field names or nested fragment spreads)
    pub fields: Vec<IntermediateFragmentField>,

    /// Fragment description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Fragment field selection - either a simple field or a nested object/fragment spread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum IntermediateFragmentField {
    /// Simple field name (e.g., "id", "name")
    Simple(String),

    /// Complex field with nested selections or directives
    Complex(IntermediateFragmentFieldDef),
}

/// Complex fragment field definition with optional alias, directives, and nested fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFragmentFieldDef {
    /// Field name (source field in the type)
    pub name: String,

    /// Output alias (optional, per GraphQL spec §2.13)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Nested field selections (for object fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<IntermediateFragmentField>>,

    /// Fragment spread (e.g., "...UserFields")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread: Option<String>,

    /// Applied directives (e.g., @skip, @include)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateAppliedDirective>>,
}

/// Directive definition in intermediate format.
///
/// Directives provide a way to describe alternate runtime execution and type validation.
/// Per GraphQL spec §2.12, directives can be applied to various locations.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "auth",
///   "locations": ["FIELD_DEFINITION", "OBJECT"],
///   "arguments": [{"name": "role", "type": "String", "nullable": false}],
///   "description": "Requires authentication with specified role"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDirective {
    /// Directive name (without @, e.g., "auth", "deprecated")
    pub name: String,

    /// Valid locations where this directive can be applied
    pub locations: Vec<String>,

    /// Directive arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Whether the directive can be applied multiple times
    #[serde(default)]
    pub repeatable: bool,

    /// Directive description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// An applied directive instance (used on fields, types, etc.).
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "skip",
///   "arguments": {"if": true}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAppliedDirective {
    /// Directive name (without @)
    pub name: String,

    /// Directive arguments as key-value pairs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}
