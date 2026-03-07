//! @requires with different data types tests.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::default_trait_access)] // Reason: test setup uses Default::default() for brevity
use fraiseql_core::federation::types::{
    EntityRepresentation, FederatedType, FederationMetadata, FieldFederationDirectives,
    FieldPathSelection, KeyDirective,
};
use serde_json::json;

use super::common;

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

    let result = common::enforce_requires(&metadata, "Order", &["shippingEstimate"], &repr);
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

    let result = common::enforce_requires(&metadata, "User", &["premiumFeature"], &repr);
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

    let result = common::enforce_requires(&metadata, "User", &["nickname"], &repr);
    assert!(result.is_ok(), "Should accept field with null value as present");
}

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

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should pass when @provides fields are present");
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

    let result = common::enforce_requires(&metadata, "User", &["totalOrders"], &repr);
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

    let result = common::enforce_requires(&metadata, "User", &["address"], &repr);
    assert!(result.is_ok(), "Should accept object field");
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

    let result = common::enforce_requires(&metadata, "User", &["profile"], &repr);
    assert!(result.is_ok(), "Should accept string field values");
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

    let result = common::enforce_requires(&metadata, "User", &["orders"], &repr);
    assert!(result.is_ok(), "Should validate field with both @requires and @provides");
}
