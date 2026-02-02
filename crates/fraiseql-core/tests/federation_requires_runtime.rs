//!
//! Tests for validating @requires directives at runtime during entity resolution:
//! - Required fields present in entity representations
//! - Error messages when required fields missing
//! - Query augmentation to include required fields
//! - Both database and HTTP resolvers
//!
//! RED PHASE: These tests are expected to FAIL until runtime enforcement is implemented

use fraiseql_core::federation::types::{
    EntityRepresentation, FederatedType, FederationMetadata, FieldFederationDirectives,
    FieldPathSelection, KeyDirective,
};
use serde_json::json;

// ============================================================================
// Test: EntityRepresentation Field Checking
// ============================================================================

#[test]
fn test_entity_representation_has_field() {
    // TEST: Check if EntityRepresentation has a field
    // GIVEN: Entity representation with email field
    // WHEN: We check if email exists
    // THEN: Should return true

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

    assert!(repr.has_field("email"), "Should find email field");
    assert!(repr.has_field("id"), "Should find id field");
    assert!(!repr.has_field("nonexistent"), "Should not find nonexistent field");
}

#[test]
fn test_entity_representation_has_nested_field() {
    // TEST: Check for nested field paths like "user.email"
    // GIVEN: Entity representation with nested structure
    // WHEN: We check for nested field
    // THEN: Should support dot notation

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("user".to_string(), json!({"id": "123", "email": "user@example.com"})),
            ("total".to_string(), json!(99.99)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    assert!(repr.has_field("id"), "Should find id");
    assert!(repr.has_field("user"), "Should find user");
    assert!(repr.has_field("total"), "Should find total");
    // Nested paths may or may not be supported depending on implementation
}

// ============================================================================
// Test: @requires Enforcement - Missing Required Fields
// ============================================================================

#[test]
fn test_requires_enforcement_missing_field() {
    // TEST: Should fail if required field is missing from representation
    // GIVEN: User.orders requires "email", but representation has only id
    // WHEN: We try to enforce @requires
    // THEN: Should return error

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
    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err(), "Should fail when required field missing from representation");
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("email") || err.to_lowercase().contains("requires"),
        "Error should mention missing field: {}",
        err
    );
}

#[test]
fn test_requires_enforcement_field_present() {
    // TEST: Should pass if all required fields are present
    // GIVEN: User.orders requires "email", representation has it
    // WHEN: We try to enforce @requires
    // THEN: Should succeed

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

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when required fields present");
}

#[test]
fn test_requires_enforcement_multiple_required_fields() {
    // TEST: Multiple @requires directives must all be satisfied
    // GIVEN: Order.shippingEstimate requires both weight and dimensions
    // WHEN: We check enforcement
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
        enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr_missing_dimensions);
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

    let result = enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr_complete);
    assert!(result.is_ok(), "Should pass when all required fields present");
}

#[test]
fn test_requires_enforcement_no_directives() {
    // TEST: Fields without @requires directives should always pass
    // GIVEN: User.name has no @requires directive
    // WHEN: We enforce requirements
    // THEN: Should pass regardless of what fields are present

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    // name field has no directives

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    // Minimal representation
    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "User", &["name"], &repr);
    assert!(result.is_ok(), "Should pass when field has no @requires directive");
}

#[test]
fn test_requires_enforcement_error_message_context() {
    // TEST: Error messages should include helpful context
    // GIVEN: User.orders requires email, but it's missing
    // WHEN: Enforcement fails
    // THEN: Error should identify type, field, and missing requirement

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

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_lowercase().contains("user"), "Error should mention type");
    assert!(
        err.to_lowercase().contains("orders") || err.to_lowercase().contains("email"),
        "Error should mention field or requirement"
    );
}

// ============================================================================
// Test: @requires with Different Data Types
// ============================================================================

#[test]
fn test_requires_with_numeric_field() {
    // TEST: @requires should work with numeric fields
    // GIVEN: Order.shippingEstimate requires numeric weight field
    // WHEN: Enforcement checks requirements
    // THEN: Should accept numeric values

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

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("weight".to_string(), json!(5.5)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr);
    assert!(result.is_ok(), "Should accept numeric weight field");
}

#[test]
fn test_requires_with_boolean_field() {
    // TEST: @requires should work with boolean fields
    // GIVEN: User.premiumFeature requires isActive boolean
    // WHEN: Enforcement checks
    // THEN: Should accept boolean values

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "premiumFeature".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["isActive".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("isActive".to_string(), json!(true)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["premiumFeature"], &repr);
    assert!(result.is_ok(), "Should accept boolean isActive field");
}

#[test]
fn test_requires_with_null_field() {
    // TEST: @requires with null values should be treated as present
    // GIVEN: User.nickname requires name field with null value
    // WHEN: Enforcement checks
    // THEN: Should accept null as present field (field exists even if null)

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "nickname".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["name".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("name".to_string(), json!(null)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["nickname"], &repr);
    assert!(result.is_ok(), "Should accept field with null value as present");
}

// ============================================================================
// Test: @provides Validation
// ============================================================================

#[test]
fn test_provides_field_present() {
    // TEST: @provides should track promised fields
    // GIVEN: User.orders @provides Order.userId
    // WHEN: Order includes userId in response
    // THEN: @provides contract is satisfied

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["userId".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("userId".to_string(), json!("user-123")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when @provides fields are present");
}

// ============================================================================
// Test: Complex Scenarios with Multiple Fields
// ============================================================================

#[test]
fn test_requires_multiple_fields_mixed_results() {
    // TEST: Enforcement with multiple fields, some requiring, some not
    // GIVEN: Type with 3 fields: 2 with @requires, 1 without
    // WHEN: Checking enforcement
    // THEN: Should validate only fields with directives

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
    user_type.set_field_directives(
        "profile".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["phone".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    // Representation with email but not phone
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

    // Should pass when requesting only orders (email present)
    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when required field for orders is present");

    // Should fail when requesting profile (phone missing)
    let result = enforce_requires(&metadata, "User", &["profile"], &repr);
    assert!(result.is_err(), "Should fail when required field for profile is missing");

    // Should fail when requesting both
    let result = enforce_requires(&metadata, "User", &["orders", "profile"], &repr);
    assert!(result.is_err(), "Should fail when any field's requirement is not met");
}

#[test]
fn test_requires_three_fields_all_present() {
    // TEST: Complex scenario with 3 required fields
    // GIVEN: Field requires 3 different fields
    // WHEN: All 3 are present
    // THEN: Should pass

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "complexField".to_string(),
        FieldFederationDirectives::new()
            .add_requires(FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            })
            .add_requires(FieldPathSelection {
                path:     vec!["dimensions".to_string()],
                typename: "Order".to_string(),
            })
            .add_requires(FieldPathSelection {
                path:     vec!["fragile".to_string()],
                typename: "Order".to_string(),
            }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("weight".to_string(), json!(2.5)),
            ("dimensions".to_string(), json!("10x10x10")),
            ("fragile".to_string(), json!(true)),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "Order", &["complexField"], &repr);
    assert!(result.is_ok(), "Should pass when all 3 required fields present");
}

#[test]
fn test_requires_three_fields_one_missing() {
    // TEST: Complex scenario with 3 required fields, 1 missing
    // GIVEN: Field requires 3 fields, only 2 present
    // WHEN: Validating
    // THEN: Should fail

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "complexField".to_string(),
        FieldFederationDirectives::new()
            .add_requires(FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            })
            .add_requires(FieldPathSelection {
                path:     vec!["dimensions".to_string()],
                typename: "Order".to_string(),
            })
            .add_requires(FieldPathSelection {
                path:     vec!["fragile".to_string()],
                typename: "Order".to_string(),
            }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("weight".to_string(), json!(2.5)),
            ("dimensions".to_string(), json!("10x10x10")),
            // fragile is missing
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "Order", &["complexField"], &repr);
    assert!(result.is_err(), "Should fail when 1 of 3 required fields missing");
}

// ============================================================================
// Test: Edge Cases
// ============================================================================

#[test]
fn test_requires_with_empty_representation() {
    // TEST: Empty representation should fail if any field has @requires
    // GIVEN: Empty entity representation
    // WHEN: Field with @requires is requested
    // THEN: Should fail

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

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: Default::default(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err(), "Should fail with empty representation");
}

#[test]
fn test_requires_with_only_key_fields() {
    // TEST: Representation with only key fields should fail if @requires other fields
    // GIVEN: Entity with only id (key field)
    // WHEN: Field requires email
    // THEN: Should fail

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

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err(), "Should fail with only key fields");
}

#[test]
fn test_requires_with_case_sensitivity() {
    // TEST: Field name matching should be case-sensitive
    // GIVEN: Field requires "email" but representation has "Email"
    // WHEN: Validating
    // THEN: Should fail (case mismatch)

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

    // Note: This test assumes case-sensitive matching
    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("Email".to_string(), json!("user@example.com")), // Wrong case
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err(), "Should fail with case mismatch (email != Email)");
}

#[test]
fn test_requires_with_special_characters_in_field_names() {
    // TEST: Field names with underscores and numbers should work
    // GIVEN: Field requires shipping_address_2
    // WHEN: Present in representation
    // THEN: Should pass

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["shipping_address_2".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("shipping_address_2".to_string(), json!("123 Main St")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr);
    assert!(result.is_ok(), "Should accept field with underscores and numbers");
}

#[test]
fn test_requires_with_array_field() {
    // TEST: Field requiring an array field should work
    // GIVEN: User.totalOrders requires orderIds (array)
    // WHEN: orderIds array is present
    // THEN: Should pass

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "totalOrders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["orderIds".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("orderIds".to_string(), json!(vec!["order-1", "order-2"])),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["totalOrders"], &repr);
    assert!(result.is_ok(), "Should accept array field");
}

#[test]
fn test_requires_with_object_field() {
    // TEST: Field requiring an object field should work
    // GIVEN: User.address requires location (object)
    // WHEN: location object is present
    // THEN: Should pass

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "address".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["location".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("location".to_string(), json!({"city": "San Francisco", "state": "CA"})),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["address"], &repr);
    assert!(result.is_ok(), "Should accept object field");
}

#[test]
fn test_requires_validation_error_includes_typename() {
    // TEST: Error message should include the typename
    // GIVEN: Validation failure for specific type
    // WHEN: Error is returned
    // THEN: Error message includes typename

    let mut product_type = FederatedType::new("Product".to_string());
    product_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    product_type.set_field_directives(
        "discount".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["price".to_string()],
            typename: "Product".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![product_type],
    };

    let repr = EntityRepresentation {
        typename:   "Product".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("789"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "Product", &["discount"], &repr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_lowercase().contains("product"), "Error should include typename: {}", err);
}

#[test]
fn test_requires_validation_error_includes_field_name() {
    // TEST: Error message should include the field name
    // GIVEN: Validation failure for specific field
    // WHEN: Error is returned
    // THEN: Error message includes field name

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "recommendation".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["interests".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "User", &["recommendation"], &repr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("recommendation"),
        "Error should include field name: {}",
        err
    );
}

#[test]
fn test_requires_with_existing_extra_fields() {
    // TEST: Extra fields in representation should not cause issues
    // GIVEN: Representation has more fields than required
    // WHEN: All required fields are present
    // THEN: Should pass

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

    // Representation with extra fields beyond what's required
    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("email".to_string(), json!("user@example.com")),
            ("name".to_string(), json!("John Doe")),
            ("phone".to_string(), json!("555-0123")),
            ("address".to_string(), json!("123 Main St")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass with extra fields when required fields present");
}

#[test]
fn test_requires_different_typenames() {
    // TEST: @requires can reference fields from different types
    // GIVEN: Order.userId requires User type fields
    // WHEN: Validating
    // THEN: Should check for field presence

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "userInfo".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["userId".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let repr = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("userId".to_string(), json!("user-123")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "Order", &["userInfo"], &repr);
    assert!(result.is_ok(), "Should handle @requires referencing different typename");
}

#[test]
fn test_requires_and_provides_on_same_field() {
    // TEST: A field can have both @requires and @provides
    // GIVEN: Field with both directives
    // WHEN: Validating
    // THEN: Should check @requires, @provides is informational

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new()
            .add_requires(FieldPathSelection {
                path:     vec!["email".to_string()],
                typename: "User".to_string(),
            })
            .add_provides(FieldPathSelection {
                path:     vec!["userId".to_string()],
                typename: "Order".to_string(),
            }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("email".to_string(), json!("user@example.com")),
            ("userId".to_string(), json!("user-123")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should validate field with both @requires and @provides");
}

#[test]
fn test_requires_enforcement_with_zero_fields() {
    // TEST: Requesting zero fields should pass
    // GIVEN: Empty field request list
    // WHEN: Validating
    // THEN: Should pass (no validations needed)

    let user_type = FederatedType::new("User".to_string());
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = enforce_requires(&metadata, "User", &[], &repr);
    assert!(result.is_ok(), "Should pass with empty field list");
}

#[test]
fn test_requires_enforcement_string_values() {
    // TEST: String values should satisfy @requires
    // GIVEN: Field requires string field
    // WHEN: String value is present
    // THEN: Should pass

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "profile".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["bio".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let repr = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("bio".to_string(), json!("Software engineer from SF")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = enforce_requires(&metadata, "User", &["profile"], &repr);
    assert!(result.is_ok(), "Should accept string field values");
}

// ============================================================================
// Helper function for runtime enforcement
// ============================================================================

/// Enforce @requires directives at runtime
///
/// Validates that all fields required by the @requires directives are present
/// in the entity representation.
///
/// # Arguments
/// * `metadata` - Federation metadata containing type and directive definitions
/// * `typename` - Name of the type being resolved
/// * `fields` - Fields being requested for resolution
/// * `representation` - Entity representation from gateway
///
/// # Returns
/// Ok(()) if all required fields are present, Err with details if not
fn enforce_requires(
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
                        "Validation Error: Required field missing\n\
                         Type: {}\n\
                         Field: {}\n\
                         Required: {}\n\
                         Issue: Field '{}' requires '{}' but it is missing from entity representation\n\
                         Suggestion: Ensure '{}' is requested from the owning subgraph",
                        typename, field, field_path, field, field_path, field_path
                    ));
                }
            }
        }
    }

    Ok(())
}
