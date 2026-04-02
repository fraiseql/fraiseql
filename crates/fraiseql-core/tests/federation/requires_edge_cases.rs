//! Edge cases for @requires enforcement tests.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::iter_on_single_items)] // Reason: test uses single-element iter for pattern uniformity
#![allow(clippy::default_trait_access)] // Reason: test setup uses Default::default() for brevity
use fraiseql_core::federation::types::{
    EntityRepresentation, FederatedType, FederationMetadata, FieldFederationDirectives,
    FieldPathSelection, KeyDirective,
};
use serde_json::json;

use super::common;

#[test]
fn test_requires_with_empty_representation() {
    // TEST: Empty representation should fail if any field has @requires
    // GIVEN: Empty entity representation
    // WHEN: Field with @requires is requested
    // THEN: Should fail

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type],
    };

    let repr = EntityRepresentation {
        typename: "User".to_string(),
        key_fields: Default::default(),
        all_fields: Default::default(),
    };

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type],
    };

    let repr = EntityRepresentation {
        typename: "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type],
    };

    // Note: This test assumes case-sensitive matching
    let repr = EntityRepresentation {
        typename: "User".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("Email".to_string(), json!("user@example.com")), // Wrong case
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["shipping_address_2".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![order_type],
    };

    let repr = EntityRepresentation {
        typename: "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("123")),
            ("shipping_address_2".to_string(), json!("123 Main St")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = common::enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr);
    assert!(result.is_ok(), "Should accept field with underscores and numbers");
}

#[test]
fn test_requires_validation_error_includes_typename() {
    // TEST: Error message should include the typename
    // GIVEN: Validation failure for specific type
    // WHEN: Error is returned
    // THEN: Error message includes typename

    let mut product_type = FederatedType::new("Product".to_string());
    product_type.keys.push(KeyDirective {
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    product_type.set_field_directives(
        "discount".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["price".to_string()],
            typename: "Product".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![product_type],
    };

    let repr = EntityRepresentation {
        typename: "Product".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("789"))].iter().cloned().collect(),
    };

    let result = common::enforce_requires(&metadata, "Product", &["discount"], &repr);
    assert!(result.is_err(), "expected Err for missing 'price' field, got: {result:?}");
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "recommendation".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["interests".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type],
    };

    let repr = EntityRepresentation {
        typename: "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = common::enforce_requires(&metadata, "User", &["recommendation"], &repr);
    assert!(result.is_err(), "expected Err for missing 'interests' field, got: {result:?}");
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["email".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type],
    };

    // Representation with extra fields beyond what's required
    let repr = EntityRepresentation {
        typename: "User".to_string(),
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

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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
        fields: vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "userInfo".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path: vec!["userId".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![order_type],
    };

    let repr = EntityRepresentation {
        typename: "Order".to_string(),
        key_fields: Default::default(),
        all_fields: [
            ("id".to_string(), json!("456")),
            ("userId".to_string(), json!("user-123")),
        ]
        .iter()
        .cloned()
        .collect(),
    };

    let result = common::enforce_requires(&metadata, "Order", &["userInfo"], &repr);
    assert!(result.is_ok(), "Should handle @requires referencing different typename");
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
        types: vec![user_type],
    };

    let repr = EntityRepresentation {
        typename: "User".to_string(),
        key_fields: Default::default(),
        all_fields: [("id".to_string(), json!("123"))].iter().cloned().collect(),
    };

    let result = common::enforce_requires(&metadata, "User", &[], &repr);
    assert!(result.is_ok(), "Should pass with empty field list");
}
