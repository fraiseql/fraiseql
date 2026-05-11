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
        users_type.set_field_directives(
            "ssn".to_string(),
            FieldFederationDirectives::new().inaccessible(),
        );

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
                    types:   vec![users_type],
                    remote_subscription_fields: HashMap::new(),
                },
            ),
            (
                "accounts".to_string(),
                FederationMetadata {
                    enabled: true,
                    version: "v2".to_string(),
                    types:   vec![users_type_b],
                    remote_subscription_fields: HashMap::new(),
                },
            ),
        ];

        let validator = CrossSubgraphValidator::new(subgraphs);
        let result = validator.validate_consistency();
        assert!(result.is_err(), "Expected inaccessible conflict to be detected");

        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(e, CompositionError::InaccessibleFieldConflict { .. })),
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
                    types:   vec![products_type_a],
                    remote_subscription_fields: HashMap::new(),
                },
            ),
            (
                "storefront".to_string(),
                FederationMetadata {
                    enabled: true,
                    version: "v2".to_string(),
                    types:   vec![products_type_b],
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
