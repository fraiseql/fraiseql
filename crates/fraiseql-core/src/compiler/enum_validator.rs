//! Enum type validation and parsing for GraphQL schemas.
//!
//! Handles parsing of GraphQL enum type definitions from JSON schema and validates
//! enum structure, naming conventions, and value uniqueness.

use serde_json::Value;

use crate::{
    compiler::ir::{IREnum, IREnumValue},
    error::{FraiseQLError, Result},
};

/// Enum type validator and parser.
///
/// Validates GraphQL enum definitions for:
/// - Correct structure (name, values)
/// - Unique enum values
/// - Valid naming conventions
/// - Proper descriptions
#[derive(Debug)]
pub struct EnumValidator;

impl EnumValidator {
    /// Parse enum definitions from JSON schema.
    ///
    /// # Arguments
    ///
    /// * `enums_value` - JSON array of enum definitions
    ///
    /// # Returns
    ///
    /// Vec of parsed `IREnum` definitions
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if `enums_value` is not an array,
    /// any enum entry is not a JSON object, is missing a `name` field, has an
    /// invalid name, or contains duplicate enum values.
    ///
    /// # Example JSON Structure
    ///
    /// ```json
    /// {
    ///   "enums": [
    ///     {
    ///       "name": "UserStatus",
    ///       "description": "User account status",
    ///       "values": [
    ///         {
    ///           "name": "ACTIVE",
    ///           "description": "User is active",
    ///           "deprecationReason": null
    ///         },
    ///         {
    ///           "name": "INACTIVE"
    ///         }
    ///       ]
    ///     }
    ///   ]
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if `enums_value` is not an array,
    /// any enum definition is missing required fields, or variant names are invalid.
    pub fn parse_enums(enums_value: &Value) -> Result<Vec<IREnum>> {
        let enums_arr = enums_value.as_array().ok_or_else(|| FraiseQLError::Validation {
            message: "enums must be an array".to_string(),
            path:    Some("schema.enums".to_string()),
        })?;

        let mut enums = Vec::new();
        for (idx, enum_def) in enums_arr.iter().enumerate() {
            let enum_obj = enum_def.as_object().ok_or_else(|| FraiseQLError::Validation {
                message: format!("enum at index {} must be an object", idx),
                path:    Some(format!("schema.enums[{}]", idx)),
            })?;

            let enum_type = Self::parse_single_enum(enum_obj, idx)?;
            enums.push(enum_type);
        }

        Ok(enums)
    }

    /// Parse a single enum definition from JSON object.
    ///
    /// # Arguments
    ///
    /// * `enum_obj` - JSON object containing enum definition
    /// * `index` - Index in array for error reporting
    fn parse_single_enum(
        enum_obj: &serde_json::Map<String, Value>,
        index: usize,
    ) -> Result<IREnum> {
        // Extract name
        let name = enum_obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "enum must have a name".to_string(),
                path:    Some(format!("schema.enums[{}].name", index)),
            })?
            .to_string();

        // Validate enum name
        Self::validate_enum_name(&name)?;

        // Extract description (optional)
        let description =
            enum_obj.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Parse enum values
        let values_value = enum_obj.get("values").ok_or_else(|| FraiseQLError::Validation {
            message: format!("enum '{}' must have 'values' field", name),
            path:    Some(format!("schema.enums[{}].values", index)),
        })?;

        let values = Self::parse_enum_values(values_value, &name)?;

        // Validate that enum has at least one value
        if values.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("enum '{}' must have at least one value", name),
                path:    Some(format!("schema.enums[{}].values", index)),
            });
        }

        Ok(IREnum {
            name,
            values,
            description,
        })
    }

    /// Parse enum values from JSON array.
    ///
    /// # Arguments
    ///
    /// * `values_value` - JSON array of enum values
    /// * `enum_name` - Name of the enum (for error messages)
    fn parse_enum_values(values_value: &Value, enum_name: &str) -> Result<Vec<IREnumValue>> {
        let values_arr = values_value.as_array().ok_or_else(|| FraiseQLError::Validation {
            message: format!("enum '{}' values must be an array", enum_name),
            path:    Some(format!("schema.enums.{}.values", enum_name)),
        })?;

        let mut values = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for (idx, value_def) in values_arr.iter().enumerate() {
            let value_obj = value_def.as_object().ok_or_else(|| FraiseQLError::Validation {
                message: format!("enum '{}' value at index {} must be an object", enum_name, idx),
                path:    Some(format!("schema.enums.{}.values[{}]", enum_name, idx)),
            })?;

            // Extract value name
            let value_name = value_obj
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!(
                        "enum '{}' value at index {} must have a name",
                        enum_name, idx
                    ),
                    path:    Some(format!("schema.enums.{}.values[{}].name", enum_name, idx)),
                })?
                .to_string();

            // Validate enum value name
            Self::validate_enum_value_name(&value_name, enum_name)?;

            // Check for duplicate values
            if !seen_names.insert(value_name.clone()) {
                return Err(FraiseQLError::Validation {
                    message: format!("enum '{}' has duplicate value '{}'", enum_name, value_name),
                    path:    Some(format!("schema.enums.{}.values", enum_name)),
                });
            }

            // Extract description (optional)
            let description =
                value_obj.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());

            // Extract deprecation reason (optional)
            let deprecation_reason = value_obj
                .get("deprecationReason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            values.push(IREnumValue {
                name: value_name,
                description,
                deprecation_reason,
            });
        }

        Ok(values)
    }

    /// Validate enum type name follows GraphQL naming conventions.
    ///
    /// Valid names: `PascalCase` starting with letter, alphanumeric + underscore
    fn validate_enum_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(FraiseQLError::Validation {
                message: "enum name cannot be empty".to_string(),
                path:    Some("schema.enums.name".to_string()),
            });
        }

        if !name
            .chars()
            .next()
            .expect("name is non-empty; empty was rejected above")
            .is_alphabetic()
        {
            return Err(FraiseQLError::Validation {
                message: format!("enum name '{}' must start with a letter", name),
                path:    Some("schema.enums.name".to_string()),
            });
        }

        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "enum name '{}' contains invalid characters (use alphanumeric and underscore)",
                    name
                ),
                path:    Some("schema.enums.name".to_string()),
            });
        }

        Ok(())
    }

    /// Validate enum value name (should be `SCREAMING_SNAKE_CASE`).
    ///
    /// Valid names: UPPERCASE with underscores
    fn validate_enum_value_name(name: &str, enum_name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("enum '{}' value name cannot be empty", enum_name),
                path:    Some(format!("schema.enums.{}.values.name", enum_name)),
            });
        }

        if !name.chars().all(|c| c.is_uppercase() || c.is_numeric() || c == '_') {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "enum '{}' value '{}' should use SCREAMING_SNAKE_CASE (uppercase with underscores)",
                    enum_name, name
                ),
                path:    Some(format!("schema.enums.{}.values.name", enum_name)),
            });
        }

        // Check that it doesn't start with underscore
        if name.starts_with('_') {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "enum '{}' value '{}' cannot start with underscore",
                    enum_name, name
                ),
                path:    Some(format!("schema.enums.{}.values.name", enum_name)),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_parse_simple_enum() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"name": "ACTIVE"},
                    {"name": "INACTIVE"}
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse simple enum should succeed: {e}"));
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "Status");
        assert_eq!(enums[0].values.len(), 2);
    }

    #[test]
    fn test_parse_enum_with_description() {
        let json = serde_json::json!([
            {
                "name": "UserStatus",
                "description": "User account status",
                "values": [
                    {
                        "name": "ACTIVE",
                        "description": "User is active"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse enum with description should succeed: {e}"));
        assert_eq!(enums[0].description, Some("User account status".to_string()));
        assert_eq!(enums[0].values[0].description, Some("User is active".to_string()));
    }

    #[test]
    fn test_parse_enum_with_deprecation() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {
                        "name": "OLD_STATUS",
                        "deprecationReason": "Use NEW_STATUS instead"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse enum with deprecation should succeed: {e}"));
        assert_eq!(
            enums[0].values[0].deprecation_reason,
            Some("Use NEW_STATUS instead".to_string())
        );
    }

    #[test]
    fn test_parse_multiple_enums() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [{"name": "ACTIVE"}]
            },
            {
                "name": "Priority",
                "values": [{"name": "HIGH"}, {"name": "LOW"}]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse multiple enums should succeed: {e}"));
        assert_eq!(enums.len(), 2);
    }

    #[test]
    fn test_enum_not_array() {
        let json = serde_json::json!({"name": "Status"});
        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for non-array enums, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_missing_name() {
        let json = serde_json::json!([
            {
                "values": [{"name": "ACTIVE"}]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing enum name, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_missing_values() {
        let json = serde_json::json!([
            {
                "name": "Status"
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing values field, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_empty_values() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": []
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty values, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_duplicate_values() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"name": "ACTIVE"},
                    {"name": "ACTIVE"}
                ]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for duplicate values, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_value_missing_name() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"description": "Active status"}
                ]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing value name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_name_valid() {
        EnumValidator::validate_enum_name("Status")
            .unwrap_or_else(|e| panic!("'Status' should be valid: {e}"));
        EnumValidator::validate_enum_name("UserStatus")
            .unwrap_or_else(|e| panic!("'UserStatus' should be valid: {e}"));
        EnumValidator::validate_enum_name("Status2")
            .unwrap_or_else(|e| panic!("'Status2' should be valid: {e}"));
    }

    #[test]
    fn test_validate_enum_name_invalid_start() {
        let result = EnumValidator::validate_enum_name("2Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for name starting with digit, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_name_invalid_chars() {
        let result1 = EnumValidator::validate_enum_name("Status-Type");
        assert!(
            matches!(result1, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for hyphen in name, got: {result1:?}"
        );
        let result2 = EnumValidator::validate_enum_name("Status Type");
        assert!(
            matches!(result2, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for space in name, got: {result2:?}"
        );
    }

    #[test]
    fn test_validate_enum_value_valid() {
        EnumValidator::validate_enum_value_name("ACTIVE", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE' should be valid: {e}"));
        EnumValidator::validate_enum_value_name("ACTIVE_STATUS", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE_STATUS' should be valid: {e}"));
        EnumValidator::validate_enum_value_name("ACTIVE_STATUS_2", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE_STATUS_2' should be valid: {e}"));
    }

    #[test]
    fn test_validate_enum_value_invalid_lowercase() {
        let result = EnumValidator::validate_enum_value_name("Active", "Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for lowercase value name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_value_invalid_start_underscore() {
        let result = EnumValidator::validate_enum_value_name("_ACTIVE", "Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for underscore-prefixed value, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_name_empty() {
        let result = EnumValidator::validate_enum_name("");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty enum name, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_complex_enum_scenario() {
        let json = serde_json::json!([
            {
                "name": "OrderStatus",
                "description": "Order processing status",
                "values": [
                    {
                        "name": "PENDING",
                        "description": "Order awaiting processing"
                    },
                    {
                        "name": "PROCESSING",
                        "description": "Order is being processed"
                    },
                    {
                        "name": "COMPLETED",
                        "description": "Order has been completed"
                    },
                    {
                        "name": "CANCELLED",
                        "description": "Order was cancelled",
                        "deprecationReason": "Use VOID instead"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse complex enum scenario should succeed: {e}"));
        assert_eq!(enums[0].name, "OrderStatus");
        assert_eq!(enums[0].values.len(), 4);
        assert!(enums[0].values[3].deprecation_reason.is_some());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let enum_val = IREnum {
            name:        "Status".to_string(),
            values:      vec![IREnumValue {
                name:               "ACTIVE".to_string(),
                description:        Some("Active status".to_string()),
                deprecation_reason: None,
            }],
            description: Some("Status enum".to_string()),
        };

        let json = serde_json::to_string(&enum_val).expect("serialize should work");
        let restored: IREnum = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.name, enum_val.name);
        assert_eq!(restored.values.len(), enum_val.values.len());
    }
}
