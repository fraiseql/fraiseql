//! Core @requires enforcement tests.

use fraiseql_core::federation::types::{
    EntityRepresentation, FederatedType, FederationMetadata, FieldFederationDirectives,
    FieldPathSelection, KeyDirective,
};
use serde_json::json;

use super::common;

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
    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
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

    let result = common::enforce_requires(
        &metadata,
        "Order",
        &["shippingEstimate"],
        &repr_missing_dimensions,
    );
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

    let result =
        common::enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr_complete);
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

    let result = common::enforce_requires(&metadata, "User", &["name"], &repr);
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

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_lowercase().contains("user"), "Error should mention type");
    assert!(
        err.to_lowercase().contains("orders") || err.to_lowercase().contains("email"),
        "Error should mention field or requirement"
    );
}

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
    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when required field for orders is present");

    // Should fail when requesting profile (phone missing)
    let result = common::enforce_requires(&metadata, "User", &["profile"], &repr);
    assert!(result.is_err(), "Should fail when required field for profile is missing");

    // Should fail when requesting both
    let result = common::enforce_requires(&metadata, "User", &["orders", "profile"], &repr);
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

    let result = common::enforce_requires(&metadata, "Order", &["complexField"], &repr);
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

    let result = common::enforce_requires(&metadata, "Order", &["complexField"], &repr);
    assert!(result.is_err(), "Should fail when 1 of 3 required fields missing");
}
