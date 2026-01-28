//! Phase 1, Cycle 1: Field-Level Directive Metadata Tests
//!
//! These tests define the expected behavior for storing and accessing
//! field-level federation directives (@requires, @provides, @shareable).
//!
//! RED PHASE: These tests are expected to FAIL until FieldFederationDirectives
//! struct and field_directives HashMap are added to FederatedType.

use fraiseql_core::federation::types::{FederatedType, FieldFederationDirectives, FieldPathSelection, KeyDirective};

// ============================================================================
// Test: Basic Field Directive Storage
// ============================================================================

#[test]
fn test_field_directive_requires_storage() {
    // TEST: FederatedType should store @requires directives for fields
    // GIVEN: A FederatedType with a field that has @requires directive
    // WHEN: We access the field's directives
    // THEN: @requires should be available and populated

    let mut user_type = FederatedType {
        name: "User".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: false,
        external_fields: vec![],
        shareable_fields: vec![],
        // NEW: field_directives will be added in GREEN phase
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate adding a @requires directive to the "orders" field
    let mut orders_directives = FieldFederationDirectives::default();
    orders_directives.requires.push(FieldPathSelection {
        path: vec!["email".to_string()],
        typename: "User".to_string(),
    });

    user_type
        .field_directives
        .insert("orders".to_string(), orders_directives);

    // Verify the directive is stored
    let directives = user_type
        .field_directives
        .get("orders")
        .expect("orders field directives should exist");
    assert!(!directives.requires.is_empty(), "requires should not be empty");
    assert_eq!(
        directives.requires[0].path,
        vec!["email".to_string()],
        "requires path should be [email]"
    );
}

#[test]
fn test_field_directive_provides_storage() {
    // TEST: FederatedType should store @provides directives for fields
    // GIVEN: A FederatedType with a field that has @provides directive
    // WHEN: We access the field's directives
    // THEN: @provides should be available and populated

    let mut order_type = FederatedType {
        name: "Order".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: true,
        external_fields: vec!["total".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate adding a @provides directive to the "shippingEstimate" field
    let mut shipping_directives = FieldFederationDirectives::default();
    shipping_directives.provides.push(FieldPathSelection {
        path: vec!["weight".to_string()],
        typename: "Product".to_string(),
    });

    order_type
        .field_directives
        .insert("shippingEstimate".to_string(), shipping_directives);

    // Verify the directive is stored
    let directives = order_type
        .field_directives
        .get("shippingEstimate")
        .expect("shippingEstimate field directives should exist");
    assert!(!directives.provides.is_empty(), "provides should not be empty");
    assert_eq!(
        directives.provides[0].path,
        vec!["weight".to_string()],
        "provides path should be [weight]"
    );
}

#[test]
fn test_field_directive_shareable_flag() {
    // TEST: FederatedType should store @shareable directive for fields
    // GIVEN: A FederatedType with a field that has @shareable directive
    // WHEN: We access the field's directives
    // THEN: shareable flag should be true

    let mut product_type = FederatedType {
        name: "Product".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: false,
        external_fields: vec![],
        shareable_fields: vec!["name".to_string()],
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate adding a @shareable directive to the "name" field
    let mut name_directives = FieldFederationDirectives::default();
    name_directives.shareable = true;

    product_type
        .field_directives
        .insert("name".to_string(), name_directives);

    // Verify the directive is stored
    let directives = product_type
        .field_directives
        .get("name")
        .expect("name field directives should exist");
    assert!(directives.shareable, "@shareable should be true for name field");
}

#[test]
fn test_field_directive_external_flag() {
    // TEST: FederatedType should store @external directive for fields
    // GIVEN: A FederatedType with a field that has @external directive
    // WHEN: We access the field's directives
    // THEN: external flag should be true

    let mut order_type = FederatedType {
        name: "Order".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: true,
        external_fields: vec!["total".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate adding an @external directive to the "total" field
    let mut total_directives = FieldFederationDirectives::default();
    total_directives.external = true;

    order_type
        .field_directives
        .insert("total".to_string(), total_directives);

    // Verify the directive is stored
    let directives = order_type
        .field_directives
        .get("total")
        .expect("total field directives should exist");
    assert!(directives.external, "@external should be true for total field");
}

// ============================================================================
// Test: Nested Field Selection Paths
// ============================================================================

#[test]
fn test_field_directive_nested_requires_path() {
    // TEST: @requires should support nested paths like "user.email"
    // GIVEN: A field with @requires(fields: "user.email")
    // WHEN: We store and access the directive
    // THEN: Path should be parsed as ["user", "email"]

    let mut order_type = FederatedType {
        name: "Order".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: true,
        external_fields: vec![],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate adding a @requires directive with nested path
    let mut pricing_directives = FieldFederationDirectives::default();
    pricing_directives.requires.push(FieldPathSelection {
        path: vec!["user".to_string(), "email".to_string()],
        typename: "User".to_string(),
    });

    order_type
        .field_directives
        .insert("pricingInfo".to_string(), pricing_directives);

    // Verify the nested path is stored correctly
    let directives = order_type
        .field_directives
        .get("pricingInfo")
        .expect("pricingInfo field directives should exist");
    assert_eq!(directives.requires[0].path.len(), 2, "path should have 2 components");
    assert_eq!(directives.requires[0].path[0], "user");
    assert_eq!(directives.requires[0].path[1], "email");
}

// ============================================================================
// Test: Multiple Directives on Same Field
// ============================================================================

#[test]
fn test_field_directive_multiple_directives_on_field() {
    // TEST: A field can have multiple directives (@requires, @provides, @shareable)
    // GIVEN: A field with multiple directives
    // WHEN: We access the field's directives
    // THEN: All directives should be stored and accessible

    let mut order_type = FederatedType {
        name: "Order".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: true,
        external_fields: vec!["items".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Simulate field with @requires + @provides + @shareable
    let mut estimate_directives = FieldFederationDirectives::default();
    estimate_directives.requires.push(FieldPathSelection {
        path: vec!["items".to_string()],
        typename: "Order".to_string(),
    });
    estimate_directives.provides.push(FieldPathSelection {
        path: vec!["weight".to_string()],
        typename: "Shipment".to_string(),
    });
    estimate_directives.shareable = true;

    order_type
        .field_directives
        .insert("shippingEstimate".to_string(), estimate_directives);

    // Verify all directives are stored
    let directives = order_type
        .field_directives
        .get("shippingEstimate")
        .expect("shippingEstimate field directives should exist");
    assert!(!directives.requires.is_empty(), "@requires should be present");
    assert!(!directives.provides.is_empty(), "@provides should be present");
    assert!(directives.shareable, "@shareable should be present");
}

// ============================================================================
// Test: Multiple Fields with Different Directives
// ============================================================================

#[test]
fn test_field_directives_multiple_fields() {
    // TEST: Different fields can have different directives
    // GIVEN: A type with multiple fields, each with different directives
    // WHEN: We access directives for each field
    // THEN: Each field's directives should be independent

    let mut user_type = FederatedType {
        name: "User".to_string(),
        keys: vec![KeyDirective {
            fields: vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends: false,
        external_fields: vec![],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Field 1: orders with @requires
    let mut orders_directives = FieldFederationDirectives::default();
    orders_directives.requires.push(FieldPathSelection {
        path: vec!["email".to_string()],
        typename: "User".to_string(),
    });
    user_type
        .field_directives
        .insert("orders".to_string(), orders_directives);

    // Field 2: profile with @shareable
    let mut profile_directives = FieldFederationDirectives::default();
    profile_directives.shareable = true;
    user_type
        .field_directives
        .insert("profile".to_string(), profile_directives);

    // Verify both fields have their own directives
    assert_eq!(user_type.field_directives.len(), 2, "should have 2 fields");

    let orders_dir = user_type.field_directives.get("orders").unwrap();
    assert!(!orders_dir.requires.is_empty(), "orders should have @requires");
    assert!(!orders_dir.shareable, "orders should not have @shareable");

    let profile_dir = user_type.field_directives.get("profile").unwrap();
    assert!(profile_dir.requires.is_empty(), "profile should not have @requires");
    assert!(profile_dir.shareable, "profile should have @shareable");
}
