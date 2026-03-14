//! Validation engine for custom GraphQL scalars.
//!
//! Provides utilities to validate custom scalar values in different contexts.

use serde_json::Value;

use super::custom_scalar::CustomScalar;
use crate::error::{FraiseQLError, Result};

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
    pub const fn as_str(&self) -> &'static str {
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
            context:     context.into(),
            message:     message.into(),
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
/// ```
/// use fraiseql_core::validation::{validate_custom_scalar, ValidationContext, CustomScalar};
/// use fraiseql_core::error::Result;
/// use serde_json::{Value, json};
///
/// #[derive(Debug)]
/// struct Email;
/// impl CustomScalar for Email {
///     fn name(&self) -> &str { "Email" }
///     fn serialize(&self, value: &Value) -> Result<Value> { Ok(value.clone()) }
///     fn parse_value(&self, value: &Value) -> Result<Value> {
///         let str_val = value.as_str().unwrap();
///         if !str_val.contains('@') {
///             return Err(fraiseql_core::error::FraiseQLError::validation("invalid email"));
///         }
///         Ok(value.clone())
///     }
///     fn parse_literal(&self, ast: &Value) -> Result<Value> {
///         self.parse_value(ast)
///     }
/// }
///
/// let email = Email;
/// let result = validate_custom_scalar(&email, &json!("test@example.com"), ValidationContext::ParseValue).unwrap();
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::{Value, json};

    use super::*;
    use crate::error::{FraiseQLError, Result};

    /// Passthrough scalar that always succeeds (for happy-path tests).
    #[derive(Debug)]
    struct PassthroughScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for PassthroughScalar {
        fn name(&self) -> &str {
            "Passthrough"
        }

        fn serialize(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_value(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_literal(&self, ast: &Value) -> Result<Value> {
            Ok(ast.clone())
        }
    }

    /// Scalar that always fails with a descriptive error.
    #[derive(Debug)]
    struct FailScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for FailScalar {
        fn name(&self) -> &str {
            "AlwaysFail"
        }

        fn serialize(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("serialize always fails"))
        }

        fn parse_value(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("parse_value always fails"))
        }

        fn parse_literal(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("parse_literal always fails"))
        }
    }

    // ── ValidationContext tests ────────────────────────────────────────────────

    #[test]
    fn test_validation_context_as_str_serialize() {
        assert_eq!(ValidationContext::Serialize.as_str(), "serialize");
    }

    #[test]
    fn test_validation_context_as_str_parse_value() {
        assert_eq!(ValidationContext::ParseValue.as_str(), "parseValue");
    }

    #[test]
    fn test_validation_context_as_str_parse_literal() {
        assert_eq!(ValidationContext::ParseLiteral.as_str(), "parseLiteral");
    }

    #[test]
    fn test_validation_context_eq() {
        assert_eq!(ValidationContext::Serialize, ValidationContext::Serialize);
        assert_ne!(ValidationContext::Serialize, ValidationContext::ParseValue);
    }

    // ── ScalarValidationError tests ────────────────────────────────────────────

    #[test]
    fn test_scalar_validation_error_new() {
        let err = ScalarValidationError::new("Email", "parseValue", "not an email");
        assert_eq!(err.scalar_name, "Email");
        assert_eq!(err.context, "parseValue");
        assert_eq!(err.message, "not an email");
    }

    #[test]
    fn test_scalar_validation_error_display() {
        let err = ScalarValidationError::new("Email", "parseValue", "bad input");
        let s = format!("{err}");
        assert!(s.contains("Email"), "missing scalar name: {s}");
        assert!(s.contains("parseValue"), "missing context: {s}");
        assert!(s.contains("bad input"), "missing message: {s}");
    }

    #[test]
    fn test_scalar_validation_error_into_fraiseql_error() {
        let err = ScalarValidationError::new("T", "serialize", "oops");
        let fraiseql_err = err.into_fraiseql_error();
        let msg = format!("{fraiseql_err}");
        assert!(msg.contains("oops"), "error message lost: {msg}");
    }

    // ── validate_custom_scalar tests ──────────────────────────────────────────

    #[test]
    fn test_validate_serialize_success() {
        let scalar = PassthroughScalar;
        let v = json!("hello");
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::Serialize);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_parse_value_success() {
        let scalar = PassthroughScalar;
        let v = json!(42);
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::ParseValue);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_parse_literal_success() {
        let scalar = PassthroughScalar;
        let v = json!(true);
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::ParseLiteral);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_serialize_failure_wraps_error() {
        let scalar = FailScalar;
        let err =
            validate_custom_scalar(&scalar, &json!("x"), ValidationContext::Serialize).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("serialize"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn test_validate_parse_value_failure_wraps_error() {
        let scalar = FailScalar;
        let err = validate_custom_scalar(&scalar, &json!("x"), ValidationContext::ParseValue)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("parseValue"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn test_validate_parse_literal_failure_wraps_error() {
        let scalar = FailScalar;
        let err = validate_custom_scalar(&scalar, &json!("x"), ValidationContext::ParseLiteral)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("parseLiteral"),
            "unexpected error message: {msg}"
        );
    }

    // ── validate_custom_scalar_parse_value convenience fn ─────────────────────

    #[test]
    fn test_convenience_fn_success() {
        let scalar = PassthroughScalar;
        let v = json!("text");
        assert_eq!(validate_custom_scalar_parse_value(&scalar, &v).unwrap(), v);
    }

    #[test]
    fn test_convenience_fn_failure() {
        let scalar = FailScalar;
        assert!(validate_custom_scalar_parse_value(&scalar, &json!("x")).is_err());
    }
}
