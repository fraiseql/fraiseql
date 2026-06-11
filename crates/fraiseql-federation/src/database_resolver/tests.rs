use std::collections::HashMap;

use super::build_select_fields;
use crate::{
    selection_parser::FieldSelection,
    types::{FederatedType, KeyDirective},
};

#[test]
fn test_database_resolver_creation() {
    // Test that resolver can be created (mock adapter would be used)
    // Actual DB tests are in integration tests
}

fn user_type_with_inaccessible() -> FederatedType {
    FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     vec!["externalOnly".to_string()],
        shareable_fields:    vec![],
        inaccessible_fields: vec!["password_hash".to_string()],
        field_directives:    HashMap::new(),
        type_shareable:      false,
    }
}

/// M-fed-select-list: `@inaccessible` / `@external` fields and injection-shaped
/// tokens are dropped from the SELECT list; key fields are always present.
#[test]
fn build_select_fields_drops_inaccessible_external_and_injection() {
    let fed_type = user_type_with_inaccessible();
    let selection = FieldSelection::new(vec![
        "name".to_string(),
        "password_hash".to_string(),  // @inaccessible -> dropped
        "externalOnly".to_string(),   // @external -> dropped
        "__typename".to_string(),     // GraphQL meta-field -> dropped
        "id, (SELECT 1)".to_string(), // not a plain identifier -> dropped
    ]);

    let fields = build_select_fields(&selection, &fed_type);

    assert!(fields.contains(&"name".to_string()), "exposed field kept");
    assert!(fields.contains(&"id".to_string()), "key field always present");
    assert!(
        !fields.contains(&"password_hash".to_string()),
        "@inaccessible field must never be selected"
    );
    assert!(
        !fields.contains(&"externalOnly".to_string()),
        "@external field must never be selected"
    );
    assert!(!fields.iter().any(|f| f == "__typename"), "__typename is not a stored column");
    assert!(
        !fields.iter().any(|f| f.contains("SELECT")),
        "injection-shaped token must be dropped"
    );
}
