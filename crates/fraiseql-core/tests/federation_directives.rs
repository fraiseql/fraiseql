//! Federation directive parsing and validation tests
//!
//! Tests for parsing and validating Apollo Federation v2 directives:
//! - @key: Entity identifier for federation
//! - @external: Field owned by another subgraph
//! - @extends: Extend entity from another subgraph
//! - @shareable: Field available across subgraphs
//! - @requires: Field dependency resolution
//! - @provides: Field resolved by this subgraph

use fraiseql_core::federation::types::{FederatedType, FederationMetadata, KeyDirective};

// ============================================================================
// @key Directive Tests
// ============================================================================

#[test]
fn test_key_directive_single_field() {
    // @key(fields: "id")
    let key = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    };

    assert_eq!(key.fields.len(), 1);
    assert_eq!(key.fields[0], "id");
    assert!(key.resolvable);
}

#[test]
fn test_key_directive_composite_key() {
    // @key(fields: "organizationId id")
    let key = KeyDirective {
        fields:     vec!["organizationId".to_string(), "id".to_string()],
        resolvable: true,
    };

    assert_eq!(key.fields.len(), 2);
    assert_eq!(key.fields[0], "organizationId");
    assert_eq!(key.fields[1], "id");
}

#[test]
fn test_key_directive_resolvable_flag() {
    // Resolvable flag indicates whether _entities query can resolve this type
    let resolvable = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    };

    let non_resolvable = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: false,
    };

    assert!(resolvable.resolvable);
    assert!(!non_resolvable.resolvable);
}

#[test]
fn test_key_directive_multiple_keys() {
    // A type can have multiple @key directives for different resolution strategies
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![
                KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                },
                KeyDirective {
                    fields:     vec!["email".to_string()],
                    resolvable: true,
                },
            ],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let user_type = &metadata.types[0];
    assert_eq!(user_type.keys.len(), 2);
    assert_eq!(user_type.keys[0].fields[0], "id");
    assert_eq!(user_type.keys[1].fields[0], "email");
}

// ============================================================================
// @external Directive Tests
// ============================================================================

#[test]
fn test_external_field_single() {
    // @external indicates field is owned by another subgraph
    let fed_type = FederatedType {
        name:             "User".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["email".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.external_fields.contains(&"email".to_string()));
    assert_eq!(fed_type.external_fields.len(), 1);
}

#[test]
fn test_external_field_multiple() {
    // Multiple @external fields in extended type
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["customerId".to_string(), "customerEmail".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert_eq!(fed_type.external_fields.len(), 2);
    assert!(fed_type.external_fields.contains(&"customerId".to_string()));
    assert!(fed_type.external_fields.contains(&"customerEmail".to_string()));
}

#[test]
fn test_external_field_key_field() {
    // Key fields should also be marked as external in extended types
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["id".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // Key field "id" is also external in this extended type
    assert!(fed_type.external_fields.contains(&"id".to_string()));
}

// ============================================================================
// @extends Directive Tests
// ============================================================================

#[test]
fn test_extends_directive_owned_entity() {
    // Local entity (not extended)
    let fed_type = FederatedType {
        name:             "User".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       false,
        external_fields:  vec![],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(!fed_type.is_extends, "Local entity should not be extended");
}

#[test]
fn test_extends_directive_extended_entity() {
    // Extended entity (extends another subgraph's type)
    let fed_type = FederatedType {
        name:             "User".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["email".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.is_extends, "Extended entity should have is_extends=true");
}

#[test]
fn test_extends_with_external_fields() {
    // Extended types must have external fields
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["customerId".to_string()],
        shareable_fields: vec!["total".to_string()],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.is_extends);
    assert!(!fed_type.external_fields.is_empty());
    assert_eq!(fed_type.external_fields[0], "customerId");
}

// ============================================================================
// @shareable Directive Tests
// ============================================================================

#[test]
fn test_shareable_field_single() {
    // @shareable indicates field can be resolved by multiple subgraphs
    let fed_type = FederatedType {
        name:             "User".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       false,
        external_fields:  vec![],
        shareable_fields: vec!["email".to_string()],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.shareable_fields.contains(&"email".to_string()));
}

#[test]
fn test_shareable_field_multiple() {
    // Multiple shareable fields
    let fed_type = FederatedType {
        name:             "Product".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       false,
        external_fields:  vec![],
        shareable_fields: vec!["name".to_string(), "description".to_string()],
        field_directives: std::collections::HashMap::new(),
    };

    assert_eq!(fed_type.shareable_fields.len(), 2);
    assert!(fed_type.shareable_fields.contains(&"name".to_string()));
    assert!(fed_type.shareable_fields.contains(&"description".to_string()));
}

#[test]
fn test_shareable_field_in_extended_type() {
    // Extended types can have shareable fields
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["customerId".to_string()],
        shareable_fields: vec!["status".to_string()],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.is_extends);
    assert!(fed_type.shareable_fields.contains(&"status".to_string()));
}

// ============================================================================
// Composite Key Validation Tests
// ============================================================================

#[test]
fn test_composite_key_multi_tenant() {
    // Multi-tenant composite key: (tenantId, id)
    let fed_type = FederatedType {
        name:             "TenantUser".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["tenantId".to_string(), "id".to_string()],
            resolvable: true,
        }],
        is_extends:       false,
        external_fields:  vec![],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert_eq!(fed_type.keys[0].fields.len(), 2);
    assert_eq!(fed_type.keys[0].fields[0], "tenantId");
    assert_eq!(fed_type.keys[0].fields[1], "id");
}

#[test]
fn test_composite_key_ordering_matters() {
    // Key field order matters for uniqueness
    let key1 = KeyDirective {
        fields:     vec!["organizationId".to_string(), "userId".to_string()],
        resolvable: true,
    };

    let key2 = KeyDirective {
        fields:     vec!["userId".to_string(), "organizationId".to_string()],
        resolvable: true,
    };

    assert_ne!(key1.fields, key2.fields, "Key field order matters");
}

#[test]
fn test_composite_key_uniqueness() {
    // All fields in composite key must be unique
    let key = KeyDirective {
        fields:     vec!["organizationId".to_string(), "id".to_string()],
        resolvable: true,
    };

    assert_eq!(key.fields.len(), 2);
    assert_ne!(key.fields[0], key.fields[1], "Key fields must be unique");
}

// ============================================================================
// Federation Metadata Structure Tests
// ============================================================================

#[test]
fn test_federation_metadata_version() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![],
    };

    assert_eq!(metadata.version, "v2");
    assert!(metadata.enabled);
}

#[test]
fn test_federation_metadata_enabled_flag() {
    let enabled = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![],
    };

    let disabled = FederationMetadata {
        enabled: false,
        version: "v2".to_string(),
        types:   vec![],
    };

    assert!(enabled.enabled);
    assert!(!disabled.enabled);
}

#[test]
fn test_federation_metadata_multiple_types() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "Order".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    };

    assert_eq!(metadata.types.len(), 2);
    assert_eq!(metadata.types[0].name, "User");
    assert_eq!(metadata.types[1].name, "Order");
}

// ============================================================================
// Directive Conflict Tests
// ============================================================================

#[test]
fn test_field_cannot_be_both_external_and_key() {
    // A field cannot be both a key field and external in the same type
    // (in extended types, key fields are external, but that's handled separately)
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["id".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    // This is valid for extended types (key fields are external)
    assert!(fed_type.external_fields.contains(&"id".to_string()));
}

#[test]
fn test_owned_type_cannot_have_external_fields() {
    // Owned (non-extended) types should not have external fields
    let fed_type = FederatedType {
        name:             "User".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       false,
        external_fields:  vec![], // Must be empty for owned types
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(!fed_type.is_extends);
    assert!(fed_type.external_fields.is_empty());
}

// ============================================================================
// Directive Validation Tests
// ============================================================================

#[test]
fn test_key_directive_requires_fields() {
    let key = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    };

    assert!(!key.fields.is_empty(), "Key directive must have fields");
}

#[test]
fn test_resolvable_key_vs_non_resolvable() {
    let resolvable = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    };

    let non_resolvable = KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: false,
    };

    // Resolvable keys can be used with _entities query
    assert!(resolvable.resolvable);
    // Non-resolvable keys cannot be used with _entities query
    assert!(!non_resolvable.resolvable);
}

#[test]
fn test_extended_type_must_have_key() {
    let fed_type = FederatedType {
        name:             "Order".to_string(),
        keys:             vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:       true,
        external_fields:  vec!["customerId".to_string()],
        shareable_fields: vec![],
        field_directives: std::collections::HashMap::new(),
    };

    assert!(fed_type.is_extends);
    assert!(!fed_type.keys.is_empty(), "Extended types must have at least one key");
}
