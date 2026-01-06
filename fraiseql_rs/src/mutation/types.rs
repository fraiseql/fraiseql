// fraiseql_rs/src/mutation/types.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration for building GraphQL mutation responses
///
/// This struct consolidates all parameters needed for mutation response building,
/// replacing the need for 9+ individual function parameters. It supports a builder
/// pattern for ergonomic construction.
#[derive(Debug, Clone)]
pub struct MutationConfig<'a> {
    /// GraphQL field name (e.g., "createUser")
    pub field_name: &'a str,
    /// Success type name (e.g., `"CreateUserSuccess"`)
    pub success_type: &'a str,
    /// Error type name (e.g., `"CreateUserError"`)
    pub error_type: &'a str,
    /// Field name for entity (e.g., "user")
    pub entity_field_name: Option<&'a str>,
    /// Entity type for __typename (e.g., "User")
    pub entity_type: Option<&'a str>,
    /// Optional cascade field selections JSON
    pub cascade_selections: Option<&'a str>,
    /// Whether to convert field names and JSON keys to camelCase
    pub auto_camel_case: bool,
    /// Optional list of expected fields in success type for validation
    pub success_type_fields: Option<&'a [String]>,
    /// Optional list of expected fields in error type for field selection
    pub error_type_fields: Option<&'a [String]>,
}

impl<'a> MutationConfig<'a> {
    /// Create a new `MutationConfig` with required fields
    ///
    /// # Arguments
    /// * `field_name` - GraphQL field name
    /// * `success_type` - Success type name
    /// * `error_type` - Error type name
    ///
    /// # Example
    /// ```ignore
    /// let config = MutationConfig::new("createUser", "CreateUserSuccess", "CreateUserError")
    ///     .with_entity("user", "User");
    /// ```
    #[must_use]
    pub const fn new(field_name: &'a str, success_type: &'a str, error_type: &'a str) -> Self {
        Self {
            field_name,
            success_type,
            error_type,
            entity_field_name: None,
            entity_type: None,
            cascade_selections: None,
            auto_camel_case: true,
            success_type_fields: None,
            error_type_fields: None,
        }
    }

    /// Set entity field name and type (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_entity(mut self, field_name: &'a str, entity_type: &'a str) -> Self {
        self.entity_field_name = Some(field_name);
        self.entity_type = Some(entity_type);
        self
    }

    /// Set entity options with Option types (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_entity_options(
        mut self,
        field_name: Option<&'a str>,
        entity_type: Option<&'a str>,
    ) -> Self {
        self.entity_field_name = field_name;
        self.entity_type = entity_type;
        self
    }

    /// Set cascade selections (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_cascade_selections(mut self, selections: Option<&'a str>) -> Self {
        self.cascade_selections = selections;
        self
    }

    /// Set auto camelCase conversion (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_auto_camel_case(mut self, enabled: bool) -> Self {
        self.auto_camel_case = enabled;
        self
    }

    /// Set success type fields (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_success_type_fields(mut self, fields: Option<&'a [String]>) -> Self {
        self.success_type_fields = fields;
        self
    }

    /// Set error type fields (builder pattern)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_error_type_fields(mut self, fields: Option<&'a [String]>) -> Self {
        self.error_type_fields = fields;
        self
    }
}

/// Mutation response format (auto-detected)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationResponse {
    /// Simple format: entity-only response (no status field)
    Simple(SimpleResponse),
    /// Full format: `mutation_response` with status/message/entity
    Full(FullResponse),
}

/// Simple format: Just entity JSONB
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleResponse {
    /// Entity data (entire JSONB)
    pub entity: Value,
}

/// Full mutation response format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullResponse {
    /// Status string (required)
    pub status: String,
    /// Human-readable message (required)
    pub message: String,
    /// `PascalCase` type name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// Entity data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<Value>,
    /// List of updated fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_fields: Option<Vec<String>>,
    /// Cascade data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade: Option<Value>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Status classification (parsed from status string)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusKind {
    /// Successful operation (success, created, updated, deleted)
    Success(String),
    /// No-operation with reason (noop:reason)
    Noop(String),
    /// Error with reason (failed:reason, `not_found:reason`, etc.)
    Error(String),
}

impl StatusKind {
    /// Parse status string into classification
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn from_str(status: &str) -> Self {
        let status_lower = status.to_lowercase();

        #[allow(clippy::if_same_then_else)]
        // Error prefixes
        if status_lower.starts_with("failed:")
            || status_lower.starts_with("unauthorized:")
            || status_lower.starts_with("forbidden:")
            || status_lower.starts_with("not_found:")
            || status_lower.starts_with("conflict:")
            || status_lower.starts_with("timeout:")
        {
            Self::Error(status.to_string())
        }
        // Noop prefix
        else if status_lower.starts_with("noop:") {
            Self::Noop(status.to_string())
        }
        // Success keywords
        else if matches!(
            status_lower.as_str(),
            "success" | "created" | "updated" | "deleted" | "completed" | "ok" | "new"
        ) {
            Self::Success(status.to_string())
        }
        // Unknown - default to success (backward compat)
        else {
            Self::Success(status.to_string())
        }
    }

    /// Check if this status is a success variant
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Check if this status is an error variant
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Map to HTTP status code
    #[must_use]
    pub fn http_code(&self) -> u16 {
        match self {
            Self::Success(_) | Self::Noop(_) => 200,
            Self::Error(reason) => {
                let reason_lower = reason.to_lowercase();
                if reason_lower.contains("not_found") {
                    404
                } else if reason_lower.contains("unauthorized") {
                    401
                } else if reason_lower.contains("forbidden") {
                    403
                } else if reason_lower.contains("conflict") {
                    409
                } else if reason_lower.contains("validation") || reason_lower.contains("invalid") {
                    422
                } else if reason_lower.contains("timeout") {
                    408
                } else {
                    500
                }
            }
        }
    }
}

/// Error type for mutation processing
#[derive(Debug, Clone, thiserror::Error)]
pub enum MutationError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Entity type required when entity is present")]
    MissingEntityType,

    #[error("Entity type must be PascalCase, got: {0}")]
    InvalidEntityType(String),

    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
}

impl From<String> for MutationError {
    fn from(s: String) -> Self {
        Self::SerializationFailed(s)
    }
}

impl From<&str> for MutationError {
    fn from(s: &str) -> Self {
        Self::SerializationFailed(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_config_builder() {
        let config = MutationConfig::new("createUser", "CreateUserSuccess", "CreateUserError")
            .with_entity("user", "User")
            .with_auto_camel_case(true);

        assert_eq!(config.field_name, "createUser");
        assert_eq!(config.success_type, "CreateUserSuccess");
        assert_eq!(config.error_type, "CreateUserError");
        assert_eq!(config.entity_field_name, Some("user"));
        assert_eq!(config.entity_type, Some("User"));
        assert!(config.auto_camel_case);
    }

    #[test]
    fn test_status_kind_success() {
        assert!(StatusKind::from_str("success").is_success());
        assert!(StatusKind::from_str("created").is_success());
        assert!(StatusKind::from_str("UPDATED").is_success());
    }

    #[test]
    fn test_status_kind_error() {
        let status = StatusKind::from_str("failed:validation");
        assert!(status.is_error());
        assert_eq!(status.http_code(), 422);
    }

    #[test]
    fn test_status_kind_http_codes() {
        assert_eq!(StatusKind::from_str("not_found:user").http_code(), 404);
        assert_eq!(StatusKind::from_str("unauthorized:token").http_code(), 401);
        assert_eq!(StatusKind::from_str("conflict:duplicate").http_code(), 409);
    }

    #[test]
    fn test_simple_response_serde() {
        use serde_json::json;

        let simple = SimpleResponse {
            entity: json!({"id": "123", "name": "Test"}),
        };

        let serialized = serde_json::to_string(&simple).unwrap();
        let deserialized: SimpleResponse = serde_json::from_str(&serialized).unwrap();

        assert_eq!(simple, deserialized);
    }
}
