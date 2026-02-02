//!
//! Tests for validating @requires directives at runtime during HTTP entity resolution:
//! - Required fields present in entity representations before HTTP call
//! - Error messages when required fields missing
//! - Query augmentation to include required fields in _entities query
//! - Both single and batch entity resolution
//!
//! RED PHASE: These tests are expected to FAIL until HTTP @requires enforcement is implemented

use fraiseql_core::federation::types::{
    EntityRepresentation, FederatedType, FederationMetadata, FieldFederationDirectives,
    FieldPathSelection, KeyDirective,
};
use serde_json::json;

// ============================================================================
// Test: HTTP @requires Validation Before Remote Call
// ============================================================================

#[test]
fn test_http_requires_validation_missing_field() {
    // TEST: Should fail if required field is missing from representation
    // GIVEN: User.orders requires "email", but representation has only id
    // WHEN: We validate before HTTP resolution
    // THEN: Should return error without making HTTP call

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    // Entity representation missing email
    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    // Should fail because email (required by orders) is missing
    let result = validate_http_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err(), "Should fail when required field missing from representation");
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("email") || err.to_lowercase().contains("requires"),
        "Error should mention missing field: {}",
        err
    );
}

#[test]
fn test_http_requires_validation_field_present() {
    // TEST: Should pass if all required fields are present
    // GIVEN: User.orders requires "email", representation has it
    // WHEN: We validate before HTTP resolution
    // THEN: Should succeed (HTTP call can proceed)

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    // Entity representation has email
    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("email".to_string(), json!("user@example.com")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = validate_http_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when required fields present");
}

#[test]
fn test_http_requires_validation_multiple_fields() {
    // TEST: Multiple @requires directives must all be satisfied
    // GIVEN: Order.shippingEstimate requires both weight and dimensions
    // WHEN: We validate before HTTP resolution
    // THEN: All required fields must be present

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new()
            .add_requires(FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            })
            .add_requires(FieldPathSelection {
                path:     vec!["dimensions".to_string()],
                typename: "Order".to_string(),
            }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    // Has weight but missing dimensions
    let repr_missing_dimensions = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("weight".to_string(), json!(2.5)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result =
        validate_http_requires(&metadata, "Order", &["shippingEstimate"], &repr_missing_dimensions);
    assert!(result.is_err(), "Should fail when any required field is missing");

    // Has both required fields
    let repr_complete = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("weight".to_string(), json!(2.5)),
            ("dimensions".to_string(), json!("10x10x10")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = validate_http_requires(&metadata, "Order", &["shippingEstimate"], &repr_complete);
    assert!(result.is_ok(), "Should pass when all required fields present");
}

#[test]
fn test_http_requires_batch_validation() {
    // TEST: Multiple representations in batch must all have required fields
    // GIVEN: Batch of 3 User entities, 1 missing required email field
    // WHEN: We validate batch before HTTP _entities call
    // THEN: Should fail because one entity is missing required field

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "profile".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr1 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("1")),
            ("email".to_string(), json!("user1@example.com")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let repr2 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("2")),
            // Missing email!
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let repr3 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("3")),
            ("email".to_string(), json!("user3@example.com")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    // Validate each representation individually
    assert!(
        validate_http_requires(&metadata, "User", &["profile"], &repr1).is_ok(),
        "First representation should pass"
    );
    assert!(
        validate_http_requires(&metadata, "User", &["profile"], &repr2).is_err(),
        "Second representation should fail (missing email)"
    );
    assert!(
        validate_http_requires(&metadata, "User", &["profile"], &repr3).is_ok(),
        "Third representation should pass"
    );
}

// ============================================================================
// Test: Query Augmentation for Required Fields
// ============================================================================

#[test]
fn test_http_query_includes_required_fields() {
    // TEST: HTTP _entities query should include required fields
    // GIVEN: Requesting Order.shippingEstimate which requires weight
    // WHEN: We build the query
    // THEN: Query should include weight in inline fragments

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    // Build augmented field list
    let requested_fields = vec!["shippingEstimate".to_string()];
    let augmented_fields = compute_augmented_fields(&metadata, "Order", &requested_fields);

    // Should include both requested field and required field
    assert!(
        augmented_fields.contains(&"shippingEstimate".to_string()),
        "Should include requested field"
    );
    assert!(
        augmented_fields.contains(&"weight".to_string()),
        "Should include required field weight"
    );
}

#[test]
fn test_http_query_deduplicates_fields() {
    // TEST: Query should not duplicate fields
    // GIVEN: Multiple fields that require overlapping dependencies
    // WHEN: We augment the field list
    // THEN: Fields should be deduplicated

    let mut type_def = FederatedType::new("Order".to_string());
    type_def.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Both fields require weight
    type_def.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    type_def.set_field_directives(
        "taxAmount".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![type_def],
    };

    let requested_fields = vec!["shippingEstimate".to_string(), "taxAmount".to_string()];
    let augmented_fields = compute_augmented_fields(&metadata, "Order", &requested_fields);

    // Count occurrences of "weight"
    let weight_count = augmented_fields.iter().filter(|f| f == &"weight").count();
    assert_eq!(weight_count, 1, "Field weight should appear only once");
}

// ============================================================================
// Helper functions for HTTP @requires enforcement
// ============================================================================

/// Validate @requires directives for HTTP entity resolution
///
/// Before making HTTP calls to remote subgraphs, verifies that all fields
/// required by @requires directives are present in the entity representation
/// received from the Apollo Router or federation gateway.
///
/// # Arguments
/// * `metadata` - Federation metadata containing type and directive definitions
/// * `typename` - Name of the type being resolved (e.g., "User", "Order")
/// * `fields` - Field names being requested for HTTP resolution
/// * `representation` - Entity representation from gateway with available fields
///
/// # Returns
/// `Ok(())` if all required fields are present, `Err` with details if not
///
/// # Example
/// ```ignore
/// // User.orders requires "email", but representation only has id
/// let result = validate_http_requires(&metadata, "User", &["orders"], &repr);
/// // Returns Err: "HTTP Validation Error: Required field missing..."
/// ```
fn validate_http_requires(
    metadata: &FederationMetadata,
    typename: &str,
    fields: &[&str],
    representation: &EntityRepresentation,
) -> Result<(), String> {
    // Find the type in metadata
    let federated_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| format!("Type {} not found in federation metadata", typename))?;

    // Check @requires for each requested field
    for field in fields {
        if let Some(directives) = federated_type.get_field_directives(field) {
            // Verify all required fields are present in representation
            for required in &directives.requires {
                let field_path = required.path.join(".");
                if !representation.has_field(&field_path) {
                    return Err(format!(
                        "HTTP Validation Error: Required field missing\n\
                         Type: {}\n\
                         Field: {}\n\
                         Required: {}\n\
                         Issue: HTTP resolution of {}.{} requires '{}' but it is missing from entity representation\n\
                         Suggestion: Ensure '{}' is included in entity representation from gateway",
                        typename, field, field_path, typename, field, field_path, field_path
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Compute augmented field list including required fields
///
/// When building an HTTP _entities query for a remote subgraph, computes the
/// complete set of fields needed by augmenting requested fields with any fields
/// required by @requires directives.
///
/// # Arguments
/// * `metadata` - Federation metadata with type and directive information
/// * `typename` - Type name to compute fields for
/// * `fields` - Originally requested field names
///
/// # Returns
/// Vector of all field names needed (requested + required), deduplicated
///
/// # Example
/// ```ignore
/// // Requesting Order.shippingEstimate which requires Order.weight
/// let fields = vec!["shippingEstimate".to_string()];
/// let augmented = compute_augmented_fields(&metadata, "Order", &fields);
/// // Returns: ["shippingEstimate", "weight"]
/// ```
fn compute_augmented_fields(
    metadata: &FederationMetadata,
    typename: &str,
    fields: &[String],
) -> Vec<String> {
    let mut augmented = fields.to_vec();

    // Find the type and add all required fields
    if let Some(federated_type) = metadata.types.iter().find(|t| t.name == typename) {
        for field in fields {
            if let Some(directives) = federated_type.get_field_directives(field) {
                for required in &directives.requires {
                    // Add all components of the required field path
                    augmented.extend(required.path.clone());
                }
            }
        }
    }

    // Deduplicate while preserving insertion order
    let mut seen = std::collections::HashSet::new();
    augmented.retain(|f| seen.insert(f.clone()));

    augmented
}
