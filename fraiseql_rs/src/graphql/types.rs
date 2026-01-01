//! GraphQL AST types for query representation.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

/// Parsed GraphQL query in Rust.
#[pyclass]
#[allow(clippy::unsafe_derive_deserialize)] // PyO3 generates unsafe methods for FFI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Operation type: "query" or "mutation"
    #[pyo3(get)]
    pub operation_type: String,

    /// Optional operation name
    #[pyo3(get)]
    pub operation_name: Option<String>,

    /// First field in selection set (root field)
    #[pyo3(get)]
    pub root_field: String,

    /// Field selections in query
    #[pyo3(get)]
    pub selections: Vec<FieldSelection>,

    /// Variable definitions
    #[pyo3(get)]
    pub variables: Vec<VariableDefinition>,

    /// Fragment definitions
    #[pyo3(get)]
    pub fragments: Vec<FragmentDefinition>,

    /// Original query string (for caching key)
    #[pyo3(get)]
    pub source: String,
}

#[pymethods]
impl ParsedQuery {
    /// Get query signature for caching (ignores variables).
    #[must_use]
    pub fn signature(&self) -> String {
        // Used by Phase 8 for query plan caching
        format!("{}::{}", self.operation_type, self.root_field)
    }

    /// Check if query is cacheable (no variables).
    #[must_use]
    pub const fn is_cacheable(&self) -> bool {
        self.variables.is_empty()
    }
}

/// Field selection in GraphQL query.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelection {
    /// GraphQL field name (e.g., "users")
    #[pyo3(get)]
    pub name: String,

    /// Alias if provided (e.g., device: equipment)
    #[pyo3(get)]
    pub alias: Option<String>,

    /// Arguments like where: {...}, limit: 10
    #[pyo3(get)]
    pub arguments: Vec<GraphQLArgument>,

    /// Recursive nested field selections
    #[pyo3(get)]
    pub nested_fields: Vec<FieldSelection>,

    /// Directives: @include, @skip, etc with arguments
    #[pyo3(get)]
    pub directives: Vec<Directive>,
}

/// GraphQL directive (e.g., @requiresRole(role: "admin")).
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    /// Directive name (e.g., "requiresRole")
    #[pyo3(get)]
    pub name: String,

    /// Directive arguments
    #[pyo3(get)]
    pub arguments: Vec<GraphQLArgument>,
}

/// GraphQL argument (e.g., where: {...}).
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLArgument {
    /// Argument name
    #[pyo3(get)]
    pub name: String,

    /// Value type: "object", "variable", or "scalar"
    #[pyo3(get)]
    pub value_type: String,

    /// Serialized value as JSON string
    #[pyo3(get)]
    pub value_json: String,
}

/// GraphQL type representation
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLType {
    /// Type name (e.g., "String", "User")
    #[pyo3(get)]
    pub name: String,
    /// Whether the type is nullable
    #[pyo3(get)]
    pub nullable: bool,
    /// Whether it's a list type
    #[pyo3(get)]
    pub list: bool,
    /// Whether list items are nullable
    #[pyo3(get)]
    pub list_nullable: bool,
}

/// Variable definition.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDefinition {
    /// Variable name without $ prefix
    #[pyo3(get)]
    pub name: String,

    /// Structured type information
    #[pyo3(get)]
    pub var_type: GraphQLType,

    /// Default value as JSON string
    #[pyo3(get)]
    pub default_value: Option<String>,
}

/// Fragment definition.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentDefinition {
    /// Fragment name
    #[pyo3(get)]
    pub name: String,

    /// Type this fragment applies to (e.g., "User")
    #[pyo3(get)]
    pub type_condition: String,

    /// Fields selected in fragment
    #[pyo3(get)]
    pub selections: Vec<FieldSelection>,

    /// Names of other fragments this one spreads
    #[pyo3(get)]
    pub fragment_spreads: Vec<String>,
}

impl Default for ParsedQuery {
    fn default() -> Self {
        Self {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: String::new(),
            selections: Vec::new(),
            variables: Vec::new(),
            fragments: Vec::new(),
            source: String::new(),
        }
    }
}

impl PartialEq for FieldSelection {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.alias == other.alias && self.arguments == other.arguments
    }
}

impl PartialEq for GraphQLArgument {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value_json == other.value_json
    }
}
