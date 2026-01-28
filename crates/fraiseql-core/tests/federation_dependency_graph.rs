//! Phase 1, Cycle 2: Dependency Graph & Cycle Detection Tests
//!
//! Tests for building dependency graphs from @requires directives and detecting
//! circular dependencies.
//!
//! RED PHASE: These tests are expected to FAIL until DependencyGraph is implemented

use fraiseql_core::federation::{
    DependencyGraph,
    types::{
        FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection,
        KeyDirective,
    },
};

// ============================================================================
// Test: Dependency Graph Build
// ============================================================================

#[test]
fn test_dependency_graph_build() {
    // TEST: Build dependency graph from federation metadata
    // GIVEN: Two types with @requires directives
    // WHEN: We build a graph
    // THEN: Graph should contain those nodes

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

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "items".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["total".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type, order_type],
    };

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");

    assert_eq!(graph.node_count(), 2, "Should have 2 nodes");
    assert!(graph.has_node("User.orders"), "Should have User.orders");
    assert!(graph.has_node("Order.items"), "Should have Order.items");
}

#[test]
fn test_dependency_graph_with_no_requires() {
    // TEST: Graph should be empty when no @requires directives exist
    // GIVEN: Types with no @requires
    // WHEN: Build graph
    // THEN: Graph should be empty

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType::new("User".to_string()),
            FederatedType::new("Order".to_string()),
        ],
    };

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");

    assert_eq!(graph.node_count(), 0, "Should have 0 nodes");
}

// ============================================================================
// Test: Cycle Detection - No Cycles
// ============================================================================

#[test]
fn test_cycle_detection_no_cycles() {
    // TEST: Valid DAG should report no cycles
    // GIVEN: Linear dependency (no cycles)
    // WHEN: Detect cycles
    // THEN: Cycles should be empty

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

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");
    let cycles = graph.detect_cycles();

    assert!(cycles.is_empty(), "Should have no cycles");
}

#[test]
fn test_cycle_detection_simple_cycle() {
    // TEST: Simple circular dependency should be detected
    // GIVEN: User.orders requires User.email, User.email requires User.orders (2-node cycle)
    // WHEN: Detect cycles
    // THEN: Should find the cycle

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
        "email".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["orders".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");
    let cycles = graph.detect_cycles();

    assert!(!cycles.is_empty(), "Should detect circular dependency");
}

#[test]
fn test_cycle_detection_longer_cycle() {
    // TEST: Three-node cycle should be detected
    // GIVEN: A.f1 → B.f2 → C.f3 → A.f1 cycle
    // WHEN: Detect cycles
    // THEN: Should find the 3-node cycle

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

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");
    let cycles = graph.detect_cycles();

    assert!(!cycles.is_empty(), "Should detect 3-node cycle");
}

// ============================================================================
// Test: Topological Sort
// ============================================================================

#[test]
fn test_topological_sort_valid_graph() {
    // TEST: Topological sort on valid DAG
    // GIVEN: User.orders requires email
    // WHEN: Topologically sort
    // THEN: Should return valid order

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

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");
    let order = graph.topological_sort().expect("Should sort successfully");

    assert!(!order.is_empty(), "Should return sorted order");
}

#[test]
fn test_topological_sort_fails_on_cycle() {
    // TEST: Topological sort should fail when cycles exist
    // GIVEN: A.f → B.g → A.f cycle
    // WHEN: Try to topologically sort
    // THEN: Should return an error

    let mut type_a = FederatedType::new("A".to_string());
    type_a.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    type_a.set_field_directives(
        "f".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["g".to_string()],
            typename: "B".to_string(),
        }),
    );

    let mut type_b = FederatedType::new("B".to_string());
    type_b.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    type_b.set_field_directives(
        "g".to_string(),
        FieldFederationDirectives::new().add_requires(FieldPathSelection {
            path:     vec!["f".to_string()],
            typename: "A".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![type_a, type_b],
    };

    let graph = DependencyGraph::build(&metadata).expect("Should build graph");
    let result = graph.topological_sort();

    assert!(result.is_err(), "Should fail when cycles exist");
}
