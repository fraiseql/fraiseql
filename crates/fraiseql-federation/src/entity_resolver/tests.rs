#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;

#[test]
fn test_deduplicate_representations() {
    let reps = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: {
                let mut m = HashMap::new();
                m.insert("id".to_string(), json!("123"));
                m
            },
            all_fields: HashMap::new(),
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: {
                let mut m = HashMap::new();
                m.insert("id".to_string(), json!("123"));
                m
            },
            all_fields: HashMap::new(),
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: {
                let mut m = HashMap::new();
                m.insert("id".to_string(), json!("456"));
                m
            },
            all_fields: HashMap::new(),
        },
    ];

    let deduped = deduplicate_representations(&reps);
    assert_eq!(deduped.len(), 2);
}

#[test]
fn test_group_entities_by_typename() {
    let reps = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        },
        EntityRepresentation {
            typename:   "Order".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        },
    ];

    let grouped = group_entities_by_typename(&reps);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped["User"].len(), 2);
    assert_eq!(grouped["Order"].len(), 1);
}

#[test]
fn test_multi_key_extract_key_fields() {
    let input = json!({
        "__typename": "OrderItem",
        "order_id": "O1",
        "product_id": "P1",
        "quantity": 5
    });

    let mut rep = EntityRepresentation::from_any(&input).unwrap();
    rep.extract_key_fields(&["order_id".to_string(), "product_id".to_string()]);

    assert_eq!(rep.key_fields.len(), 2);
    assert_eq!(rep.key_fields["order_id"], json!("O1"));
    assert_eq!(rep.key_fields["product_id"], json!("P1"));
}

#[test]
fn test_multi_key_deduplicate() {
    let make_rep = |oid: &str, pid: &str| {
        let mut rep = EntityRepresentation {
            typename:   "OrderItem".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        };
        rep.key_fields.insert("order_id".to_string(), json!(oid));
        rep.key_fields.insert("product_id".to_string(), json!(pid));
        rep
    };

    let reps = vec![
        make_rep("O1", "P1"),
        make_rep("O1", "P1"), // duplicate
        make_rep("O1", "P2"), // different product
    ];

    let deduped = deduplicate_representations(&reps);
    assert_eq!(deduped.len(), 2, "should deduplicate identical multi-key reps");
}

#[test]
fn test_override_field_included_in_local_resolution() {
    // A field with @override(from: "old") must be resolved locally — the subgraph
    // owns it. Verify that FederationResolver classifies the type as Local (not Http).
    use crate::types::{FederatedType, FederationMetadata, KeyDirective};

    let mut product = FederatedType::new("Product".to_string());
    product.keys = vec![KeyDirective {
        fields:     vec!["sku".to_string()],
        resolvable: true,
    }];
    product.set_field_directives(
        "price".to_string(),
        crate::types::FieldFederationDirectives::new()
            .with_override_from("old-pricing".to_string()),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![product],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    let resolver = crate::types::FederationResolver::new(metadata);
    let strategy = resolver.get_or_determine_strategy("Product").unwrap();

    // Product is NOT an extended type, so it resolves locally
    assert!(
        matches!(strategy, crate::types::ResolutionStrategy::Local { .. }),
        "Type with @override must resolve locally, got: {strategy}"
    );
}
