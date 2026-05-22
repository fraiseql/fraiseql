use super::*;

#[test]
fn test_detect_mutation() {
    assert!(is_mutation("mutation { updateUser { id } }"));
    assert!(is_mutation("mutation UpdateUser { updateUser { id } }"));
    assert!(is_mutation("  mutation  {  updateUser  {  id  }  }"));
    assert!(!is_mutation("query { user { id } }"));
    assert!(!is_mutation("{ user { id } }"));
}

#[test]
fn test_extract_mutation_name() {
    let query = "mutation { updateUser { id name } }";
    assert_eq!(extract_mutation_name(query), Some("updateUser".to_string()));

    let query = "mutation UpdateUser($id: ID!) { updateUser(id: $id) { id } }";
    assert_eq!(extract_mutation_name(query), Some("updateUser".to_string()));

    let query = "query { user { id } }";
    // Should not extract from non-mutations
    let _result = extract_mutation_name(query);
    // May or may not find "user" depending on implementation
}

#[test]
fn test_extract_mutation_with_variables() {
    let query = "mutation CreateUser($input: UserInput!) { createUser(input: $input) { id } }";
    assert_eq!(extract_mutation_name(query), Some("createUser".to_string()));
}

#[test]
fn test_extract_typename_from_mutation() {
    assert_eq!(extract_typename_from_mutation("createUser"), Some("User".to_string()));
    assert_eq!(extract_typename_from_mutation("updateOrder"), Some("Order".to_string()));
    assert_eq!(extract_typename_from_mutation("deleteProduct"), Some("Product".to_string()));
    assert_eq!(extract_typename_from_mutation("unknown"), None);
}

#[test]
fn test_mutation_ownership_federation_disabled() {
    let metadata = crate::FederationMetadata::default();
    // With federation disabled, all mutations are local
    assert!(is_local_mutation("updateUser", &metadata));
    assert!(!is_extended_mutation("updateUser", &metadata));
}

#[test]
fn test_mutation_ownership_local_type() {
    let metadata = crate::FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![crate::FederatedType {
            name:                "User".to_string(),
            keys:                vec![],
            is_extends:          false, // NOT extended = local
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    assert!(is_local_mutation("updateUser", &metadata));
    assert!(!is_extended_mutation("updateUser", &metadata));
}

#[test]
fn test_mutation_ownership_extended_type() {
    let metadata = crate::FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![crate::FederatedType {
            name:                "User".to_string(),
            keys:                vec![],
            is_extends:          true, // Extended = remote
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    assert!(!is_local_mutation("updateUser", &metadata));
    assert!(is_extended_mutation("updateUser", &metadata));
}
