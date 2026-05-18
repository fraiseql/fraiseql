#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::types::KeyDirective;

fn make_test_metadata() -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                "User".to_string(),
            keys:                vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:          false,
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

#[test]
fn test_find_federation_type_success() {
    let metadata = make_test_metadata();
    let fed_type = find_federation_type("User", &metadata)
        .unwrap_or_else(|e| panic!("expected Ok for 'User' type: {e}"));
    assert_eq!(fed_type.name, "User");
}

#[test]
fn test_find_federation_type_not_found() {
    let metadata = make_test_metadata();
    let result = find_federation_type("Order", &metadata);
    assert!(result.is_err(), "expected Err for missing type 'Order'");
}

#[test]
fn test_get_key_directive_success() {
    let metadata = make_test_metadata();
    let fed_type = find_federation_type("User", &metadata).unwrap();
    let key_dir = get_key_directive(fed_type)
        .unwrap_or_else(|e| panic!("expected Ok from get_key_directive: {e}"));
    assert_eq!(key_dir.fields, vec!["id".to_string()]);
}

#[test]
fn test_find_type_with_key_success() {
    let metadata = make_test_metadata();
    let (fed_type, key_dir) = find_type_with_key("User", &metadata)
        .unwrap_or_else(|e| panic!("expected Ok for 'User' with key: {e}"));
    assert_eq!(fed_type.name, "User");
    assert_eq!(key_dir.fields[0], "id");
}

#[test]
fn test_find_type_with_key_not_found() {
    let metadata = make_test_metadata();
    let result = find_type_with_key("NonExistent", &metadata);
    assert!(result.is_err(), "expected Err for missing type 'NonExistent'");
}
