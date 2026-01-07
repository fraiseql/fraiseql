//! Advanced variable processing and validation.
//!
//! This module implements GraphQL variable type validation, coercion,
//! and default value handling according to the GraphQL specification.

use crate::graphql::types::{GraphQLType, ParsedQuery, VariableDefinition};
use crate::query::schema::SchemaMetadata;
use crate::validation::id_policy::IDPolicy;
use std::collections::HashMap;

/// Variable processing result
#[derive(Debug)]
pub struct VariableResult {
    /// Processed variables with coerced values
    pub variables: HashMap<String, serde_json::Value>,
    /// Any validation errors encountered
    pub errors: Vec<String>,
}

/// Variable processor for advanced GraphQL variable handling
#[derive(Debug)]
pub struct VariableProcessor {
    /// Variable definitions from the query
    definitions: HashMap<String, VariableDefinition>,
    /// ID policy for validation
    id_policy: IDPolicy,
}

impl VariableProcessor {
    /// Create a new variable processor
    #[must_use]
    pub fn new(query: &ParsedQuery) -> Self {
        // Extract variable definitions from query
        let definitions = query
            .variables
            .iter()
            .map(|var| (var.name.clone(), var.clone()))
            .collect();

        Self {
            definitions,
            id_policy: IDPolicy::default(),
        }
    }

    /// Create a new variable processor with schema and ID policy
    #[must_use]
    pub fn with_schema(query: &ParsedQuery, _schema: SchemaMetadata, id_policy: IDPolicy) -> Self {
        // Extract variable definitions from query
        let definitions = query
            .variables
            .iter()
            .map(|var| (var.name.clone(), var.clone()))
            .collect();

        Self {
            definitions,
            id_policy,
        }
    }

    /// Process and validate variables against their definitions
    #[must_use]
    pub fn process_variables(
        &self,
        input_variables: &HashMap<String, serde_json::Value>,
    ) -> VariableResult {
        let mut processed = HashMap::new();
        let mut errors = Vec::new();

        for (var_name, definition) in &self.definitions {
            match Self::process_variable(var_name, definition, input_variables) {
                Ok(value) => {
                    if self.should_skip_id_validation(&value, definition) {
                        processed.insert(var_name.clone(), value);
                    } else if let Err(e) = self.validate_id_value(&value, var_name) {
                        errors.push(e);
                    } else {
                        processed.insert(var_name.clone(), value);
                    }
                }
                Err(error) => {
                    errors.push(error);
                }
            }
        }

        // Check for undefined variables
        for var_name in input_variables.keys() {
            if !self.definitions.contains_key(var_name) {
                errors.push(format!("Variable '${var_name}' is not defined in query"));
            }
        }

        VariableResult {
            variables: processed,
            errors,
        }
    }

    /// Process a single variable
    fn process_variable(
        var_name: &str,
        definition: &VariableDefinition,
        input_variables: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        input_variables.get(var_name).map_or_else(
            || {
                // Use default value if available
                definition.default_value.as_ref().map_or_else(
                    || {
                        if definition.var_type.nullable {
                            Ok(serde_json::Value::Null)
                        } else {
                            Err(format!("Required variable '${var_name}' is not provided"))
                        }
                    },
                    |default_str| {
                        // Parse the JSON string to serde_json::Value
                        serde_json::from_str(default_str).map_err(|_| {
                            format!("Invalid default value for variable '${var_name}'")
                        })
                    },
                )
            },
            |value| {
                // Validate and coerce the provided value
                Self::validate_and_coerce_value(value, &definition.var_type)
            },
        )
    }

    /// Validate and coerce a value to the expected GraphQL type
    fn validate_and_coerce_value(
        value: &serde_json::Value,
        expected_type: &GraphQLType,
    ) -> Result<serde_json::Value, String> {
        match expected_type.name.as_str() {
            "String" => Self::coerce_to_string(value),
            "Int" => Self::coerce_to_int(value),
            "Float" => Self::coerce_to_float(value),
            "Boolean" => Self::coerce_to_boolean(value),
            "ID" => Self::coerce_to_id(value),
            _ => {
                // For custom types, just validate nullability
                if value.is_null() && !expected_type.nullable {
                    return Err(format!(
                        "Non-nullable type '{}' cannot be null",
                        expected_type.name
                    ));
                }
                Ok(value.clone())
            }
        }
    }

    fn coerce_to_string(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        match value {
            serde_json::Value::String(s) => Ok(serde_json::Value::String(s.clone())),
            serde_json::Value::Number(n) => Ok(serde_json::Value::String(n.to_string())),
            serde_json::Value::Bool(b) => Ok(serde_json::Value::String(b.to_string())),
            _ => Err("Cannot coerce value to String".to_string()),
        }
    }

    fn coerce_to_int(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        match value {
            serde_json::Value::Number(n) if n.is_i64() => Ok(serde_json::Value::Number(n.clone())),
            serde_json::Value::String(s) => s
                .parse::<i64>()
                .map(|n| serde_json::Value::Number(serde_json::Number::from(n)))
                .map_err(|_| "Cannot coerce string to Int".to_string()),
            _ => Err("Cannot coerce value to Int".to_string()),
        }
    }

    fn coerce_to_float(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        match value {
            serde_json::Value::Number(n) => Ok(serde_json::Value::Number(n.clone())),
            serde_json::Value::String(s) => s
                .parse::<f64>()
                .map(|n| {
                    serde_json::Number::from_f64(n).map_or_else(
                        || serde_json::Value::String(s.clone()),
                        serde_json::Value::Number,
                    )
                })
                .map_err(|_| "Cannot coerce string to Float".to_string()),
            _ => Err("Cannot coerce value to Float".to_string()),
        }
    }

    fn coerce_to_boolean(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        match value {
            serde_json::Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            serde_json::Value::String(s) => match s.to_lowercase().as_str() {
                "true" => Ok(serde_json::Value::Bool(true)),
                "false" => Ok(serde_json::Value::Bool(false)),
                _ => Err("Cannot coerce string to Boolean".to_string()),
            },
            _ => Err("Cannot coerce value to Boolean".to_string()),
        }
    }

    fn coerce_to_id(value: &serde_json::Value) -> Result<serde_json::Value, String> {
        // ID is serialized as String
        Self::coerce_to_string(value)
    }

    /// Check if ID validation should be skipped
    fn should_skip_id_validation(
        &self,
        value: &serde_json::Value,
        definition: &VariableDefinition,
    ) -> bool {
        !self.id_policy.enforces_uuid()
            || definition.var_type.name != "ID"
            || !matches!(value, serde_json::Value::String(_))
    }

    /// Validate ID value and return error string if invalid
    fn validate_id_value(&self, value: &serde_json::Value, var_name: &str) -> Result<(), String> {
        if let serde_json::Value::String(id_str) = value {
            crate::validation::id_policy::validate_id(id_str, self.id_policy)
                .map_err(|e| format!("Invalid ID in variable '${var_name}': {}", e.message))
        } else {
            Ok(())
        }
    }

    /// Static helper for testing - process variable without schema/policy
    #[cfg(test)]
    fn process_variable_static(
        var_name: &str,
        definition: &VariableDefinition,
        input_variables: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        Self::process_variable(var_name, definition, input_variables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql::types::{GraphQLType, VariableDefinition};

    #[test]
    fn test_string_coercion() {
        let query = ParsedQuery {
            variables: vec![VariableDefinition {
                name: "test".to_string(),
                var_type: GraphQLType {
                    name: "String".to_string(),
                    nullable: false,
                    list: false,
                    list_nullable: false,
                },
                default_value: None,
            }],
            ..Default::default()
        };
        let processor = VariableProcessor::new(&query);

        let result = processor.process_variables(&HashMap::from([(
            "test".to_string(),
            serde_json::json!("hello"),
        )]));

        assert!(result.errors.is_empty());
        assert_eq!(
            result.variables.get("test"),
            Some(&serde_json::json!("hello"))
        );
    }

    #[test]
    fn test_int_coercion() {
        let var_def = VariableDefinition {
            name: "test".to_string(),
            var_type: GraphQLType {
                name: "Int".to_string(),
                nullable: false,
                list: false,
                list_nullable: false,
            },
            default_value: None,
        };

        let result = VariableProcessor::process_variable_static(
            "test",
            &var_def,
            &HashMap::from([("test".to_string(), serde_json::json!(42))]),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!(42));
    }

    #[test]
    fn test_string_coercion_from_int() {
        let var_def = VariableDefinition {
            name: "test".to_string(),
            var_type: GraphQLType {
                name: "String".to_string(),
                nullable: false,
                list: false,
                list_nullable: false,
            },
            default_value: None,
        };

        let result = VariableProcessor::process_variable_static(
            "test",
            &var_def,
            &HashMap::from([("test".to_string(), serde_json::json!(123))]),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!("123"));
    }

    #[test]
    fn test_float_coercion() {
        let var_def = VariableDefinition {
            name: "test".to_string(),
            var_type: GraphQLType {
                name: "Float".to_string(),
                nullable: false,
                list: false,
                list_nullable: false,
            },
            default_value: None,
        };

        let result = VariableProcessor::process_variable_static(
            "test",
            &var_def,
            &HashMap::from([("test".to_string(), serde_json::json!(2.5))]),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!(2.5));
    }

    #[test]
    fn test_boolean_coercion() {
        let var_def = VariableDefinition {
            name: "test".to_string(),
            var_type: GraphQLType {
                name: "Boolean".to_string(),
                nullable: false,
                list: false,
                list_nullable: false,
            },
            default_value: None,
        };

        let result = VariableProcessor::process_variable_static(
            "test",
            &var_def,
            &HashMap::from([("test".to_string(), serde_json::json!(true))]),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!(true));
    }

    #[test]
    fn test_default_value_usage() {
        let query = ParsedQuery {
            variables: vec![VariableDefinition {
                name: "test".to_string(),
                var_type: GraphQLType {
                    name: "String".to_string(),
                    nullable: false,
                    list: false,
                    list_nullable: false,
                },
                default_value: Some("\"default_value\"".to_string()),
            }],
            ..Default::default()
        };
        let processor = VariableProcessor::new(&query);

        // Test with no variable provided - should use default
        let result = processor.process_variables(&HashMap::new());
        assert!(result.errors.is_empty());
        assert_eq!(
            result.variables.get("test"),
            Some(&serde_json::json!("default_value"))
        );
    }

    #[test]
    fn test_missing_required_variable() {
        let query = ParsedQuery {
            variables: vec![VariableDefinition {
                name: "required_var".to_string(),
                var_type: GraphQLType {
                    name: "String".to_string(),
                    nullable: false,
                    list: false,
                    list_nullable: false,
                },
                default_value: None,
            }],
            ..Default::default()
        };
        let processor = VariableProcessor::new(&query);

        let result = processor.process_variables(&HashMap::new());
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("required"));
    }

    #[test]
    fn test_invalid_variable_type() {
        let var_def = VariableDefinition {
            name: "test".to_string(),
            var_type: GraphQLType {
                name: "Int".to_string(),
                nullable: false,
                list: false,
                list_nullable: false,
            },
            default_value: None,
        };

        let result = VariableProcessor::process_variable_static(
            "test",
            &var_def,
            &HashMap::from([("test".to_string(), serde_json::json!("not_a_number"))]),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Int"));
    }

    #[test]
    fn test_id_variable_validation_with_uuid_policy() {
        let query = ParsedQuery {
            variables: vec![VariableDefinition {
                name: "userId".to_string(),
                var_type: GraphQLType {
                    name: "ID".to_string(),
                    nullable: false,
                    list: false,
                    list_nullable: false,
                },
                default_value: None,
            }],
            ..Default::default()
        };

        let schema = SchemaMetadata {
            tables: Default::default(),
            types: Default::default(),
            id_policy: crate::validation::id_policy::IDPolicy::UUID,
        };

        let processor = VariableProcessor::with_schema(
            &query,
            schema,
            crate::validation::id_policy::IDPolicy::UUID,
        );

        // Valid UUID should pass
        let result = processor.process_variables(&HashMap::from([(
            "userId".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        )]));
        assert!(result.errors.is_empty());

        // Invalid ID should fail
        let result = processor.process_variables(&HashMap::from([(
            "userId".to_string(),
            serde_json::json!("not-a-uuid"),
        )]));
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("Invalid ID"));
    }

    #[test]
    fn test_id_variable_validation_with_opaque_policy() {
        let query = ParsedQuery {
            variables: vec![VariableDefinition {
                name: "userId".to_string(),
                var_type: GraphQLType {
                    name: "ID".to_string(),
                    nullable: false,
                    list: false,
                    list_nullable: false,
                },
                default_value: None,
            }],
            ..Default::default()
        };

        let schema = SchemaMetadata {
            tables: Default::default(),
            types: Default::default(),
            id_policy: crate::validation::id_policy::IDPolicy::OPAQUE,
        };

        let processor = VariableProcessor::with_schema(
            &query,
            schema,
            crate::validation::id_policy::IDPolicy::OPAQUE,
        );

        // Any string should pass with OPAQUE policy
        let result = processor.process_variables(&HashMap::from([(
            "userId".to_string(),
            serde_json::json!("anything-goes"),
        )]));
        assert!(result.errors.is_empty());

        let result = processor.process_variables(&HashMap::from([(
            "userId".to_string(),
            serde_json::json!("12345"),
        )]));
        assert!(result.errors.is_empty());
    }
}
