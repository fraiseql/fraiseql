//! Intermediate Schema Format
//!
//! Language-agnostic schema representation that all language libraries output.
//! See .claude/INTERMEDIATE_SCHEMA_FORMAT.md for full specification.

use serde::{Deserialize, Serialize};

/// Intermediate schema - universal format from all language libraries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateSchema {
    /// Schema format version
    #[serde(default = "default_version")]
    pub version: String,

    /// GraphQL object types
    #[serde(default)]
    pub types: Vec<IntermediateType>,

    /// GraphQL queries
    #[serde(default)]
    pub queries: Vec<IntermediateQuery>,

    /// GraphQL mutations
    #[serde(default)]
    pub mutations: Vec<IntermediateMutation>,

    /// Analytics fact tables (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_tables: Option<Vec<IntermediateFactTable>>,

    /// Analytics aggregate queries (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_queries: Option<Vec<IntermediateAggregateQuery>>,
}

fn default_version() -> String {
    "2.0.0".to_string()
}

/// Type definition in intermediate format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateType {
    /// Type name (e.g., "User")
    pub name: String,

    /// Type fields
    pub fields: Vec<IntermediateField>,

    /// Type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Field definition in intermediate format
///
/// **NOTE**: Uses `type` field (not `field_type`)
/// This is the language-agnostic format. Rust conversion happens in converter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateField {
    /// Field name (e.g., "id")
    pub name: String,

    /// Field type name (e.g., "Int", "String", "User")
    ///
    /// **Language-agnostic**: All languages use "type", not "field_type"
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,
}

/// Query definition in intermediate format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateQuery {
    /// Query name (e.g., "users")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Query description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (table/view name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Auto-generated parameters config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_params: Option<IntermediateAutoParams>,
}

/// Mutation definition in intermediate format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateMutation {
    /// Mutation name (e.g., "createUser")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Mutation arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Mutation description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (function name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Operation type (CREATE, UPDATE, DELETE, CUSTOM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

/// Argument definition in intermediate format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateArgument {
    /// Argument name
    pub name: String,

    /// Argument type name
    ///
    /// **Language-agnostic**: Uses "type", not "arg_type"
    #[serde(rename = "type")]
    pub arg_type: String,

    /// Is argument optional?
    pub nullable: bool,

    /// Default value (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

/// Auto-params configuration in intermediate format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateAutoParams {
    #[serde(default)]
    pub limit: bool,
    #[serde(default)]
    pub offset: bool,
    #[serde(rename = "where", default)]
    pub where_clause: bool,
    #[serde(default)]
    pub order_by: bool,
}

/// Fact table definition in intermediate format (Analytics)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateFactTable {
    pub table_name: String,
    pub measures: Vec<IntermediateMeasure>,
    pub dimensions: IntermediateDimensions,
    pub denormalized_filters: Vec<IntermediateFilter>,
}

/// Measure column definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateMeasure {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
}

/// Dimensions metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateDimensions {
    pub name: String,
    pub paths: Vec<IntermediateDimensionPath>,
}

/// Dimension path within JSONB
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateDimensionPath {
    pub name: String,
    /// JSON path (accepts both "json_path" and "path" for cross-language compat)
    #[serde(alias = "path")]
    pub json_path: String,
    /// Data type (accepts both "data_type" and "type" for cross-language compat)
    #[serde(alias = "type")]
    pub data_type: String,
}

/// Denormalized filter column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateFilter {
    pub name: String,
    pub sql_type: String,
    pub indexed: bool,
}

/// Aggregate query definition (Analytics)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateAggregateQuery {
    pub name: String,
    pub fact_table: String,
    pub auto_group_by: bool,
    pub auto_aggregates: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_schema() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.version, "2.0.0");
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries.len(), 0);
        assert_eq!(schema.mutations.len(), 0);
    }

    #[test]
    fn test_parse_type_with_type_field() {
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {
                        "name": "id",
                        "type": "Int",
                        "nullable": false
                    },
                    {
                        "name": "name",
                        "type": "String",
                        "nullable": false
                    }
                ]
            }],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.types[0].name, "User");
        assert_eq!(schema.types[0].fields.len(), 2);
        assert_eq!(schema.types[0].fields[0].name, "id");
        assert_eq!(schema.types[0].fields[0].field_type, "Int");
        assert!(!schema.types[0].fields[0].nullable);
    }

    #[test]
    fn test_parse_query_with_arguments() {
        let json = r#"{
            "types": [],
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "arguments": [
                    {
                        "name": "limit",
                        "type": "Int",
                        "nullable": false,
                        "default": 10
                    }
                ],
                "sql_source": "v_user"
            }],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.queries.len(), 1);
        assert_eq!(schema.queries[0].arguments.len(), 1);
        assert_eq!(schema.queries[0].arguments[0].arg_type, "Int");
        assert_eq!(schema.queries[0].arguments[0].default, Some(serde_json::json!(10)));
    }
}
