//! Query and mutation operation definitions for TOML schema.

use serde::{Deserialize, Serialize};

use super::types::ArgumentDefinition;

/// Global defaults for list-query auto-params.
///
/// Applied when a per-query `auto_params` does not specify a given flag.
/// Relay queries and single-item queries are never affected.
///
/// ```toml
/// [query_defaults]
/// where    = true
/// order_by = true
/// limit    = false  # e.g. Relay-first project
/// offset   = false
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefaults {
    /// Enable automatic `where` filter parameter (default: true)
    #[serde(rename = "where", default = "default_true")]
    pub where_clause: bool,
    /// Enable automatic `order_by` parameter (default: true)
    #[serde(default = "default_true")]
    pub order_by:     bool,
    /// Enable automatic `limit` parameter (default: true)
    #[serde(default = "default_true")]
    pub limit:        bool,
    /// Enable automatic `offset` parameter (default: true)
    #[serde(default = "default_true")]
    pub offset:       bool,
}

impl Default for QueryDefaults {
    fn default() -> Self {
        Self {
            where_clause: true,
            order_by:     true,
            limit:        true,
            offset:       true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchemaMetadata {
    /// Schema name
    pub name:            String,
    /// Schema version
    pub version:         String,
    /// Optional schema description
    pub description:     Option<String>,
    /// Target database (postgresql, mysql, sqlite, sqlserver)
    pub database_target: String,
}

impl Default for SchemaMetadata {
    fn default() -> Self {
        Self {
            name:            "myapp".to_string(),
            version:         "1.0.0".to_string(),
            description:     None,
            database_target: "postgresql".to_string(),
        }
    }
}

/// Query definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefinition {
    /// Return type name
    pub return_type:  String,
    /// Whether query returns an array
    #[serde(default)]
    pub return_array: bool,
    /// SQL source for the query
    pub sql_source:   String,
    /// Query description
    pub description:  Option<String>,
    /// Query arguments
    pub args:         Vec<ArgumentDefinition>,
}

impl Default for QueryDefinition {
    fn default() -> Self {
        Self {
            return_type:  "String".to_string(),
            return_array: false,
            sql_source:   "v_entity".to_string(),
            description:  None,
            args:         vec![],
        }
    }
}

/// Mutation definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MutationDefinition {
    /// Return type name
    pub return_type: String,
    /// SQL function or procedure source.
    ///
    /// When absent, the compiler resolves the function name from the `[crud]`
    /// naming config using the `operation` and the entity name derived from
    /// `return_type`. A compile error is emitted if both are missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source:  Option<String>,
    /// Operation type (CREATE, UPDATE, DELETE)
    pub operation:   String,
    /// Mutation description
    pub description: Option<String>,
    /// Mutation arguments
    pub args:        Vec<ArgumentDefinition>,
}

impl Default for MutationDefinition {
    fn default() -> Self {
        Self {
            return_type: "String".to_string(),
            sql_source:  None,
            operation:   "CREATE".to_string(),
            description: None,
            args:        vec![],
        }
    }
}
