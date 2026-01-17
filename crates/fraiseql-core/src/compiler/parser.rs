//! Schema parser - JSON → Authoring IR.
//!
//! Parses JSON schema definitions from Python/TypeScript decorators
//! into internal Intermediate Representation.
//!
//! # Example
//!
//! ```rust
//! use fraiseql_core::compiler::parser::SchemaParser;
//!
//! let parser = SchemaParser::new();
//! let schema_json = r#"{"types": [], "queries": []}"#;
//! let ir = parser.parse(schema_json).unwrap();
//! ```

use serde_json::Value;

use crate::error::{FraiseQLError, Result};
use super::ir::{AuthoringIR, IRType, IRQuery, IRMutation, IRSubscription, IRField, IRArgument, AutoParams, MutationOperation};

/// Schema parser.
///
/// Transforms JSON schema from authoring languages into internal IR.
pub struct SchemaParser {
    // Parser state (if needed in future)
}

impl SchemaParser {
    /// Create new schema parser.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Parse JSON schema into IR.
    ///
    /// # Arguments
    ///
    /// * `schema_json` - JSON schema string from decorators
    ///
    /// # Returns
    ///
    /// Parsed Authoring IR
    ///
    /// # Errors
    ///
    /// Returns error if JSON is malformed or missing required fields.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::compiler::parser::SchemaParser;
    ///
    /// let parser = SchemaParser::new();
    /// let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
    /// let ir = parser.parse(json).unwrap();
    /// assert!(ir.types.is_empty());
    /// ```
    pub fn parse(&self, schema_json: &str) -> Result<AuthoringIR> {
        // Parse JSON
        let value: Value = serde_json::from_str(schema_json)
            .map_err(|e| FraiseQLError::Parse {
                message: format!("Failed to parse schema JSON: {e}"),
                location: "root".to_string(),
            })?;

        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "Schema must be a JSON object".to_string(),
                location: "root".to_string(),
            })?;

        // Parse types
        let types = if let Some(types_val) = obj.get("types") {
            self.parse_types(types_val)?
        } else {
            Vec::new()
        };

        // Parse queries
        let queries = if let Some(queries_val) = obj.get("queries") {
            self.parse_queries(queries_val)?
        } else {
            Vec::new()
        };

        // Parse mutations
        let mutations = if let Some(mutations_val) = obj.get("mutations") {
            self.parse_mutations(mutations_val)?
        } else {
            Vec::new()
        };

        // Parse subscriptions
        let subscriptions = if let Some(subscriptions_val) = obj.get("subscriptions") {
            self.parse_subscriptions(subscriptions_val)?
        } else {
            Vec::new()
        };

        let fact_tables = if let Some(fact_tables_val) = obj.get("fact_tables") {
            if let Some(obj) = fact_tables_val.as_object() {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        } else {
            std::collections::HashMap::new()
        };

        Ok(AuthoringIR {
            types,
            queries,
            mutations,
            subscriptions,
            fact_tables,
        })
    }

    fn parse_types(&self, value: &Value) -> Result<Vec<IRType>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "types must be an array".to_string(),
                location: "types".to_string(),
            })?;

        array.iter().enumerate().map(|(i, type_val)| {
            self.parse_type(type_val, i)
        }).collect()
    }

    fn parse_type(&self, value: &Value, index: usize) -> Result<IRType> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Type at index {index} must be an object"),
                location: format!("types[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Type at index {index} missing 'name' field"),
                location: format!("types[{index}].name"),
            })?
            .to_string();

        let fields = if let Some(fields_val) = obj.get("fields") {
            self.parse_fields(fields_val, &name)?
        } else {
            Vec::new()
        };

        Ok(IRType {
            name,
            fields,
            sql_source: obj.get("sql_source").and_then(|v| v.as_str()).map(String::from),
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_fields(&self, value: &Value, type_name: &str) -> Result<Vec<IRField>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("fields for type {type_name} must be an array"),
                location: format!("{type_name}.fields"),
            })?;

        array.iter().enumerate().map(|(i, field_val)| {
            self.parse_field(field_val, type_name, i)
        }).collect()
    }

    fn parse_field(&self, value: &Value, type_name: &str, index: usize) -> Result<IRField> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Field at index {index} in type {type_name} must be an object"),
                location: format!("{type_name}.fields[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Field at index {index} in type {type_name} missing 'name'"),
                location: format!("{type_name}.fields[{index}].name"),
            })?
            .to_string();

        let field_type = obj.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Field '{name}' in type {type_name} missing 'type'"),
                location: format!("{type_name}.fields.{name}.type"),
            })?
            .to_string();

        let nullable = obj.get("nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(IRField {
            name,
            field_type,
            nullable,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            sql_column: obj.get("sql_column").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_queries(&self, value: &Value) -> Result<Vec<IRQuery>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "queries must be an array".to_string(),
                location: "queries".to_string(),
            })?;

        array.iter().enumerate().map(|(i, query_val)| {
            self.parse_query(query_val, i)
        }).collect()
    }

    fn parse_query(&self, value: &Value, index: usize) -> Result<IRQuery> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Query at index {index} must be an object"),
                location: format!("queries[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Query at index {index} missing 'name'"),
                location: format!("queries[{index}].name"),
            })?
            .to_string();

        let return_type = obj.get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Query '{name}' missing 'return_type'"),
                location: format!("queries.{name}.return_type"),
            })?
            .to_string();

        let returns_list = obj.get("returns_list")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let nullable = obj.get("nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let arguments = if let Some(args_val) = obj.get("arguments") {
            self.parse_arguments(args_val, &name)?
        } else {
            Vec::new()
        };

        let auto_params = if let Some(auto_val) = obj.get("auto_params") {
            self.parse_auto_params(auto_val)?
        } else {
            AutoParams::default()
        };

        Ok(IRQuery {
            name,
            return_type,
            returns_list,
            nullable,
            arguments,
            sql_source: obj.get("sql_source").and_then(|v| v.as_str()).map(String::from),
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            auto_params,
        })
    }

    fn parse_mutations(&self, value: &Value) -> Result<Vec<IRMutation>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "mutations must be an array".to_string(),
                location: "mutations".to_string(),
            })?;

        array.iter().enumerate().map(|(i, mutation_val)| {
            self.parse_mutation(mutation_val, i)
        }).collect()
    }

    fn parse_mutation(&self, value: &Value, index: usize) -> Result<IRMutation> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Mutation at index {index} must be an object"),
                location: format!("mutations[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Mutation at index {index} missing 'name'"),
                location: format!("mutations[{index}].name"),
            })?
            .to_string();

        let return_type = obj.get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Mutation '{name}' missing 'return_type'"),
                location: format!("mutations.{name}.return_type"),
            })?
            .to_string();

        let nullable = obj.get("nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let arguments = if let Some(args_val) = obj.get("arguments") {
            self.parse_arguments(args_val, &name)?
        } else {
            Vec::new()
        };

        let operation = obj.get("operation")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "create" => MutationOperation::Create,
                "update" => MutationOperation::Update,
                "delete" => MutationOperation::Delete,
                _ => MutationOperation::Custom,
            })
            .unwrap_or(MutationOperation::Custom);

        Ok(IRMutation {
            name,
            return_type,
            nullable,
            arguments,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            operation,
        })
    }

    fn parse_subscriptions(&self, value: &Value) -> Result<Vec<IRSubscription>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "subscriptions must be an array".to_string(),
                location: "subscriptions".to_string(),
            })?;

        array.iter().enumerate().map(|(i, sub_val)| {
            self.parse_subscription(sub_val, i)
        }).collect()
    }

    fn parse_subscription(&self, value: &Value, index: usize) -> Result<IRSubscription> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Subscription at index {index} must be an object"),
                location: format!("subscriptions[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Subscription at index {index} missing 'name'"),
                location: format!("subscriptions[{index}].name"),
            })?
            .to_string();

        let return_type = obj.get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Subscription '{name}' missing 'return_type'"),
                location: format!("subscriptions.{name}.return_type"),
            })?
            .to_string();

        let arguments = if let Some(args_val) = obj.get("arguments") {
            self.parse_arguments(args_val, &name)?
        } else {
            Vec::new()
        };

        Ok(IRSubscription {
            name,
            return_type,
            arguments,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_arguments(&self, value: &Value, parent_name: &str) -> Result<Vec<IRArgument>> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("arguments for '{parent_name}' must be an array"),
                location: format!("{parent_name}.arguments"),
            })?;

        array.iter().enumerate().map(|(i, arg_val)| {
            self.parse_argument(arg_val, parent_name, i)
        }).collect()
    }

    fn parse_argument(&self, value: &Value, parent_name: &str, index: usize) -> Result<IRArgument> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Argument at index {index} for '{parent_name}' must be an object"),
                location: format!("{parent_name}.arguments[{index}]"),
            })?;

        let name = obj.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Argument at index {index} for '{parent_name}' missing 'name'"),
                location: format!("{parent_name}.arguments[{index}].name"),
            })?
            .to_string();

        let arg_type = obj.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message: format!("Argument '{name}' for '{parent_name}' missing 'type'"),
                location: format!("{parent_name}.arguments.{name}.type"),
            })?
            .to_string();

        let nullable = obj.get("nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(IRArgument {
            name,
            arg_type,
            nullable,
            default_value: obj.get("default_value").cloned(),
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_auto_params(&self, value: &Value) -> Result<AutoParams> {
        let obj = value.as_object()
            .ok_or_else(|| FraiseQLError::Parse {
                message: "auto_params must be an object".to_string(),
                location: "auto_params".to_string(),
            })?;

        Ok(AutoParams {
            has_where: obj.get("has_where").and_then(|v| v.as_bool()).unwrap_or(false),
            has_order_by: obj.get("has_order_by").and_then(|v| v.as_bool()).unwrap_or(false),
            has_limit: obj.get("has_limit").and_then(|v| v.as_bool()).unwrap_or(false),
            has_offset: obj.get("has_offset").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }
}

impl Default for SchemaParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_schema() {
        let parser = SchemaParser::new();
        let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
        let ir = parser.parse(json).unwrap();

        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
        assert!(ir.mutations.is_empty());
        assert!(ir.subscriptions.is_empty());
    }

    #[test]
    fn test_parse_minimal_schema() {
        let parser = SchemaParser::new();
        let json = r"{}";
        let ir = parser.parse(json).unwrap();

        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
    }

    #[test]
    fn test_parse_type_with_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {"name": "id", "type": "Int!", "nullable": false},
                    {"name": "name", "type": "String!", "nullable": false}
                ],
                "sql_source": "v_user"
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.types.len(), 1);
        assert_eq!(ir.types[0].name, "User");
        assert_eq!(ir.types[0].fields.len(), 2);
        assert_eq!(ir.types[0].sql_source, Some("v_user".to_string()));
    }

    #[test]
    fn test_parse_query_with_auto_params() {
        let parser = SchemaParser::new();
        let json = r#"{
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "v_user",
                "auto_params": {
                    "has_where": true,
                    "has_limit": true
                }
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.queries.len(), 1);
        assert_eq!(ir.queries[0].name, "users");
        assert!(ir.queries[0].returns_list);
        assert!(ir.queries[0].auto_params.has_where);
        assert!(ir.queries[0].auto_params.has_limit);
    }

    #[test]
    fn test_parse_mutation() {
        let parser = SchemaParser::new();
        let json = r#"{
            "mutations": [{
                "name": "createUser",
                "return_type": "User",
                "nullable": false,
                "operation": "create",
                "arguments": [
                    {"name": "input", "type": "CreateUserInput!", "nullable": false}
                ]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.mutations.len(), 1);
        assert_eq!(ir.mutations[0].name, "createUser");
        assert_eq!(ir.mutations[0].operation, MutationOperation::Create);
        assert_eq!(ir.mutations[0].arguments.len(), 1);
    }

    #[test]
    fn test_parse_invalid_json() {
        let parser = SchemaParser::new();
        let json = "not valid json";
        let result = parser.parse(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_field() {
        let parser = SchemaParser::new();
        let json = r#"{
            "types": [{
                "fields": []
            }]
        }"#;
        let result = parser.parse(json);
        assert!(result.is_err());
    }
}
