#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_composition_validator_creation() {
    let _validator = CompositionValidator::new();
}

#[test]
fn test_cross_subgraph_validator_creation() {
    let subgraphs = vec![];
    let _validator = CrossSubgraphValidator::new(subgraphs);
}

#[test]
fn test_composed_schema_creation() {
    let schema = ComposedSchema::new();
    assert!(schema.types.is_empty());
}

#[test]
fn test_composed_type_from_federated() {
    let ftype = FederatedType::new("User".to_string());
    let composed = ComposedType::from_federated(&ftype);
    assert_eq!(composed.name, "User");
    assert!(!composed.is_extended);
}

#[test]
fn test_composed_type_merge() {
    let user_primary = FederatedType::new("User".to_string());
    let mut user_extension = FederatedType::new("User".to_string());
    user_extension.is_extends = true;

    let mut composed = ComposedType::from_federated(&user_primary);
    composed.merge_from(&user_extension);

    assert_eq!(composed.definitions.len(), 2);
    assert!(composed.is_extended);
}

#[test]
fn test_inaccessible_field_conflict_detected() {
    use crate::types::{FieldFederationDirectives, KeyDirective};

    let mut users_type = FederatedType::new("User".to_string());
    users_type.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    // Mark "ssn" as inaccessible in subgraph A
    users_type
        .set_field_directives("ssn".to_string(), FieldFederationDirectives::new().inaccessible());

    let mut users_type_b = FederatedType::new("User".to_string());
    users_type_b.is_extends = true;
    users_type_b.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    // "ssn" NOT inaccessible in subgraph B — should be a conflict
    users_type_b.set_field_directives("ssn".to_string(), FieldFederationDirectives::new());

    let subgraphs = vec![
        (
            "users".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types: vec![users_type],
                remote_subscription_fields: HashMap::new(),
            },
        ),
        (
            "accounts".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types: vec![users_type_b],
                remote_subscription_fields: HashMap::new(),
            },
        ),
    ];

    let validator = CrossSubgraphValidator::new(subgraphs);
    let result = validator.validate_consistency();
    assert!(result.is_err(), "Expected inaccessible conflict to be detected");

    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CompositionError::InaccessibleFieldConflict { .. })),
        "Expected InaccessibleFieldConflict error, got: {:?}",
        errors
    );
}

#[test]
fn test_override_field_conflict_detected() {
    use crate::types::{FieldFederationDirectives, KeyDirective};

    let mut products_type_a = FederatedType::new("Product".to_string());
    products_type_a.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    // Override "price" from "pricing" subgraph
    products_type_a.set_field_directives(
        "price".to_string(),
        FieldFederationDirectives::new().with_override_from("pricing".to_string()),
    );

    let mut products_type_b = FederatedType::new("Product".to_string());
    products_type_b.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    // Also override "price" from a DIFFERENT subgraph — should conflict
    products_type_b.set_field_directives(
        "price".to_string(),
        FieldFederationDirectives::new().with_override_from("inventory".to_string()),
    );

    let subgraphs = vec![
        (
            "catalog".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types: vec![products_type_a],
                remote_subscription_fields: HashMap::new(),
            },
        ),
        (
            "storefront".to_string(),
            FederationMetadata {
                enabled: true,
                version: "v2".to_string(),
                types: vec![products_type_b],
                remote_subscription_fields: HashMap::new(),
            },
        ),
    ];

    let validator = CrossSubgraphValidator::new(subgraphs);
    let result = validator.validate_consistency();
    assert!(result.is_err(), "Expected override conflict to be detected");

    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CompositionError::OverrideFieldConflict { .. })),
        "Expected OverrideFieldConflict error, got: {:?}",
        errors
    );
}

// ── #728 — the validator must enforce its documented rules ────

fn metadata_for(types: Vec<FederatedType>) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types,
        remote_subscription_fields: HashMap::new(),
    }
}

/// #728.1 — an `@external` field whose owner set has more than one primary
/// definition must be rejected (`ExternalFieldMultipleOwners` was defined and
/// Display-formatted but never constructed).
#[test]
fn external_field_owned_by_multiple_subgraphs_is_detected() {
    use crate::types::{FieldFederationDirectives, KeyDirective};

    let key = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];

    // Two subgraphs BOTH define User.email as a primary (non-external) field.
    let mut owner_a = FederatedType::new("User".to_string());
    owner_a.keys = key.clone();
    owner_a.set_field_directives("email".to_string(), FieldFederationDirectives::new());
    let mut owner_b = FederatedType::new("User".to_string());
    owner_b.keys = key.clone();
    owner_b.set_field_directives("email".to_string(), FieldFederationDirectives::new());

    // A third subgraph references it as @external.
    let mut extender = FederatedType::new("User".to_string());
    extender.is_extends = true;
    extender.keys = key;
    extender.external_fields = vec!["email".to_string()];

    let validator = CrossSubgraphValidator::new(vec![
        ("users".to_string(), metadata_for(vec![owner_a])),
        ("accounts".to_string(), metadata_for(vec![owner_b])),
        ("reviews".to_string(), metadata_for(vec![extender])),
    ]);
    let errors = validator
        .validate_consistency()
        .expect_err("an @external field with two owners must be rejected");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CompositionError::ExternalFieldMultipleOwners { .. })),
        "expected ExternalFieldMultipleOwners, got: {errors:?}"
    );
}

/// #728.2 — two subgraphs each *primarily* defining the same type with
/// different `@key`s must conflict (`or_insert_with` silently kept the
/// first-seen key and let the disagreement pass).
#[test]
fn two_primary_definitions_with_different_keys_conflict() {
    use crate::types::KeyDirective;

    let mut primary_a = FederatedType::new("Order".to_string());
    primary_a.keys = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];
    let mut primary_b = FederatedType::new("Order".to_string());
    primary_b.keys = vec![KeyDirective {
        fields:     vec!["orderNumber".to_string()],
        resolvable: true,
    }];

    let validator = CrossSubgraphValidator::new(vec![
        ("orders".to_string(), metadata_for(vec![primary_a])),
        ("billing".to_string(), metadata_for(vec![primary_b])),
    ]);
    let errors = validator
        .validate_consistency()
        .expect_err("two primaries disagreeing on @key must be rejected");
    assert!(
        errors.iter().any(|e| matches!(e, CompositionError::KeyMismatch { .. })),
        "expected KeyMismatch, got: {errors:?}"
    );
}

/// #728.2b — a type may declare multiple `@key`s; an extension matching ANY of
/// them is valid (only `keys.first()` was compared, so an extension using the
/// second declared key was falsely rejected).
#[test]
fn extension_matching_any_declared_primary_key_is_valid() {
    use crate::types::KeyDirective;

    let mut primary = FederatedType::new("Product".to_string());
    primary.keys = vec![
        KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        },
        KeyDirective {
            fields:     vec!["sku".to_string()],
            resolvable: true,
        },
    ];
    let mut extension = FederatedType::new("Product".to_string());
    extension.is_extends = true;
    extension.keys = vec![KeyDirective {
        fields:     vec!["sku".to_string()],
        resolvable: true,
    }];

    let validator = CrossSubgraphValidator::new(vec![
        ("catalog".to_string(), metadata_for(vec![primary])),
        ("reviews".to_string(), metadata_for(vec![extension])),
    ]);
    assert!(
        validator.validate_consistency().is_ok(),
        "an extension keyed on any of the primary's declared @keys is valid"
    );
}

/// #728.3 — type-level `@shareable` covers every field of the type; a peer
/// marking one of those fields shareable at field level is consistent, not a
/// conflict (`type_shareable` was ignored by `validate_shareable_consistency`).
#[test]
fn type_level_shareable_satisfies_field_level_shareable_peer() {
    use crate::types::{FieldFederationDirectives, KeyDirective};

    let key = vec![KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    }];

    // Subgraph A: the whole type is @shareable (fields carry no field-level flag).
    let mut type_a = FederatedType::new("Money".to_string());
    type_a.keys = key.clone();
    type_a.type_shareable = true;
    type_a.set_field_directives("amount".to_string(), FieldFederationDirectives::new());

    // Subgraph B: the same field marked @shareable at field level.
    let mut type_b = FederatedType::new("Money".to_string());
    type_b.keys = key;
    type_b.set_field_directives("amount".to_string(), FieldFederationDirectives::new().shareable());

    let validator = CrossSubgraphValidator::new(vec![
        ("pricing".to_string(), metadata_for(vec![type_a])),
        ("billing".to_string(), metadata_for(vec![type_b])),
    ]);
    assert!(
        validator.validate_consistency().is_ok(),
        "type-level @shareable covers the field; this is consistent, not a conflict"
    );
}
