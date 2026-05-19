#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_dependency_node_creation() {
    let node = DependencyNode {
        id: "User.orders".to_string(),
        requires: vec![FieldPathSelection {
            path: vec!["email".to_string()],
            typename: "User".to_string(),
        }],
    };

    assert_eq!(node.id, "User.orders");
    assert_eq!(node.requires.len(), 1);
}

#[test]
fn test_empty_graph() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![],
        remote_subscription_fields: HashMap::new(),
    };

    let graph = DependencyGraph::build(&metadata).unwrap();
    assert_eq!(graph.node_count(), 0);
    assert!(graph.detect_cycles().is_empty());
}
