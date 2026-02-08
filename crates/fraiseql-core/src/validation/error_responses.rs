//! GraphQL-compliant error response formatting for validation errors.
//!
//! This module provides utilities for converting validation errors into
//! GraphQL error responses with proper structure and context.

use crate::error::{FraiseQLError, ValidationFieldError};
use serde::{Deserialize, Serialize};

/// A GraphQL error with extensions (validation details).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLValidationError {
    /// Error message shown to client
    pub message: String,

    /// Path to the field with error (e.g., "createUser.input.email")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,

    /// Extensions with validation details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ValidationErrorExtensions>,
}

/// Extensions carrying validation-specific error details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationErrorExtensions {
    /// GraphQL error code
    pub code: String,

    /// Human-readable rule type that failed
    pub rule_type: String,

    /// Field path as dot-separated string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,

    /// Additional context (e.g., why pattern didn't match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Collection of GraphQL validation errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLValidationResponse {
    /// List of validation errors
    pub errors: Vec<GraphQLValidationError>,

    /// Total error count
    pub error_count: usize,
}

impl GraphQLValidationResponse {
    /// Create a new empty error response.
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            error_count: 0,
        }
    }

    /// Add a validation field error to the response.
    pub fn add_field_error(
        &mut self,
        field_error: ValidationFieldError,
        context: Option<serde_json::Value>,
    ) {
        let path = Self::parse_path(&field_error.field);
        let extensions = ValidationErrorExtensions {
            code: "VALIDATION_FAILED".to_string(),
            rule_type: field_error.rule_type,
            field_path: Some(field_error.field.clone()),
            context,
        };

        self.errors.push(GraphQLValidationError {
            message: format!("Validation failed: {}", field_error.message),
            path: Some(path),
            extensions: Some(extensions),
        });

        self.error_count += 1;
    }

    /// Add multiple validation errors at once.
    pub fn add_errors(&mut self, errors: Vec<ValidationFieldError>) {
        for error in errors {
            self.add_field_error(error, None);
        }
    }

    /// Convert from FraiseQLError to validation response.
    pub fn from_error(error: &FraiseQLError) -> Option<Self> {
        if let FraiseQLError::Validation { message, path } = error {
            let mut response = Self::new();
            response.errors.push(GraphQLValidationError {
                message: message.clone(),
                path: path.as_ref().map(|p| Self::parse_path(p)),
                extensions: Some(ValidationErrorExtensions {
                    code: "VALIDATION_FAILED".to_string(),
                    rule_type: "unknown".to_string(),
                    field_path: path.clone(),
                    context: None,
                }),
            });
            response.error_count = 1;
            Some(response)
        } else {
            None
        }
    }

    /// Parse a dot-separated field path into path segments.
    fn parse_path(path: &str) -> Vec<String> {
        path.split('.').map(|s| s.to_string()).collect()
    }

    /// Check if response has any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Serialize to JSON suitable for GraphQL response.
    pub fn to_graphql_errors(&self) -> serde_json::Value {
        serde_json::json!({
            "errors": self.errors,
            "error_count": self.error_count
        })
    }
}

impl Default for GraphQLValidationResponse {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_empty_response() {
        let response = GraphQLValidationResponse::new();
        assert!(!response.has_errors());
        assert_eq!(response.error_count, 0);
    }

    #[test]
    fn test_add_single_error() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("email", "pattern", "Invalid email format");
        response.add_field_error(field_error, None);

        assert!(response.has_errors());
        assert_eq!(response.error_count, 1);
        assert_eq!(response.errors[0].extensions.as_ref().unwrap().rule_type, "pattern");
    }

    #[test]
    fn test_add_multiple_errors() {
        let mut response = GraphQLValidationResponse::new();
        let errors = vec![
            ValidationFieldError::new("email", "pattern", "Invalid email"),
            ValidationFieldError::new("phone", "pattern", "Invalid phone"),
        ];
        response.add_errors(errors);

        assert_eq!(response.error_count, 2);
    }

    #[test]
    fn test_path_parsing() {
        let path = GraphQLValidationResponse::parse_path("user.email");
        assert_eq!(path, vec!["user".to_string(), "email".to_string()]);

        let path = GraphQLValidationResponse::parse_path("address.zipcode");
        assert_eq!(path, vec!["address".to_string(), "zipcode".to_string()]);
    }

    #[test]
    fn test_json_serialization() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("field1", "rule1", "Error message");
        response.add_field_error(field_error, Some(serde_json::json!({"detail": "extra"})));

        let json = response.to_graphql_errors();
        assert!(json["error_count"].is_number());
        assert!(json["errors"].is_array());
    }

    #[test]
    fn test_from_fraiseql_error() {
        let error = FraiseQLError::Validation {
            message: "Validation failed".to_string(),
            path: Some("user.email".to_string()),
        };

        let response = GraphQLValidationResponse::from_error(&error);
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(response.error_count, 1);
    }

    #[test]
    fn test_context_inclusion() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("password", "length", "Too short");
        let context = serde_json::json!({"minimum_length": 12, "provided_length": 8});
        response.add_field_error(field_error, Some(context));

        assert!(response.errors[0].extensions.as_ref().unwrap().context.is_some());
    }
}
