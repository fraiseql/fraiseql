//! GraphQL AST types for query representation.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

/// Parsed GraphQL query in Rust.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    #[pyo3(get)]
    pub operation_type: String, // "query" | "mutation"

    #[pyo3(get)]
    pub operation_name: Option<String>,

    #[pyo3(get)]
    pub root_field: String, // First field in selection set

    #[pyo3(get)]
    pub selections: Vec<FieldSelection>,

    #[pyo3(get)]
    pub variables: Vec<VariableDefinition>,

    #[pyo3(get)]
    pub source: String, // Original query string (for caching key)
}

#[pymethods]
impl ParsedQuery {
    /// Get query signature for caching (ignores variables).
    pub fn signature(&self) -> String {
        // Used by Phase 8 for query plan caching
        format!("{}::{}", self.operation_type, self.root_field)
    }

    /// Check if query is cacheable (no variables).
    pub fn is_cacheable(&self) -> bool {
        self.variables.is_empty()
    }
}

/// Field selection in GraphQL query.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelection {
    #[pyo3(get)]
    pub name: String, // GraphQL field name (e.g., "users")

    #[pyo3(get)]
    pub alias: Option<String>, // Alias if provided (e.g., device: equipment)

    #[pyo3(get)]
    pub arguments: Vec<GraphQLArgument>, // Args like where: {...}, limit: 10

    #[pyo3(get)]
    pub nested_fields: Vec<FieldSelection>, // Recursive nested selections

    #[pyo3(get)]
    pub directives: Vec<String>, // @include, @skip, etc
}

/// GraphQL argument (e.g., where: {...}).
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLArgument {
    #[pyo3(get)]
    pub name: String, // Argument name

    #[pyo3(get)]
    pub value_type: String, // "object" | "variable" | "scalar"

    #[pyo3(get)]
    pub value_json: String, // Serialized value (JSON)
}

/// Variable definition.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDefinition {
    #[pyo3(get)]
    pub name: String, // Variable name without $

    #[pyo3(get)]
    pub var_type: String, // Type string (e.g., "UserWhere!")

    #[pyo3(get)]
    pub default_value: Option<String>, // Default value as JSON
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
