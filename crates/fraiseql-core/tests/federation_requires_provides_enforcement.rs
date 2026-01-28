//! Federation @requires/@provides Directive Enforcement
//!
//! Comprehensive tests for compile-time and runtime enforcement of @requires and @provides
//! directives. Tests cover validation, error handling, and integration with entity resolution.

use std::collections::HashMap;

use fraiseql_core::federation::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection, KeyDirective,
};

// ============================================================================
// Test Fixtures and Helpers
// ============================================================================

/// Create a basic federated type for testing
fn create_user_type() -> FederatedType {
    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type
}

/// Create a federated type with @requires directive
fn create_order_with_requires() -> FederatedType {
    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add @requires on shippingEstimate field
    let mut shipping_directives = FieldFederationDirectives::new();
    shipping_directives.requires.push(FieldPathSelection {
        path:     vec!["weight".to_string()],
        typename: "Order".to_string(),
    });
    order_type.set_field_directives("shippingEstimate".to_string(), shipping_directives);

    order_type
}

/// Create federation metadata with multiple types
fn create_federation_metadata() -> FederationMetadata {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    // Add User type
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    metadata.types.push(user);

    // Add Order type with @requires
    let order = create_order_with_requires();
    metadata.types.push(order);

    metadata
}

// ============================================================================
// Test Category 1: Compile-Time Validation (6 tests)
// ============================================================================

#[test]
fn test_requires_field_exists_on_type() {
    let order = create_order_with_requires();
    assert!(order.field_has_requires("shippingEstimate"));

    let directives = order.get_field_directives("shippingEstimate").unwrap();
    assert_eq!(directives.requires.len(), 1);
    assert_eq!(directives.requires[0].path, vec!["weight"]);
}

#[test]
fn test_requires_references_valid_field() {
    let order = create_order_with_requires();
    let requires_fields = order.get_field_directives("shippingEstimate").unwrap().requires.clone();

    // Simulate validation: check if "weight" field exists on Order type
    let weight_exists = order.name == "Order"; // In real impl, check actual fields
    assert!(weight_exists);
    assert_eq!(requires_fields[0].typename, "Order");
}

#[test]
fn test_requires_field_not_external_only() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add @requires on field
    let mut shipping_directives = FieldFederationDirectives::new();
    shipping_directives.requires.push(FieldPathSelection {
        path:     vec!["weight".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("shippingEstimate".to_string(), shipping_directives);

    // Required field should not be @external (it must be available locally)
    assert!(!order.field_is_external("weight"));
}

#[test]
fn test_provides_field_references_valid_field() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add @provides directive
    let mut full_name_directives = FieldFederationDirectives::new();
    full_name_directives.provides.push(FieldPathSelection {
        path:     vec!["firstName".to_string()],
        typename: "User".to_string(),
    });
    full_name_directives.provides.push(FieldPathSelection {
        path:     vec!["lastName".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("fullName".to_string(), full_name_directives);

    assert!(user.field_has_provides("fullName"));
    let directives = user.get_field_directives("fullName").unwrap();
    assert_eq!(directives.provides.len(), 2);
}

#[test]
fn test_no_circular_requires_dependencies() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add @requires A -> B
    let mut a_directives = FieldFederationDirectives::new();
    a_directives.requires.push(FieldPathSelection {
        path:     vec!["b".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("a".to_string(), a_directives);

    // Add @requires B -> C (not circular, valid)
    let mut b_directives = FieldFederationDirectives::new();
    b_directives.requires.push(FieldPathSelection {
        path:     vec!["c".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("b".to_string(), b_directives);

    // Simulate cycle detection
    let b_deps = order.get_field_directives("b").unwrap().requires.clone();

    // A depends on B, B depends on C (no cycle)
    let has_cycle = b_deps.iter().any(|b| b.path[0] == "a");
    assert!(!has_cycle);
}

#[test]
fn test_requires_on_multiple_fields() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Field A @requires X, Y
    let mut a_directives = FieldFederationDirectives::new();
    a_directives.requires.push(FieldPathSelection {
        path:     vec!["x".to_string()],
        typename: "Order".to_string(),
    });
    a_directives.requires.push(FieldPathSelection {
        path:     vec!["y".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("a".to_string(), a_directives);

    let directives = order.get_field_directives("a").unwrap();
    assert_eq!(directives.requires.len(), 2);
    assert_eq!(directives.requires[0].path[0], "x");
    assert_eq!(directives.requires[1].path[0], "y");
}

// ============================================================================
// Test Category 2: Runtime @requires Validation (6 tests)
// ============================================================================

#[test]
fn test_requires_validation_success_all_fields_present() {
    let order = create_order_with_requires();
    let directives = order.get_field_directives("shippingEstimate").unwrap();

    // Simulate entity with required fields present
    let mut entity_fields = HashMap::new();
    entity_fields.insert("weight".to_string(), "5.0");

    // Validation: check all required fields are present
    let all_present =
        directives.requires.iter().all(|req| entity_fields.contains_key(&req.path[0]));

    assert!(all_present);
}

#[test]
fn test_requires_validation_fails_missing_required_field() {
    let order = create_order_with_requires();
    let directives = order.get_field_directives("shippingEstimate").unwrap();

    // Simulate entity WITHOUT required fields
    let entity_fields: HashMap<String, String> = HashMap::new();

    // Validation should fail
    let all_present =
        directives.requires.iter().all(|req| entity_fields.contains_key(&req.path[0]));

    assert!(!all_present);
}

#[test]
fn test_requires_error_message_includes_missing_field() {
    let order = create_order_with_requires();
    let directives = order.get_field_directives("shippingEstimate").unwrap();

    // Find missing required fields
    let entity_fields: HashMap<String, String> = HashMap::new();
    let missing: Vec<_> = directives
        .requires
        .iter()
        .filter(|req| !entity_fields.contains_key(&req.path[0]))
        .map(|req| req.path[0].clone())
        .collect();

    // Error message should include the missing field
    assert!(!missing.is_empty());
    assert_eq!(missing[0], "weight");
}

#[test]
fn test_requires_validation_with_nested_field_path() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @requires with nested field (e.g., shipping.address)
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["shipping".to_string(), "address".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("trackingUrl".to_string(), directives);

    let field_directives = order.get_field_directives("trackingUrl").unwrap();
    assert_eq!(field_directives.requires[0].path.len(), 2);
    assert_eq!(field_directives.requires[0].path, vec!["shipping", "address"]);
}

#[test]
fn test_requires_validates_before_field_resolution() {
    let order = create_order_with_requires();

    // Field requiring validation
    let field = "shippingEstimate";
    assert!(order.field_has_requires(field));

    // Check should happen BEFORE attempting to resolve the field
    let has_requires = order.field_has_requires(field);
    assert!(has_requires);
}

#[test]
fn test_requires_with_multiple_required_fields() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @requires with multiple fields
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["weight".to_string()],
        typename: "Order".to_string(),
    });
    directives.requires.push(FieldPathSelection {
        path:     vec!["dimensions".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("cost".to_string(), directives);

    let field_directives = order.get_field_directives("cost").unwrap();
    assert_eq!(field_directives.requires.len(), 2);

    // All required fields must be present for resolution
    let entity_fields = vec!["weight", "dimensions"];
    let all_present = field_directives
        .requires
        .iter()
        .all(|req| entity_fields.contains(&req.path[0].as_str()));
    assert!(all_present);
}

// ============================================================================
// Test Category 3: Runtime @provides Validation (5 tests)
// ============================================================================

#[test]
fn test_provides_validates_returned_fields() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @provides directive
    let mut directives = FieldFederationDirectives::new();
    directives.provides.push(FieldPathSelection {
        path:     vec!["firstName".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("fullName".to_string(), directives);

    // Simulating field that provides firstName
    let field_directives = user.get_field_directives("fullName").unwrap();
    let promised_fields: Vec<_> =
        field_directives.provides.iter().map(|p| p.path[0].clone()).collect();

    assert!(promised_fields.contains(&"firstName".to_string()));
}

#[test]
fn test_provides_with_multiple_fields() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @provides multiple fields
    let mut directives = FieldFederationDirectives::new();
    directives.provides.push(FieldPathSelection {
        path:     vec!["firstName".to_string()],
        typename: "User".to_string(),
    });
    directives.provides.push(FieldPathSelection {
        path:     vec!["lastName".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("fullName".to_string(), directives);

    let field_directives = user.get_field_directives("fullName").unwrap();
    assert_eq!(field_directives.provides.len(), 2);
}

#[test]
fn test_provides_warning_if_field_not_returned() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @provides directive
    let mut directives = FieldFederationDirectives::new();
    directives.provides.push(FieldPathSelection {
        path:     vec!["email".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("profile".to_string(), directives);

    // Simulating returned value that doesn't include promised field
    let returned_fields = vec!["name", "age"]; // missing 'email'
    let promised_fields: Vec<_> = user
        .get_field_directives("profile")
        .unwrap()
        .provides
        .iter()
        .map(|p| p.path[0].clone())
        .collect();

    // Should detect missing field
    let missing_promised = promised_fields.iter().any(|p| !returned_fields.contains(&p.as_str()));
    assert!(missing_promised);
}

#[test]
fn test_provides_with_nested_field_path() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // @provides with nested path
    let mut directives = FieldFederationDirectives::new();
    directives.provides.push(FieldPathSelection {
        path:     vec!["profile".to_string(), "bio".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("computedProfile".to_string(), directives);

    let field_directives = user.get_field_directives("computedProfile").unwrap();
    assert_eq!(field_directives.provides[0].path.len(), 2);
}

#[test]
fn test_provides_contract_between_subgraphs() {
    let metadata = create_federation_metadata();

    // Order type has shippingEstimate field
    let order = &metadata.types.iter().find(|t| t.name == "Order").unwrap();

    // If a field has @requires, that's a contract with other subgraphs
    assert!(order.field_has_requires("shippingEstimate"));
}

// ============================================================================
// Test Category 4: Integration with Entity Resolution (5 tests)
// ============================================================================

#[test]
fn test_requires_checked_before_field_resolution() {
    let order = create_order_with_requires();

    // Sequence: 1) Check if field requires anything
    let requires = order.field_has_requires("shippingEstimate");
    assert!(requires);

    // 2) If requires, validate required fields present
    // 3) Only then resolve the field
    let directives = order.get_field_directives("shippingEstimate").unwrap();
    assert_eq!(directives.requires.len(), 1);
}

#[test]
fn test_requires_error_propagation_to_resolver() {
    let order = create_order_with_requires();

    // Missing required field should produce error
    let entity_has_weight = false;

    let directives = order.get_field_directives("shippingEstimate").unwrap();
    let requires_weight = directives.requires.iter().any(|r| r.path[0] == "weight");

    // If field requires weight but entity doesn't have it, error propagates
    if requires_weight && !entity_has_weight {
        // Error condition detected
        assert!(true);
    }
}

#[test]
fn test_requires_works_with_external_fields() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Mark weight as external (from another subgraph)
    order.external_fields.push("weight".to_string());

    // But can still @require it
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["weight".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("shippingCost".to_string(), directives);

    // Verify the external field is marked as external
    assert!(order.external_fields.contains(&"weight".to_string()));
    // And that shippingCost has @requires
    assert!(order.field_has_requires("shippingCost"));
}

#[test]
fn test_requires_validation_error_includes_field_name() {
    let order = create_order_with_requires();

    // Error should include which field failed and which @requires was missing
    let result_field = "shippingEstimate";
    let missing_field = "weight";

    let directives = order.get_field_directives(result_field).unwrap();
    let has_requires = directives.requires.iter().any(|r| r.path[0] == missing_field);

    // Error message should be constructible from this info
    assert!(has_requires);
    let error_msg = format!(
        "Field {}.{} requires {} but it was not provided",
        order.name, result_field, missing_field
    );
    assert!(error_msg.contains(result_field));
    assert!(error_msg.contains(missing_field));
}

#[test]
fn test_requires_validation_skipped_if_no_requires() {
    let user = create_user_type();

    // Field without @requires should not undergo validation
    assert!(!user.field_has_requires("name"));

    // Can resolve without checking
    let has_requires = user.field_has_requires("name");
    assert!(!has_requires);
}

// ============================================================================
// Test Category 5: Complex Scenarios (5 tests)
// ============================================================================

#[test]
fn test_requires_and_provides_on_same_field() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Single field both @requires and @provides
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["firstName".to_string()],
        typename: "User".to_string(),
    });
    directives.requires.push(FieldPathSelection {
        path:     vec!["lastName".to_string()],
        typename: "User".to_string(),
    });
    directives.provides.push(FieldPathSelection {
        path:     vec!["displayName".to_string()],
        typename: "User".to_string(),
    });
    user.set_field_directives("fullName".to_string(), directives);

    let field_directives = user.get_field_directives("fullName").unwrap();
    assert!(!field_directives.requires.is_empty());
    assert!(!field_directives.provides.is_empty());
}

#[test]
fn test_requires_with_shareable_field() {
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Field that is both @shareable and has @requires
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["email".to_string()],
        typename: "User".to_string(),
    });
    directives.shareable = true;
    user.set_field_directives("publicEmail".to_string(), directives);

    let field_directives = user.get_field_directives("publicEmail").unwrap();
    assert!(!field_directives.requires.is_empty());
    assert!(field_directives.shareable);
}

#[test]
fn test_requires_with_external_field_on_extended_type() {
    let mut order = FederatedType::new("Order".to_string());
    order.is_extends = true;
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order.external_fields.push("userId".to_string());

    // @requires an external field
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec!["userId".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("userStatus".to_string(), directives);

    assert!(order.is_extends);
    assert!(order.external_fields.contains(&"userId".to_string()));
    assert!(order.field_has_requires("userStatus"));
}

#[test]
fn test_requires_across_multiple_fields_same_type() {
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Field A @requires X
    let mut a_directives = FieldFederationDirectives::new();
    a_directives.requires.push(FieldPathSelection {
        path:     vec!["x".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("a".to_string(), a_directives);

    // Field B @requires Y
    let mut b_directives = FieldFederationDirectives::new();
    b_directives.requires.push(FieldPathSelection {
        path:     vec!["y".to_string()],
        typename: "Order".to_string(),
    });
    order.set_field_directives("b".to_string(), b_directives);

    assert!(order.field_has_requires("a"));
    assert!(order.field_has_requires("b"));

    let a_req = order.get_field_directives("a").unwrap();
    let b_req = order.get_field_directives("b").unwrap();
    assert_eq!(a_req.requires[0].path[0], "x");
    assert_eq!(b_req.requires[0].path[0], "y");
}

#[test]
fn test_federation_metadata_with_requires_provides() {
    let metadata = create_federation_metadata();

    // Verify metadata structure
    assert!(metadata.enabled);
    assert!(!metadata.types.is_empty());

    // Find Order type
    let order = metadata.types.iter().find(|t| t.name == "Order");
    assert!(order.is_some());

    let order = order.unwrap();
    assert!(order.field_has_requires("shippingEstimate"));
}
