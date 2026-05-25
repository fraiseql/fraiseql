#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_validator_creation() {
    let metadata = FederationMetadata::default();
    let _validator = RequiresProvidesValidator::new(metadata);
}

#[test]
fn test_runtime_validator_missing_required_field() {
    let directives = FieldFederationDirectives {
        requires:      vec![FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }],
        provides:      vec![],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

    let result = RequiresProvidesRuntimeValidator::validate_required_fields(
        "Order",
        "shippingEstimate",
        &directives,
        &entity_fields,
    );

    assert!(result.is_err(), "expected Err when required field 'weight' is missing");
}

#[test]
fn test_runtime_validator_all_required_fields_present() {
    let directives = FieldFederationDirectives {
        requires:      vec![FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }],
        provides:      vec![],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
    entity_fields.insert("weight".to_string(), serde_json::json!(5.0));

    let result = RequiresProvidesRuntimeValidator::validate_required_fields(
        "Order",
        "shippingEstimate",
        &directives,
        &entity_fields,
    );

    result.unwrap_or_else(|e| panic!("expected Ok when all required fields present: {e:?}"));
}

#[test]
fn test_entity_validation_multiple_missing_fields() {
    let directives = FieldFederationDirectives {
        requires:      vec![
            FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            },
            FieldPathSelection {
                path:     vec!["shippingAddress".to_string()],
                typename: "Order".to_string(),
            },
        ],
        provides:      vec![],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

    let result = RequiresProvidesRuntimeValidator::validate_required_fields(
        "Order",
        "shippingEstimate",
        &directives,
        &entity_fields,
    );

    match result {
        Err(errors) => assert_eq!(errors.len(), 2),
        Ok(()) => panic!("Expected validation errors for missing fields"),
    }
}

#[test]
fn test_entity_validation_partial_fields() {
    let directives = FieldFederationDirectives {
        requires:      vec![
            FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            },
            FieldPathSelection {
                path:     vec!["shippingAddress".to_string()],
                typename: "Order".to_string(),
            },
        ],
        provides:      vec![],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
    entity_fields.insert("weight".to_string(), serde_json::json!(5.0));

    let result = RequiresProvidesRuntimeValidator::validate_required_fields(
        "Order",
        "shippingEstimate",
        &directives,
        &entity_fields,
    );

    match result {
        Err(errors) => {
            assert_eq!(errors.len(), 1);
            match &errors[0] {
                DirectiveValidationError::MissingRequiredField { required_field, .. } => {
                    assert_eq!(required_field, "shippingAddress");
                },
                _ => panic!("Expected MissingRequiredField error"),
            }
        },
        Ok(()) => panic!("Expected validation error for missing shippingAddress"),
    }
}

#[test]
fn test_validate_provides_fields_missing() {
    let directives = FieldFederationDirectives {
        requires:      vec![],
        provides:      vec![FieldPathSelection {
            path:     vec!["userId".to_string()],
            typename: "Order".to_string(),
        }],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let entity_fields: HashMap<String, serde_json::Value> = HashMap::new();

    RequiresProvidesRuntimeValidator::validate_provides_fields(
        "Order",
        "user",
        &directives,
        &entity_fields,
    );
}

#[test]
fn test_validate_provides_fields_present() {
    let directives = FieldFederationDirectives {
        requires:      vec![],
        provides:      vec![FieldPathSelection {
            path:     vec!["userId".to_string()],
            typename: "Order".to_string(),
        }],
        external:      false,
        shareable:     false,
        inaccessible:  false,
        override_from: None,
    };

    let mut entity_fields: HashMap<String, serde_json::Value> = HashMap::new();
    entity_fields.insert("userId".to_string(), serde_json::json!("user-123"));

    RequiresProvidesRuntimeValidator::validate_provides_fields(
        "Order",
        "user",
        &directives,
        &entity_fields,
    );
}
