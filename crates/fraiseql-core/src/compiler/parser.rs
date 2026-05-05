//! Schema parser - JSON → Authoring IR.
//!
//! Parses JSON schema definitions emitted by authoring-language decorators
//! into internal Intermediate Representation.
//!
//! Supports parsing all GraphQL schema elements:
//! - **Types**: Object definitions with fields
//! - **Interfaces**: Abstract type contracts that types can implement
//! - **Unions**: Type combinations allowing multiple member types
//! - **Input Types**: Input object definitions for mutations and filters
//! - **Enums**: Enumeration type definitions
//! - **Queries**: Root query definitions
//! - **Mutations**: Root mutation definitions
//! - **Subscriptions**: Root subscription definitions
//!
//! # Example
//!
//! ```rust
//! use fraiseql_core::compiler::parser::SchemaParser;
//!
//! let parser = SchemaParser::new();
//! let schema_json = r#"{
//!     "types": [{"name": "User", "fields": []}],
//!     "interfaces": [{"name": "Node", "fields": []}],
//!     "unions": [{"name": "SearchResult", "types": ["User"]}],
//!     "input_types": [{"name": "UserInput", "fields": []}],
//!     "queries": [{"name": "users", "return_type": "User", "returns_list": true}]
//! }"#;
//! let ir = parser.parse(schema_json).unwrap();
//! assert_eq!(ir.types.len(), 1);
//! assert_eq!(ir.interfaces.len(), 1);
//! assert_eq!(ir.unions.len(), 1);
//! assert_eq!(ir.input_types.len(), 1);
//! assert_eq!(ir.queries.len(), 1);
//! ```

use serde_json::Value;

use super::{
    enum_validator::EnumValidator,
    ir::{
        AuthoringIR, AutoParams, IRArgument, IRField, IRInputField, IRInputType, IRInterface,
        IRMutation, IRQuery, IRScalar, IRSubscription, IRType, IRUnion, MutationOperation,
    },
};
use crate::{
    error::{FraiseQLError, Result},
    schema::GraphQLValue,
};

/// Schema parser.
///
/// Transforms JSON schema from authoring languages into internal IR.
pub struct SchemaParser {
    // Parser state (if needed in future)
}

impl SchemaParser {
    /// Create new schema parser.
    #[must_use]
    pub const fn new() -> Self {
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
        let value: Value = serde_json::from_str(schema_json).map_err(|e| FraiseQLError::Parse {
            message:  format!("Failed to parse schema JSON: {e}"),
            location: "root".to_string(),
        })?;

        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  "Schema must be a JSON object".to_string(),
            location: "root".to_string(),
        })?;

        let types = obj.get("types").map_or(Ok(vec![]), |v| self.parse_types(v))?;
        let queries = obj.get("queries").map_or(Ok(vec![]), |v| self.parse_queries(v))?;
        let mutations = obj.get("mutations").map_or(Ok(vec![]), |v| self.parse_mutations(v))?;
        let subscriptions =
            obj.get("subscriptions").map_or(Ok(vec![]), |v| self.parse_subscriptions(v))?;
        let fact_tables = obj
            .get("fact_tables")
            .and_then(Value::as_object)
            .map_or_else::<Result<_>, _, _>(
                || Ok(std::collections::HashMap::new()),
                |o| {
                    o.iter()
                        .map(|(k, v)| {
                            let meta: crate::compiler::fact_table::FactTableMetadata =
                                serde_json::from_value(v.clone()).map_err(|e| {
                                    FraiseQLError::Parse {
                                        message:  format!(
                                            "Invalid fact table metadata for '{}': {e}",
                                            k
                                        ),
                                        location: format!("fact_tables.{}", k),
                                    }
                                })?;
                            Ok((k.clone(), meta))
                        })
                        .collect()
                },
            )?;
        let enums = obj.get("enums").map_or(Ok(vec![]), EnumValidator::parse_enums)?;
        let interfaces = obj.get("interfaces").map_or(Ok(vec![]), |v| self.parse_interfaces(v))?;
        let unions = obj.get("unions").map_or(Ok(vec![]), |v| self.parse_unions(v))?;
        let input_types =
            obj.get("input_types").map_or(Ok(vec![]), |v| self.parse_input_types(v))?;
        let scalars = obj.get("scalars").map_or(Ok(vec![]), |v| self.parse_scalars(v))?;

        // Warn about unsupported fragments feature
        if obj.contains_key("fragments") {
            tracing::warn!(
                "'fragments' feature in schema is not yet supported and will be ignored"
            );
        }

        Ok(AuthoringIR {
            types,
            enums,
            interfaces,
            unions,
            input_types,
            scalars,
            queries,
            mutations,
            subscriptions,
            fact_tables,
        })
    }

    fn parse_types(&self, value: &Value) -> Result<Vec<IRType>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "types must be an array".to_string(),
            location: "types".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, type_val)| self.parse_type(type_val, i))
            .collect()
    }

    fn parse_type(&self, value: &Value, index: usize) -> Result<IRType> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Type at index {index} must be an object"),
            location: format!("types[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Type at index {index} missing 'name' field"),
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
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("fields for type {type_name} must be an array"),
            location: format!("{type_name}.fields"),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, field_val)| self.parse_field(field_val, type_name, i))
            .collect()
    }

    fn parse_field(&self, value: &Value, type_name: &str, index: usize) -> Result<IRField> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Field at index {index} in type {type_name} must be an object"),
            location: format!("{type_name}.fields[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Field at index {index} in type {type_name} missing 'name'"),
                location: format!("{type_name}.fields[{index}].name"),
            })?
            .to_string();

        let field_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Field '{name}' in type {type_name} missing 'type'"),
                location: format!("{type_name}.fields.{name}.type"),
            })?
            .to_string();

        let nullable = obj.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);

        Ok(IRField {
            name,
            field_type,
            nullable,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
            sql_column: obj.get("sql_column").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_queries(&self, value: &Value) -> Result<Vec<IRQuery>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "queries must be an array".to_string(),
            location: "queries".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, query_val)| self.parse_query(query_val, i))
            .collect()
    }

    fn parse_query(&self, value: &Value, index: usize) -> Result<IRQuery> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Query at index {index} must be an object"),
            location: format!("queries[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Query at index {index} missing 'name'"),
                location: format!("queries[{index}].name"),
            })?
            .to_string();

        let return_type = obj
            .get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Query '{name}' missing 'return_type'"),
                location: format!("queries.{name}.return_type"),
            })?
            .to_string();

        let returns_list = obj.get("returns_list").and_then(|v| v.as_bool()).unwrap_or(false);

        let nullable = obj.get("nullable").and_then(|v| v.as_bool()).unwrap_or(false);

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
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "mutations must be an array".to_string(),
            location: "mutations".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, mutation_val)| self.parse_mutation(mutation_val, i))
            .collect()
    }

    fn parse_mutation(&self, value: &Value, index: usize) -> Result<IRMutation> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Mutation at index {index} must be an object"),
            location: format!("mutations[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Mutation at index {index} missing 'name'"),
                location: format!("mutations[{index}].name"),
            })?
            .to_string();

        let return_type = obj
            .get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Mutation '{name}' missing 'return_type'"),
                location: format!("mutations.{name}.return_type"),
            })?
            .to_string();

        let nullable = obj.get("nullable").and_then(|v| v.as_bool()).unwrap_or(false);

        let arguments = if let Some(args_val) = obj.get("arguments") {
            self.parse_arguments(args_val, &name)?
        } else {
            Vec::new()
        };

        let operation = if let Some(s) = obj.get("operation").and_then(|v| v.as_str()) {
            match s.to_lowercase().as_str() {
                "create" => MutationOperation::Create,
                "update" => MutationOperation::Update,
                "delete" => MutationOperation::Delete,
                "custom" => MutationOperation::Custom,
                other => {
                    return Err(FraiseQLError::Parse {
                        message:  format!(
                            "Mutation '{name}' has unknown operation {other:?}. \
                             Valid values are: create, update, delete, custom"
                        ),
                        location: format!("mutations.{name}.operation"),
                    });
                },
            }
        } else {
            MutationOperation::Custom
        };

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
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "subscriptions must be an array".to_string(),
            location: "subscriptions".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, sub_val)| self.parse_subscription(sub_val, i))
            .collect()
    }

    fn parse_subscription(&self, value: &Value, index: usize) -> Result<IRSubscription> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Subscription at index {index} must be an object"),
            location: format!("subscriptions[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Subscription at index {index} missing 'name'"),
                location: format!("subscriptions[{index}].name"),
            })?
            .to_string();

        let return_type = obj
            .get("return_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Subscription '{name}' missing 'return_type'"),
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
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("arguments for '{parent_name}' must be an array"),
            location: format!("{parent_name}.arguments"),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, arg_val)| self.parse_argument(arg_val, parent_name, i))
            .collect()
    }

    fn parse_argument(&self, value: &Value, parent_name: &str, index: usize) -> Result<IRArgument> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Argument at index {index} for '{parent_name}' must be an object"),
            location: format!("{parent_name}.arguments[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Argument at index {index} for '{parent_name}' missing 'name'"),
                location: format!("{parent_name}.arguments[{index}].name"),
            })?
            .to_string();

        let arg_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Argument '{name}' for '{parent_name}' missing 'type'"),
                location: format!("{parent_name}.arguments.{name}.type"),
            })?
            .to_string();

        let nullable = obj.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);

        Ok(IRArgument {
            name,
            arg_type,
            nullable,
            default_value: obj.get("default_value").map(GraphQLValue::from_json).transpose()?,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_auto_params(&self, value: &Value) -> Result<AutoParams> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  "auto_params must be an object".to_string(),
            location: "auto_params".to_string(),
        })?;

        Ok(AutoParams {
            has_where:    obj.get("has_where").and_then(|v| v.as_bool()).unwrap_or(false),
            has_order_by: obj.get("has_order_by").and_then(|v| v.as_bool()).unwrap_or(false),
            has_limit:    obj.get("has_limit").and_then(|v| v.as_bool()).unwrap_or(false),
            has_offset:   obj.get("has_offset").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }

    fn parse_interfaces(&self, value: &Value) -> Result<Vec<IRInterface>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "interfaces must be an array".to_string(),
            location: "interfaces".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, interface_val)| self.parse_interface(interface_val, i))
            .collect()
    }

    fn parse_interface(&self, value: &Value, index: usize) -> Result<IRInterface> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Interface at index {index} must be an object"),
            location: format!("interfaces[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Interface at index {index} missing 'name' field"),
                location: format!("interfaces[{index}].name"),
            })?
            .to_string();

        let fields = if let Some(fields_val) = obj.get("fields") {
            self.parse_fields(fields_val, &name)?
        } else {
            Vec::new()
        };

        Ok(IRInterface {
            name,
            fields,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_unions(&self, value: &Value) -> Result<Vec<IRUnion>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "unions must be an array".to_string(),
            location: "unions".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, union_val)| self.parse_union(union_val, i))
            .collect()
    }

    fn parse_union(&self, value: &Value, index: usize) -> Result<IRUnion> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Union at index {index} must be an object"),
            location: format!("unions[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Union at index {index} missing 'name' field"),
                location: format!("unions[{index}].name"),
            })?
            .to_string();

        let types = if let Some(types_val) = obj.get("types") {
            let array = types_val.as_array().ok_or_else(|| FraiseQLError::Parse {
                message:  format!("'types' for union {name} must be an array"),
                location: format!("unions.{name}.types"),
            })?;

            array
                .iter()
                .enumerate()
                .map(|(i, type_val)| {
                    type_val.as_str().ok_or_else(|| FraiseQLError::Parse {
                        message:  format!("Type at index {i} in union {name} must be a string"),
                        location: format!("unions.{name}.types[{i}]"),
                    })
                })
                .collect::<Result<Vec<_>>>()?
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            Vec::new()
        };

        Ok(IRUnion {
            name,
            types,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_input_types(&self, value: &Value) -> Result<Vec<IRInputType>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "input_types must be an array".to_string(),
            location: "input_types".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, input_type_val)| self.parse_input_type(input_type_val, i))
            .collect()
    }

    fn parse_input_type(&self, value: &Value, index: usize) -> Result<IRInputType> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Input type at index {index} must be an object"),
            location: format!("input_types[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Input type at index {index} missing 'name' field"),
                location: format!("input_types[{index}].name"),
            })?
            .to_string();

        let fields = if let Some(fields_val) = obj.get("fields") {
            self.parse_input_fields(fields_val, &name)?
        } else {
            Vec::new()
        };

        Ok(IRInputType {
            name,
            fields,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_input_fields(&self, value: &Value, type_name: &str) -> Result<Vec<IRInputField>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("fields for input type {type_name} must be an array"),
            location: format!("{type_name}.fields"),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, field_val)| self.parse_input_field(field_val, type_name, i))
            .collect()
    }

    fn parse_input_field(
        &self,
        value: &Value,
        type_name: &str,
        index: usize,
    ) -> Result<IRInputField> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Input field at index {index} in type {type_name} must be an object"),
            location: format!("{type_name}.fields[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!(
                    "Input field at index {index} in type {type_name} missing 'name'"
                ),
                location: format!("{type_name}.fields[{index}].name"),
            })?
            .to_string();

        let field_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Input field '{name}' in type {type_name} missing 'type'"),
                location: format!("{type_name}.fields.{name}.type"),
            })?
            .to_string();

        let nullable = obj.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);

        Ok(IRInputField {
            name,
            field_type,
            nullable,
            default_value: obj.get("default_value").map(GraphQLValue::from_json).transpose()?,
            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
        })
    }

    fn parse_scalars(&self, value: &Value) -> Result<Vec<IRScalar>> {
        let array = value.as_array().ok_or_else(|| FraiseQLError::Parse {
            message:  "scalars must be an array".to_string(),
            location: "scalars".to_string(),
        })?;

        array
            .iter()
            .enumerate()
            .map(|(i, scalar_val)| self.parse_scalar(scalar_val, i))
            .collect()
    }

    fn parse_scalar(&self, value: &Value, index: usize) -> Result<IRScalar> {
        let obj = value.as_object().ok_or_else(|| FraiseQLError::Parse {
            message:  format!("Scalar at index {index} must be an object"),
            location: format!("scalars[{index}]"),
        })?;

        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Parse {
                message:  format!("Scalar at index {index} missing 'name' field"),
                location: format!("scalars[{index}].name"),
            })?
            .to_string();

        let description = obj.get("description").and_then(|v| v.as_str()).map(String::from);
        let specified_by_url =
            obj.get("specified_by_url").and_then(|v| v.as_str()).map(String::from);
        let base_type = obj.get("base_type").and_then(|v| v.as_str()).map(String::from);

        // Parse validation rules if present
        let validation_rules = if let Some(rules_val) = obj.get("validation_rules") {
            serde_json::from_value(rules_val.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(IRScalar {
            name,
            description,
            specified_by_url,
            validation_rules,
            base_type,
        })
    }
}

impl Default for SchemaParser {
    fn default() -> Self {
        Self::new()
    }
}
