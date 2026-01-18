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
//! ```ignore
//! use fraiseql_core::compiler::ir::{AuthoringIR, IRType, IRField};
//!
//! let ir = AuthoringIR {
//!     types: vec![
//!         IRType {
//!             name: "User".to_string(),
//!             fields: vec![
//!                 IRField {
//!                     name: "id".to_string(),
//!                     field_type: "Int!".to_string(),
//!                     nullable: false,
//!                 }
//!             ],
//!             sql_source: Some("v_user".to_string()),
//!         }
//!     ],
//!     queries: vec![],
//!     mutations: vec![],
//!     subscriptions: vec![],
//! };
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

    /// Query definitions.
    pub queries: Vec<IRQuery>,

    /// Mutation definitions.
    pub mutations: Vec<IRMutation>,

    /// Subscription definitions.
    pub subscriptions: Vec<IRSubscription>,

    /// Fact table metadata (from Python decorators).
    /// Key: table name (e.g., "tf_sales")
    /// Value: FactTableMetadata as JSON
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fact_tables: HashMap<String, serde_json::Value>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRField {
    /// Field name.
    pub name: String,

    /// Field type (e.g., "String!", "Int", "[User]").
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

    /// Default value (as JSON).
    pub default_value: Option<serde_json::Value>,

    /// Argument description.
    pub description: Option<String>,
}

/// Auto-wired parameters configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IREnum {
    /// Enum name (e.g., "Status").
    pub name: String,

    /// Enum values.
    pub values: Vec<IREnumValue>,

    /// Enum description.
    pub description: Option<String>,
}

/// IR Enum value definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IREnumValue {
    /// Value name (e.g., "ACTIVE").
    pub name: String,

    /// Value description.
    pub description: Option<String>,

    /// Deprecation reason (if deprecated).
    pub deprecation_reason: Option<String>,
}

/// IR Interface definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRInterface {
    /// Interface name (e.g., "Node").
    pub name: String,

    /// Interface fields.
    pub fields: Vec<IRField>,

    /// Interface description.
    pub description: Option<String>,
}

/// IR Union definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRUnion {
    /// Union name (e.g., "SearchResult").
    pub name: String,

    /// Types that are part of this union.
    pub types: Vec<String>,

    /// Union description.
    pub description: Option<String>,
}

/// IR Input type definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IRInputType {
    /// Input type name (e.g., "CreateUserInput").
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

    /// Default value (as JSON).
    pub default_value: Option<serde_json::Value>,

    /// Field description.
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authoring_ir_new() {
        let ir = AuthoringIR::new();
        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
        assert!(ir.mutations.is_empty());
        assert!(ir.subscriptions.is_empty());
    }

    #[test]
    fn test_ir_type() {
        let ir_type = IRType {
            name:        "User".to_string(),
            fields:      vec![IRField {
                name:        "id".to_string(),
                field_type:  "Int!".to_string(),
                nullable:    false,
                description: None,
                sql_column:  Some("id".to_string()),
            }],
            sql_source:  Some("v_user".to_string()),
            description: Some("User type".to_string()),
        };

        assert_eq!(ir_type.name, "User");
        assert_eq!(ir_type.fields.len(), 1);
        assert_eq!(ir_type.sql_source, Some("v_user".to_string()));
    }

    #[test]
    fn test_ir_query() {
        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams {
                has_where: true,
                has_limit: true,
                ..Default::default()
            },
        };

        assert_eq!(query.name, "users");
        assert!(query.returns_list);
        assert!(query.auto_params.has_where);
        assert!(query.auto_params.has_limit);
    }

    #[test]
    fn test_ir_mutation() {
        let mutation = IRMutation {
            name:        "createUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![IRArgument {
                name:          "input".to_string(),
                arg_type:      "CreateUserInput!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: None,
            operation:   MutationOperation::Create,
        };

        assert_eq!(mutation.name, "createUser");
        assert_eq!(mutation.operation, MutationOperation::Create);
        assert_eq!(mutation.arguments.len(), 1);
    }

    #[test]
    fn test_auto_params_default() {
        let params = AutoParams::default();
        assert!(!params.has_where);
        assert!(!params.has_order_by);
        assert!(!params.has_limit);
        assert!(!params.has_offset);
    }

    #[test]
    fn test_mutation_operations() {
        assert_eq!(MutationOperation::Create, MutationOperation::Create);
        assert_ne!(MutationOperation::Create, MutationOperation::Update);
    }
}
