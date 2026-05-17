#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_entity_representation_from_any() {
    let input = serde_json::json!({
        "__typename": "User",
        "id": "123",
        "email": "test@example.com"
    });

    let rep = EntityRepresentation::from_any(&input).unwrap();
    assert_eq!(rep.typename, "User");
    assert_eq!(rep.all_fields.len(), 3);
}

#[test]
fn test_entity_representation_missing_typename() {
    let input = serde_json::json!({
        "id": "123"
    });

    let result = EntityRepresentation::from_any(&input);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for missing __typename, got: {result:?}"
    );
}

#[test]
fn test_extract_key_fields() {
    let input = serde_json::json!({
        "__typename": "User",
        "id": "123",
        "email": "test@example.com"
    });

    let mut rep = EntityRepresentation::from_any(&input).unwrap();
    rep.extract_key_fields(&["id".to_string()]);

    assert_eq!(rep.key_fields.len(), 1);
    assert_eq!(rep.key_fields["id"], "123");
}

#[test]
fn test_federation_metadata_default() {
    let meta = FederationMetadata::default();
    assert!(!meta.enabled);
    assert_eq!(meta.version, "v2");
    assert!(meta.types.is_empty());
}

#[test]
fn test_field_directives_inaccessible() {
    let directives = FieldFederationDirectives::new().inaccessible();
    assert!(directives.inaccessible);
    assert!(!directives.shareable);
    assert!(!directives.external);
}

#[test]
fn test_field_directives_override() {
    let directives = FieldFederationDirectives::new().with_override_from("products".to_string());
    assert_eq!(directives.override_from.as_deref(), Some("products"));
    assert!(!directives.inaccessible);
}

#[test]
fn test_field_directives_inaccessible_and_override_combined() {
    let directives = FieldFederationDirectives::new()
        .inaccessible()
        .with_override_from("reviews".to_string());
    assert!(directives.inaccessible);
    assert_eq!(directives.override_from.as_deref(), Some("reviews"));
}

#[test]
fn test_federated_type_field_is_inaccessible() {
    let mut ftype = FederatedType::new("User".to_string());
    ftype.set_field_directives("ssn".to_string(), FieldFederationDirectives::new().inaccessible());
    assert!(ftype.field_is_inaccessible("ssn"));
    assert!(!ftype.field_is_inaccessible("name"));
}

#[test]
fn test_federated_type_field_has_override() {
    let mut ftype = FederatedType::new("Product".to_string());
    ftype.set_field_directives(
        "price".to_string(),
        FieldFederationDirectives::new().with_override_from("pricing".to_string()),
    );
    assert!(ftype.field_has_override("price"));
    assert!(!ftype.field_has_override("name"));
}

#[test]
fn test_federated_type_inaccessible_fields() {
    let mut ftype = FederatedType::new("User".to_string());
    ftype.inaccessible_fields = vec!["ssn".to_string(), "internal_id".to_string()];
    assert_eq!(ftype.inaccessible_fields.len(), 2);
}
