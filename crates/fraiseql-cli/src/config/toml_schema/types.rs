//! Type and field definitions for TOML schema configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeDefinition {
    /// SQL source table or view
    pub sql_source: String,
    /// Human-readable type description
    pub description: Option<String>,
    /// Field definitions
    pub fields: BTreeMap<String, FieldDefinition>,
}

impl Default for TypeDefinition {
    fn default() -> Self {
        Self {
            sql_source: "v_entity".to_string(),
            description: None,
            fields: BTreeMap::new(),
        }
    }
}

/// Field definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// GraphQL field type (ID, String, Int, Boolean, DateTime, etc.)
    #[serde(rename = "type")]
    pub field_type: String,
    /// Whether field can be null
    #[serde(default)]
    pub nullable: bool,
    /// Field description
    pub description: Option<String>,
}

/// Argument definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentDefinition {
    /// Argument name
    pub name: String,
    /// Argument type
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Whether argument is required
    #[serde(default)]
    pub required: bool,
    /// Default value if not provided
    pub default: Option<serde_json::Value>,
    /// Argument description
    pub description: Option<String>,
}
