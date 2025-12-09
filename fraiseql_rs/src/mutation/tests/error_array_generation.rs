//! Unit tests for error array generation in mutation responses.
//!
//! Tests the logic for:
//! - Extracting error identifiers from status strings
//! - Auto-generating errors arrays
//! - Handling explicit errors from metadata
//!
//! WP-034 Phase 1 (RED) - All tests should FAIL initially.

#[cfg(test)]
mod error_array_generation_tests {
    use serde_json::{json, Value};
    use crate::mutation::response_builder::{generate_errors_array, extract_identifier_from_status};
    use crate::mutation::{MutationStatus, MutationResult};

    #[test]
    fn test_extract_identifier_from_failed_with_colon() {
        // Status: "failed:validation" -> identifier: "validation"
        let status = MutationStatus::Error("failed:validation".to_string());
        let identifier = extract_identifier_from_status(&status);
        assert_eq!(identifier, "validation");
    }

    #[test]
    fn test_extract_identifier_from_noop_with_colon() {
        // Status: "noop:not_found" -> identifier: "not_found"
        let status = MutationStatus::Noop("not_found".to_string());
        let identifier = extract_identifier_from_status(&status);
        assert_eq!(identifier, "not_found");
    }

    #[test]
    fn test_extract_identifier_from_failed_without_colon() {
        // Status: "failed" (no colon) -> identifier: "general_error"
        let status = MutationStatus::Error("failed".to_string());
        let identifier = extract_identifier_from_status(&status);
        assert_eq!(identifier, "general_error");
    }

    #[test]
    fn test_extract_identifier_multiple_colons() {
        // Only split on first colon: "failed:validation:email" -> "validation:email"
        let status = MutationStatus::Error("failed:validation:email".to_string());
        let identifier = extract_identifier_from_status(&status);
        assert_eq!(identifier, "validation:email");
    }

    #[test]
    fn test_generate_errors_array_auto() {
        // Test auto-generation from status string
        let result = MutationResult {
            status: MutationStatus::Error("failed:validation".to_string()),
            message: "Validation failed".to_string(),
            entity: None,
            entity_type: Some("User".to_string()),
            entity_id: None,
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        let errors = generate_errors_array(&result, 400).unwrap();
        let errors_array = errors.as_array().unwrap();

        assert_eq!(errors_array.len(), 1);
        assert_eq!(errors_array[0]["code"], 400);
        assert_eq!(errors_array[0]["identifier"], "validation");
        assert_eq!(errors_array[0]["message"], "Validation failed");
        assert_eq!(errors_array[0]["details"], Value::Null);
    }

    #[test]
    fn test_generate_errors_array_explicit_override() {
        // Test that explicit errors in metadata override auto-generation
        let explicit_errors = json!([
            {
                "code": 400,
                "identifier": "email_invalid",
                "message": "Email format is invalid",
                "details": {"field": "email"}
            }
        ]);

        let result = MutationResult {
            status: MutationStatus::Error("failed:validation".to_string()),
            message: "Multiple validation errors".to_string(),
            entity: None,
            entity_type: Some("User".to_string()),
            entity_id: None,
            updated_fields: None,
            cascade: None,
            metadata: Some(json!({"errors": explicit_errors})),
            is_simple_format: false,
        };

        let errors = generate_errors_array(&result, 400).unwrap();
        let errors_array = errors.as_array().unwrap();

        // Should use explicit errors, NOT auto-generated
        assert_eq!(errors_array.len(), 1);
        assert_eq!(errors_array[0]["identifier"], "email_invalid");
        assert_eq!(errors_array[0]["message"], "Email format is invalid");
        assert_eq!(errors_array[0]["details"]["field"], "email");
    }

    #[test]
    fn test_generate_errors_array_noop_status() {
        // Test error generation from noop status (e.g., not_found)
        let result = MutationResult {
            status: MutationStatus::Noop("not_found".to_string()),
            message: "User not found".to_string(),
            entity: None,
            entity_type: Some("User".to_string()),
            entity_id: None,
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        let errors = generate_errors_array(&result, 404).unwrap();
        let errors_array = errors.as_array().unwrap();

        assert_eq!(errors_array.len(), 1);
        assert_eq!(errors_array[0]["code"], 404);
        assert_eq!(errors_array[0]["identifier"], "not_found");
        assert_eq!(errors_array[0]["message"], "User not found");
    }
}
