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
    pub(crate) fn validate_enum_name(name: &str) -> Result<()> {
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
    pub(crate) fn validate_enum_value_name(name: &str, enum_name: &str) -> Result<()> {
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
