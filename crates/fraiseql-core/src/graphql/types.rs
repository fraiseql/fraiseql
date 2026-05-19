//! GraphQL AST types for query representation.
//!
//! These types represent parsed GraphQL queries in a Rust-native format.
//! They are produced by the parser and consumed by fragment resolution
//! and directive evaluation.

use serde::{Deserialize, Serialize};

/// Parsed GraphQL query.
///
/// Contains all information extracted from a GraphQL query string,
/// including operation details, selections, variables, and fragments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Operation type: "query", "mutation", or "subscription"
    pub operation_type: String,

    /// Optional operation name (e.g., "`GetUsers`")
    pub operation_name: Option<String>,

    /// First field in selection set (root field for single-root queries)
    pub root_field: String,

    /// Field selections in query
    pub selections: Vec<FieldSelection>,

    /// Variable definitions
    pub variables: Vec<VariableDefinition>,

    /// Fragment definitions
    pub fragments: Vec<FragmentDefinition>,

    /// Original query string (for caching key)
    pub source: String,
}

impl ParsedQuery {
    /// Get query signature for caching (ignores variables).
    #[must_use]
    pub fn signature(&self) -> String {
        format!("{}::{}", self.operation_type, self.root_field)
    }

    /// Check if query is cacheable (no variables).
    #[must_use]
    pub const fn is_cacheable(&self) -> bool {
        self.variables.is_empty()
    }
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

/// Field selection in GraphQL query.
///
/// Represents a single field selection with optional alias, arguments,
/// nested selections, and directives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelection {
    /// GraphQL field name (e.g., "users")
    pub name: String,

    /// Alias if provided (e.g., `device: equipment`)
    pub alias: Option<String>,

    /// Arguments like `where: {...}, limit: 10`
    pub arguments: Vec<GraphQLArgument>,

    /// Recursive nested field selections
    pub nested_fields: Vec<FieldSelection>,

    /// Directives: @include, @skip, etc. with arguments
    pub directives: Vec<Directive>,
}

impl FieldSelection {
    /// Get the response key for this field (alias if present, otherwise name).
    #[must_use]
    pub fn response_key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

impl PartialEq for FieldSelection {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.alias == other.alias && self.arguments == other.arguments
    }
}

/// GraphQL directive (e.g., `@skip(if: true)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    /// Directive name (e.g., "skip", "include")
    pub name: String,

    /// Directive arguments
    pub arguments: Vec<GraphQLArgument>,
}

/// GraphQL argument (e.g., `where: {...}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLArgument {
    /// Argument name
    pub name: String,

    /// Value type: "object", "variable", "string", "int", "boolean", etc.
    pub value_type: String,

    /// Serialized value as JSON string
    pub value_json: String,
}

impl PartialEq for GraphQLArgument {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value_json == other.value_json
    }
}

/// GraphQL type representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLType {
    /// Type name (e.g., "String", "User")
    pub name: String,
    /// Whether the type is nullable
    pub nullable: bool,
    /// Whether it's a list type
    pub list: bool,
    /// Whether list items are nullable
    pub list_nullable: bool,
}

/// Variable definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDefinition {
    /// Variable name without $ prefix
    pub name: String,

    /// Structured type information
    pub var_type: GraphQLType,

    /// Default value as JSON string
    pub default_value: Option<String>,
}

/// Fragment definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentDefinition {
    /// Fragment name
    pub name: String,

    /// Type this fragment applies to (e.g., "User")
    pub type_condition: String,

    /// Fields selected in fragment
    pub selections: Vec<FieldSelection>,

    /// Names of other fragments this one spreads
    pub fragment_spreads: Vec<String>,
}
