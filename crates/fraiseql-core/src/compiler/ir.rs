//! Intermediate Representation (IR) for schema compilation.
//!
//! The IR is the internal representation of a GraphQL schema during compilation.
//! It's created from authoring-time JSON and transformed into runtime-optimized
//! `CompiledSchema`.
//!
//! # IR Structure
//!
//! ```text
//! AuthoringIR
//! ├─ types: Vec<IRType>
//! ├─ queries: Vec<IRQuery>
//! ├─ mutations: Vec<IRMutation>
//! └─ subscriptions: Vec<IRSubscription>
//! ```
//!
//! # Example
//!
//! ```rust
//! use fraiseql_core::compiler::ir::{AuthoringIR, IRType, IRField};
//!
//! let mut ir = AuthoringIR::new();
//! ir.types.push(IRType {
//!     name: "User".to_string(),
//!     fields: vec![
//!         IRField {
//!             name: "id".to_string(),
//!             field_type: "Int!".to_string(),
//!             nullable: false,
//!             description: None,
//!             sql_column: None,
//!         }
//!     ],
//!     sql_source: Some("v_user".to_string()),
//!     description: None,
//! });
//! assert_eq!(ir.types.len(), 1);
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    compiler::fact_table::FactTableMetadata, schema::GraphQLValue, validation::ValidationRule,
};

/// Authoring Intermediate Representation.
///
/// This is the parsed representation of a GraphQL schema before
/// SQL template generation and optimization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthoringIR {
    /// Type definitions.
    pub types: Vec<IRType>,

    /// Enum definitions.
    #[serde(default)]
    pub enums: Vec<IREnum>,

    /// Interface definitions.
    #[serde(default)]
    pub interfaces: Vec<IRInterface>,

    /// Union definitions.
    #[serde(default)]
    pub unions: Vec<IRUnion>,

    /// Input type definitions.
    #[serde(default)]
    pub input_types: Vec<IRInputType>,

    /// Custom scalar type definitions.
    #[serde(default)]
    pub scalars: Vec<IRScalar>,

    /// Query definitions.
    pub queries: Vec<IRQuery>,

    /// Mutation definitions.
    pub mutations: Vec<IRMutation>,

    /// Subscription definitions.
    pub subscriptions: Vec<IRSubscription>,

    /// Fact table metadata (from authoring-language decorators).
    /// Key: table name (e.g., "`tf_sales`")
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fact_tables: HashMap<String, FactTableMetadata>,
}

impl AuthoringIR {
    /// Create empty IR.
    #[must_use]
    pub fn new() -> Self {
        Self {
            types:         Vec::new(),
            enums:         Vec::new(),
            interfaces:    Vec::new(),
            unions:        Vec::new(),
            input_types:   Vec::new(),
            scalars:       Vec::new(),
            queries:       Vec::new(),
            mutations:     Vec::new(),
            subscriptions: Vec::new(),
            fact_tables:   HashMap::new(),
        }
    }
}

impl Default for AuthoringIR {
    fn default() -> Self {
        Self::new()
    }
}

/// IR Type definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRType {
    /// Type name (e.g., "User").
    pub name: String,

    /// Field definitions.
    pub fields: Vec<IRField>,

    /// SQL source (table/view name).
    pub sql_source: Option<String>,

    /// Type description.
    pub description: Option<String>,
}

/// IR Field definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRField {
    /// Field name.
    pub name: String,

    /// Field type (e.g., `"String!"`, `"Int"`, `"[User]"`).
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,

    /// Field description.
    pub description: Option<String>,

    /// SQL column mapping.
    pub sql_column: Option<String>,
}

/// IR Query definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRQuery {
    /// Query name (e.g., "users", "user").
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Does this return a list?
    pub returns_list: bool,

    /// Is return value nullable?
    pub nullable: bool,

    /// Query arguments.
    pub arguments: Vec<IRArgument>,

    /// SQL source (table/view).
    pub sql_source: Option<String>,

    /// Query description.
    pub description: Option<String>,

    /// Auto-wired parameters (where, orderBy, limit, offset).
    pub auto_params: AutoParams,
}

/// IR Mutation definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRMutation {
    /// Mutation name (e.g., "createUser", "updatePost").
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Is return value nullable?
    pub nullable: bool,

    /// Mutation arguments.
    pub arguments: Vec<IRArgument>,

    /// Mutation description.
    pub description: Option<String>,

    /// SQL operation type.
    pub operation: MutationOperation,
}

/// IR Subscription definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRSubscription {
    /// Subscription name.
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Subscription arguments.
    pub arguments: Vec<IRArgument>,

    /// Subscription description.
    pub description: Option<String>,
}

/// IR Argument definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRArgument {
    /// Argument name.
    pub name: String,

    /// Argument type.
    pub arg_type: String,

    /// Is argument nullable?
    pub nullable: bool,

    /// Default value.
    pub default_value: Option<GraphQLValue>,

    /// Argument description.
    pub description: Option<String>,
}

/// Auto-wired parameters configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AutoParams {
    /// Enable WHERE parameter?
    #[serde(default)]
    pub has_where: bool,

    /// Enable orderBy parameter?
    #[serde(default)]
    pub has_order_by: bool,

    /// Enable limit parameter?
    #[serde(default)]
    pub has_limit: bool,

    /// Enable offset parameter?
    #[serde(default)]
    pub has_offset: bool,
}

/// Mutation operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MutationOperation {
    /// INSERT operation.
    Create,

    /// UPDATE operation.
    Update,

    /// DELETE operation.
    Delete,

    /// Custom SQL operation.
    Custom,
}

/// IR Enum definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IREnum {
    /// Enum name (e.g., "Status").
    pub name: String,

    /// Enum values.
    pub values: Vec<IREnumValue>,

    /// Enum description.
    pub description: Option<String>,
}

/// IR Enum value definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IREnumValue {
    /// Value name (e.g., "ACTIVE").
    pub name: String,

    /// Value description.
    pub description: Option<String>,

    /// Deprecation reason (if deprecated).
    pub deprecation_reason: Option<String>,
}

/// IR Interface definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRInterface {
    /// Interface name (e.g., "Node").
    pub name: String,

    /// Interface fields.
    pub fields: Vec<IRField>,

    /// Interface description.
    pub description: Option<String>,
}

/// IR Union definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IRUnion {
    /// Union name (e.g., "`SearchResult`").
    pub name: String,

    /// Types that are part of this union.
    pub types: Vec<String>,

    /// Union description.
    pub description: Option<String>,
}

/// IR Input type definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRInputType {
    /// Input type name (e.g., "`CreateUserInput`").
    pub name: String,

    /// Input fields.
    pub fields: Vec<IRInputField>,

    /// Input type description.
    pub description: Option<String>,
}

/// IR Input field definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRInputField {
    /// Field name.
    pub name: String,

    /// Field type (e.g., "String!", "Int").
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,

    /// Default value.
    pub default_value: Option<GraphQLValue>,

    /// Field description.
    pub description: Option<String>,
}

/// IR Scalar type definition.
///
/// Represents a custom scalar type with optional validation rules.
/// Custom scalars allow developers to define domain-specific scalar types
/// with validation rules beyond the builtin GraphQL scalars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRScalar {
    /// Scalar name (e.g., "Email", "ISBN", "IBAN").
    pub name: String,

    /// Scalar description.
    pub description: Option<String>,

    /// URL specification (RFC or standard that defines this scalar type).
    /// Per GraphQL spec §3.5.1 (`specified_by_url`).
    pub specified_by_url: Option<String>,

    /// Validation rules for this scalar.
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,

    /// Base type for type aliases (e.g., "String" for Email alias).
    pub base_type: Option<String>,
}

impl IRScalar {
    /// Create a new scalar definition with minimal required fields.
    #[must_use]
    pub const fn new(name: String) -> Self {
        Self {
            name,
            description: None,
            specified_by_url: None,
            validation_rules: Vec::new(),
            base_type: None,
        }
    }
}
