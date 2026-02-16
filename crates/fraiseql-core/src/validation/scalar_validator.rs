//! Validation engine for custom GraphQL scalars.
//!
//! Provides utilities to validate custom scalar values in different contexts.

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

use super::custom_scalar::CustomScalar;

/// Validation context for custom scalar operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationContext {
    /// Serialize a database value to GraphQL response.
    Serialize,

    /// Parse a variable value from GraphQL operation.
    ParseValue,

    /// Parse a literal value from GraphQL query string.
    ParseLiteral,
}

impl ValidationContext {
    /// Get the string representation of this context.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Serialize => "serialize",
            Self::ParseValue => "parseValue",
            Self::ParseLiteral => "parseLiteral",
        }
    }
}

/// Error returned when custom scalar validation fails.
#[derive(Debug, Clone)]
pub struct ScalarValidationError {
    /// Name of the scalar that failed validation.
    pub scalar_name: String,

    /// Context in which validation occurred.
    pub context: String,

    /// Underlying error message.
    pub message: String,
}

impl ScalarValidationError {
    /// Create a new scalar validation error.
    pub fn new(
        scalar_name: impl Into<String>,
        context: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            scalar_name: scalar_name.into(),
            context: context.into(),
            message: message.into(),
        }
    }

    /// Convert to FraiseQLError.
    pub fn into_fraiseql_error(self) -> FraiseQLError {
        FraiseQLError::validation(self.to_string())
    }
}

impl std::fmt::Display for ScalarValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Scalar \"{}\" validation failed in {}: {}",
            self.scalar_name, self.context, self.message
        )
    }
}

impl std::error::Error for ScalarValidationError {}

/// Validate a custom scalar value in a given context.
///
/// # Arguments
///
/// * `scalar` - The custom scalar implementation
/// * `value` - The value to validate
/// * `context` - The validation context
///
/// # Errors
///
/// Returns `ScalarValidationError` if validation fails.
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::validation::{validate_custom_scalar, ValidationContext, CustomScalar};
/// use serde_json::json;
///
/// struct Email;
/// impl CustomScalar for Email {
///     fn name(&self) -> &str { "Email" }
///     fn serialize(&self, value: &serde_json::Value) -> Result<serde_json::Value> { Ok(value.clone()) }
///     fn parse_value(&self, value: &serde_json::Value) -> Result<serde_json::Value> {
///         let str_val = value.as_str().unwrap();
///         if !str_val.contains('@') {
///             return Err(crate::error::FraiseQLError::validation("invalid email"));
///         }
///         Ok(value.clone())
///     }
///     fn parse_literal(&self, ast: &serde_json::Value) -> Result<serde_json::Value> {
///         self.parse_value(ast)
///     }
/// }
///
/// let email = Email;
/// let result = validate_custom_scalar(&email, &json!("test@example.com"), ValidationContext::ParseValue)?;
/// assert_eq!(result, json!("test@example.com"));
/// ```
pub fn validate_custom_scalar(
    scalar: &dyn CustomScalar,
    value: &Value,
    context: ValidationContext,
) -> Result<Value> {
    match context {
        ValidationContext::Serialize => scalar.serialize(value).map_err(|e| {
            FraiseQLError::validation(format!(
                "Scalar \"{}\" validation failed in serialize: {}",
                scalar.name(),
                e
            ))
        }),

        ValidationContext::ParseValue => scalar.parse_value(value).map_err(|e| {
            FraiseQLError::validation(format!(
                "Scalar \"{}\" validation failed in parseValue: {}",
                scalar.name(),
                e
            ))
        }),

        ValidationContext::ParseLiteral => scalar.parse_literal(value).map_err(|e| {
            FraiseQLError::validation(format!(
                "Scalar \"{}\" validation failed in parseLiteral: {}",
                scalar.name(),
                e
            ))
        }),
    }
}

/// Convenience function that defaults context to ParseValue.
pub fn validate_custom_scalar_parse_value(
    scalar: &dyn CustomScalar,
    value: &Value,
) -> Result<Value> {
    validate_custom_scalar(scalar, value, ValidationContext::ParseValue)
}
