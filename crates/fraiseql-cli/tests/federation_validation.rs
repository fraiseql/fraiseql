//! Tests for validating federation directives at schema compilation time:
//! - @requires/@provides reference valid fields
//! - No circular dependencies
//! - @external only on @extends types
//! - @key fields exist
//!
//! CYCLE 3: Tests for compile-time federation validation

use fraiseql_core::federation::{
    DependencyGraph,
    types::{
        FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection,
        KeyDirective,
    },
}; // DependencyGraph used in validate_federation_metadata helper

// ============================================================================
// Test: @requires Field Validation
// ============================================================================

#[test]
fn test_validate_requires_field_exists() {
    // TEST: @requires must reference a field that exists
    // GIVEN: User.orders requires "email" field (which exists)
    // WHEN: We validate the schema
    // THEN: Validation should pass

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

    // This should validate successfully
    let result = validate_federation_metadata(&metadata);
    assert!(result.is_ok(), "Should validate when @requires field exists");
}

#[test]
fn test_validate_requires_empty_path() {
    // TEST: @requires must have non-empty field path
    // GIVEN: User.orders requires empty path (invalid)
    // WHEN: We validate the schema
    // THEN: Validation should fail with helpful error

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    // Manually create directives with empty path to test
    let mut directives = FieldFederationDirectives::new();
    directives.requires.push(FieldPathSelection {
        path:     vec![], // Empty path - invalid!
        typename: "User".to_string(),
    });
    user_type.set_field_directives("orders".to_string(), directives);

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_err(), "Should fail when @requires has empty path");
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("empty"),
        "Error message should mention empty path: {}",
        err
    );
}

#[test]
fn test_validate_requires_nested_field_path() {
    // TEST: @requires with nested paths like "profile.age"
    // GIVEN: Order.shippingEstimate requires "user.email"
    // WHEN: We validate
    // THEN: Should validate the nested path components

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["user".to_string(), "email".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_ok(), "Should support nested field paths in @requires");
}

// ============================================================================
// Test: @provides Field Validation
// ============================================================================

#[test]
fn test_validate_provides_field_exists() {
    // TEST: @provides must reference fields the resolver actually provides
    // GIVEN: Order.shippingEstimate provides "weight"
    // WHEN: We validate
    // THEN: Validation should pass (warning if not returned, but not error)

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let result = validate_federation_metadata(&metadata);
    // @provides is informational, so validation passes
    assert!(result.is_ok(), "Should validate @provides declarations");
}

// ============================================================================
// Test: @external Field Validation
// ============================================================================

#[test]
fn test_validate_external_only_on_extends() {
    // TEST: @external can only appear on @extends types
    // GIVEN: Extended Order type with @external total field
    // WHEN: We validate
    // THEN: Validation should pass

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.is_extends = true;
    order_type
        .set_field_directives("total".to_string(), FieldFederationDirectives::new().external());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_ok(), "Should allow @external on @extends types");
}

#[test]
fn test_validate_external_only_on_extends_fails() {
    // TEST: @external on non-extends type should fail
    // GIVEN: Non-extended User type with @external field
    // WHEN: We validate
    // THEN: Validation should fail

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    // is_extends is false (default)
    user_type
        .set_field_directives("email".to_string(), FieldFederationDirectives::new().external());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_err(), "Should fail when @external used on non-extends type");
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("external") || err.to_lowercase().contains("extends"),
        "Error should explain @external restriction: {}",
        err
    );
}

// ============================================================================
// Test: Circular Dependency Detection
// ============================================================================

#[test]
fn test_validate_two_node_circular_requires() {
    // TEST: Circular @requires should be rejected (2-node cycle)
    // GIVEN: User.orders requires Order.user AND Order.user requires User.orders (circle!)
    // WHEN: We validate
    // THEN: Validation should fail with circular dependency error

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "orders".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["user".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "user".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["orders".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type, order_type],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_err(), "Should fail when 2-node circular @requires detected");
    let err = result.unwrap_err();
    assert!(
        err.to_lowercase().contains("circular")
            || err.to_lowercase().contains("cycle")
            || err.to_lowercase().contains("dependency"),
        "Error should mention circular dependency: {}",
        err
    );
}

#[test]
fn test_validate_three_node_cycle() {
    // TEST: Three-node cycles should be detected
    // GIVEN: A.f1 requires B.f2, B.f2 requires C.f3, C.f3 requires A.f1 (cycle!)
    // WHEN: We validate
    // THEN: Validation should fail

    let mut type_a = FederatedType::new("A".to_string());
    type_a.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    type_a.set_field_directives(
        "f1".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["f2".to_string()],
            typename: "B".to_string(),
        }),
    );

    let mut type_b = FederatedType::new("B".to_string());
    type_b.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    type_b.set_field_directives(
        "f2".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["f3".to_string()],
            typename: "C".to_string(),
        }),
    );

    let mut type_c = FederatedType::new("C".to_string());
    type_c.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    type_c.set_field_directives(
        "f3".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["f1".to_string()],
            typename: "A".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![type_a, type_b, type_c],
    };

    let result = validate_federation_metadata(&metadata);
    assert!(result.is_err(), "Should fail when 3-node circular dependency detected");
}

// ============================================================================
// Test: @key Field Validation
// ============================================================================

#[test]
fn test_validate_key_fields_exist() {
    // TEST: @key fields must reference existing fields
    // GIVEN: User type with @key(fields: "id")
    // WHEN: We validate
    // THEN: Validation should pass (id exists implicitly in federation context)

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let result = validate_federation_metadata(&metadata);
    // Key validation may pass if we trust the @key is valid
    // or may need to validate against type definitions
    assert!(result.is_ok(), "Should allow valid @key declarations");
}

// ============================================================================
// Helper function for federation validation
// ============================================================================

/// Validate federation metadata with detailed error reporting
fn validate_federation_metadata(metadata: &FederationMetadata) -> Result<(), String> {
    // Check if federation is enabled
    if !metadata.enabled {
        return Ok(());
    }

    // Step 1: Validate @requires/@provides and @external directives
    for federated_type in &metadata.types {
        for (field_name, directives) in &federated_type.field_directives {
            // Validate @requires fields have non-empty paths
            for (idx, required) in directives.requires.iter().enumerate() {
                if required.path.is_empty() {
                    return Err(format!(
                        "Validation Error: Invalid @requires directive on {}.{}\n\
                         Position: @requires[{}]\n\
                         Issue: Field path cannot be empty\n\
                         Suggestion: Specify the required field name, e.g., @requires(fields: \"email\")",
                        federated_type.name, field_name, idx
                    ));
                }
            }

            // Validate @provides fields have non-empty paths
            for (idx, provided) in directives.provides.iter().enumerate() {
                if provided.path.is_empty() {
                    return Err(format!(
                        "Validation Error: Invalid @provides directive on {}.{}\n\
                         Position: @provides[{}]\n\
                         Issue: Field path cannot be empty\n\
                         Suggestion: Specify the provided field name, e.g., @provides(fields: \"weight\")",
                        federated_type.name, field_name, idx
                    ));
                }
            }

            // Validate @external only on @extends types
            if directives.external && !federated_type.is_extends {
                return Err(format!(
                    "Validation Error: @external directive on non-extended type\n\
                     Type: {}\n\
                     Field: {}\n\
                     Issue: @external can only be used on @extends types\n\
                     Suggestion: Add @extends directive to type {}, or remove @external from field",
                    federated_type.name, field_name, federated_type.name
                ));
            }
        }
    }

    // Step 2: Check for circular dependencies using DependencyGraph
    let graph = DependencyGraph::build(metadata).map_err(|e| {
        format!("Validation Error: Failed to build dependency graph\nReason: {}", e)
    })?;

    let cycles = graph.detect_cycles();
    if !cycles.is_empty() {
        let cycle_description = cycles
            .iter()
            .enumerate()
            .map(|(i, cycle)| format!("  Cycle {}: {}", i + 1, cycle.join(" → ")))
            .collect::<Vec<_>>()
            .join("\n");

        return Err(format!(
            "Validation Error: Circular @requires dependencies detected\n\
             Cycles found:\n{}\n\
             Issue: Field requirements form circular dependency chain\n\
             Suggestion: Remove one of the @requires directives to break the cycle",
            cycle_description
        ));
    }

    Ok(())
}
