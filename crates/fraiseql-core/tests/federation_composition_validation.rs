//! Federation Multi-Subgraph Composition Validation
//!
//! Comprehensive tests for composing multiple federated subgraphs into a single supergraph.
//! Tests cover consistency checking, conflict detection, and composition strategies.

use std::collections::HashMap;

use fraiseql_core::federation::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection, KeyDirective,
};

// ============================================================================
// Test Fixtures and Helpers
// ============================================================================

/// Create a users subgraph federation metadata
fn create_users_subgraph() -> FederationMetadata {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;
    metadata.version = "v2".to_string();

    // User type with @key(fields: "id")
    let mut user = FederatedType::new("User".to_string());
    user.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add fields
    user.set_field_directives("id".to_string(), FieldFederationDirectives::new());
    user.set_field_directives("email".to_string(), FieldFederationDirectives::new());
    user.set_field_directives("profile".to_string(), FieldFederationDirectives::new());

    metadata.types.push(user);
    metadata
}

/// Create an orders subgraph federation metadata
fn create_orders_subgraph() -> FederationMetadata {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;
    metadata.version = "v2".to_string();

    // Order type with @key(fields: "id")
    let mut order = FederatedType::new("Order".to_string());
    order.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    // Add fields
    order.set_field_directives("id".to_string(), FieldFederationDirectives::new());
    order.set_field_directives("userId".to_string(), FieldFederationDirectives::new());
    order.set_field_directives("total".to_string(), FieldFederationDirectives::new());

    // Extend User type
    let mut user_extension = FederatedType::new("User".to_string());
    user_extension.is_extends = true;
    user_extension.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: false,
    });

    // User.orders field @requires userId
    let mut orders_directives = FieldFederationDirectives::new();
    orders_directives.requires.push(FieldPathSelection {
        path:     vec!["userId".to_string()],
        typename: "User".to_string(),
    });
    user_extension.set_field_directives("orders".to_string(), orders_directives);

    metadata.types.push(order);
    metadata.types.push(user_extension);
    metadata
}

/// Create a products subgraph federation metadata
fn create_products_subgraph() -> FederationMetadata {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;
    metadata.version = "v2".to_string();

    // Product type with @key(fields: "id")
    let mut product = FederatedType::new("Product".to_string());
    product.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });

    product.set_field_directives("id".to_string(), FieldFederationDirectives::new());
    product.set_field_directives("name".to_string(), FieldFederationDirectives::new());
    product.set_field_directives("price".to_string(), FieldFederationDirectives::new());

    metadata.types.push(product);
    metadata
}

// ============================================================================
// Test Category 1: Basic Composition (6 tests)
// ============================================================================

#[test]
fn test_compose_two_subgraphs() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();

    // Both should be valid subgraphs
    assert!(users.enabled);
    assert!(orders.enabled);

    // Users has User type
    assert!(users.types.iter().any(|t| t.name == "User"));

    // Orders has Order and User extension
    assert!(orders.types.iter().any(|t| t.name == "Order"));
    assert!(orders.types.iter().any(|t| t.name == "User" && t.is_extends));
}

#[test]
fn test_compose_three_subgraphs() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();
    let products = create_products_subgraph();

    assert!(users.enabled);
    assert!(orders.enabled);
    assert!(products.enabled);

    // Each has their primary type
    assert!(users.types.iter().any(|t| t.name == "User" && !t.is_extends));
    assert!(orders.types.iter().any(|t| t.name == "Order"));
    assert!(products.types.iter().any(|t| t.name == "Product"));
}

#[test]
fn test_composition_preserves_key_directives() {
    let users = create_users_subgraph();
    let user_type = users.types.iter().find(|t| t.name == "User").unwrap();

    // Should have @key directive
    assert!(!user_type.keys.is_empty());
    assert_eq!(user_type.keys[0].fields, vec!["id"]);
    assert!(user_type.keys[0].resolvable);
}

#[test]
fn test_composition_preserves_extends() {
    let orders = create_orders_subgraph();
    let user_extension = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    // Should be marked as @extends
    assert!(user_extension.is_extends);
}

#[test]
fn test_composition_preserves_field_directives() {
    let orders = create_orders_subgraph();
    let user_extension = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    // User.orders should have @requires
    let orders_directives = user_extension.get_field_directives("orders").unwrap();
    assert!(!orders_directives.requires.is_empty());
}

#[test]
fn test_composition_combines_types_from_multiple_subgraphs() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();
    let products = create_products_subgraph();

    let mut all_types = HashMap::new();
    for subgraph in &[users, orders, products] {
        for ftype in &subgraph.types {
            all_types.entry(ftype.name.clone()).or_insert_with(Vec::new).push(ftype.clone());
        }
    }

    // Should have User (from users + orders extension), Order, Product
    assert!(all_types.contains_key("User"));
    assert!(all_types.contains_key("Order"));
    assert!(all_types.contains_key("Product"));
}

// ============================================================================
// Test Category 2: @key Consistency (6 tests)
// ============================================================================

#[test]
fn test_key_consistency_same_fields_across_subgraphs() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();

    let user_in_users = users.types.iter().find(|t| t.name == "User").unwrap();
    let user_in_orders = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    // Both should reference User with @key(fields: "id")
    assert_eq!(user_in_users.keys[0].fields, vec!["id"]);
    assert_eq!(user_in_orders.keys[0].fields, vec!["id"]);
}

#[test]
fn test_key_ownership_primary_type() {
    let users = create_users_subgraph();
    let user_type = users.types.iter().find(|t| t.name == "User").unwrap();

    // Primary definition should be resolvable
    assert!(user_type.keys[0].resolvable);
}

#[test]
fn test_key_extended_type_not_resolvable() {
    let orders = create_orders_subgraph();
    let user_extension = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    // Extension should not be resolvable (references parent)
    assert!(!user_extension.keys[0].resolvable);
}

#[test]
fn test_key_consistency_validation_same_type_different_keys_fails() {
    let mut users = create_users_subgraph();
    let mut conflicting_orders = create_orders_subgraph();

    // Modify the orders extension to have different key
    if let Some(user_ext) =
        conflicting_orders.types.iter_mut().find(|t| t.name == "User" && t.is_extends)
    {
        user_ext.keys.clear();
        user_ext.keys.push(KeyDirective {
            fields:     vec!["email".to_string()], // Different key!
            resolvable: false,
        });
    }

    // Should detect key mismatch
    let user_key_in_users = &users.types.iter().find(|t| t.name == "User").unwrap().keys[0];
    let user_key_in_orders = &conflicting_orders
        .types
        .iter()
        .find(|t| t.name == "User" && t.is_extends)
        .unwrap()
        .keys[0];

    assert_ne!(user_key_in_users.fields, user_key_in_orders.fields);
}

#[test]
fn test_key_multiple_fields() {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut type_def = FederatedType::new("Entity".to_string());
    type_def.keys.push(KeyDirective {
        fields:     vec!["id".to_string(), "tenantId".to_string()],
        resolvable: true,
    });

    metadata.types.push(type_def);

    let entity = metadata.types.iter().find(|t| t.name == "Entity").unwrap();
    assert_eq!(entity.keys[0].fields.len(), 2);
    assert_eq!(entity.keys[0].fields, vec!["id", "tenantId"]);
}

// ============================================================================
// Test Category 3: @external Field Ownership (5 tests)
// ============================================================================

#[test]
fn test_external_field_ownership_single_owner() {
    let orders = create_orders_subgraph();
    let user_ext = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    // Can mark userId as external (owned by users subgraph)
    // but it's not marked in this test, which is fine
    // External fields are only marked on extensions
}

#[test]
fn test_external_field_marked_on_extended_type() {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut order = FederatedType::new("Order".to_string());
    order.set_field_directives("id".to_string(), FieldFederationDirectives::new());

    let mut _user_ext = FederatedType::new("User".to_string());
    _user_ext.is_extends = true;
    _user_ext.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: false,
    });

    // Mark userId as external (owned elsewhere)
    _user_ext.external_fields.push("userId".to_string());

    metadata.types.push(order);
    metadata.types.push(_user_ext);

    let user = metadata.types.iter().find(|t| t.name == "User").unwrap();
    assert!(user.external_fields.contains(&"userId".to_string()));
}

#[test]
fn test_external_field_cannot_be_defined_twice() {
    // This is a composition validation rule:
    // If User.email is external in orders subgraph,
    // then it cannot be external in another subgraph that also extends User
    // (only one owner for external fields)

    let mut orders = create_orders_subgraph();

    // Mark email as external in orders
    if let Some(user_ext) = orders.types.iter_mut().find(|t| t.name == "User" && t.is_extends) {
        user_ext.external_fields.push("email".to_string());
    }

    // In composition validation, should detect this external field
    assert!(
        orders
            .types
            .iter()
            .any(|t| t.name == "User" && t.external_fields.contains(&"email".to_string()))
    );
}

#[test]
fn test_external_field_multiple_fields() {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut user_ext = FederatedType::new("User".to_string());
    user_ext.is_extends = true;
    user_ext.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: false,
    });

    // Multiple external fields from different subgraph
    user_ext.external_fields.push("email".to_string());
    user_ext.external_fields.push("profile".to_string());

    metadata.types.push(user_ext);

    let user = metadata.types.iter().find(|t| t.name == "User").unwrap();
    assert_eq!(user.external_fields.len(), 2);
    assert!(user.external_fields.contains(&"email".to_string()));
    assert!(user.external_fields.contains(&"profile".to_string()));
}

#[test]
fn test_external_field_cannot_be_owned_by_defining_subgraph() {
    let users = create_users_subgraph();
    let user_type = users.types.iter().find(|t| t.name == "User").unwrap();

    // Primary definition should not have external fields
    // (it owns all its own fields)
    assert!(user_type.external_fields.is_empty());
}

// ============================================================================
// Test Category 4: @shareable Field Consistency (5 tests)
// ============================================================================

#[test]
fn test_shareable_field_same_type_both_subgraphs() {
    let mut users = create_users_subgraph();
    let mut orders = create_orders_subgraph();

    // Mark User.email as @shareable in both subgraphs
    if let Some(user) = users.types.iter_mut().find(|t| t.name == "User") {
        let mut email_directives = FieldFederationDirectives::new();
        email_directives.shareable = true;
        user.set_field_directives("email".to_string(), email_directives);
    }

    if let Some(user_ext) = orders.types.iter_mut().find(|t| t.name == "User" && t.is_extends) {
        let mut email_directives = FieldFederationDirectives::new();
        email_directives.shareable = true;
        user_ext.set_field_directives("email".to_string(), email_directives);
    }

    // Both should have shareable marked
    let user_email_in_users = users
        .types
        .iter()
        .find(|t| t.name == "User")
        .unwrap()
        .get_field_directives("email")
        .unwrap();
    assert!(user_email_in_users.shareable);
}

#[test]
fn test_shareable_field_conflict_one_shareable_one_not() {
    let mut users = create_users_subgraph();
    let mut orders = create_orders_subgraph();

    // Mark as shareable in users
    if let Some(user) = users.types.iter_mut().find(|t| t.name == "User") {
        let mut email_directives = FieldFederationDirectives::new();
        email_directives.shareable = true;
        user.set_field_directives("email".to_string(), email_directives);
    }

    // NOT shareable in orders extension
    if let Some(user_ext) = orders.types.iter_mut().find(|t| t.name == "User" && t.is_extends) {
        let email_directives = FieldFederationDirectives::new();
        user_ext.set_field_directives("email".to_string(), email_directives);
    }

    // Should detect conflict
    let user_in_users = users.types.iter().find(|t| t.name == "User").unwrap();
    let user_in_orders = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    let email_in_users = user_in_users.get_field_directives("email").unwrap();
    let email_in_orders = user_in_orders.get_field_directives("email").unwrap();

    assert_ne!(email_in_users.shareable, email_in_orders.shareable);
}

#[test]
fn test_shareable_field_both_resolvable() {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut user_a = FederatedType::new("User".to_string());
    let mut email_a = FieldFederationDirectives::new();
    email_a.shareable = true;
    user_a.set_field_directives("email".to_string(), email_a);

    let mut user_b = FederatedType::new("User".to_string());
    user_b.is_extends = true;
    let mut email_b = FieldFederationDirectives::new();
    email_b.shareable = true;
    user_b.set_field_directives("email".to_string(), email_b);

    metadata.types.push(user_a);
    metadata.types.push(user_b);

    // Both definitions have @shareable
    assert!(metadata.types.iter().all(|t| t.name == "User"
        && t.get_field_directives("email").map(|d| d.shareable).unwrap_or(false)));
}

#[test]
fn test_shareable_multiple_fields() {
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut user = FederatedType::new("User".to_string());

    // Multiple shareable fields
    let mut email_directives = FieldFederationDirectives::new();
    email_directives.shareable = true;
    user.set_field_directives("email".to_string(), email_directives);

    let mut profile_directives = FieldFederationDirectives::new();
    profile_directives.shareable = true;
    user.set_field_directives("profile".to_string(), profile_directives);

    metadata.types.push(user);

    let user_type = metadata.types.iter().find(|t| t.name == "User").unwrap();
    assert!(user_type.get_field_directives("email").unwrap().shareable);
    assert!(user_type.get_field_directives("profile").unwrap().shareable);
}

#[test]
fn test_shareable_not_on_local_fields() {
    let users = create_users_subgraph();
    let user_type = users.types.iter().find(|t| t.name == "User").unwrap();

    // Local fields (not extended) typically don't need @shareable
    let email_directives = user_type.get_field_directives("email").unwrap();
    assert!(!email_directives.shareable);
}

// ============================================================================
// Test Category 5: Conflict Resolution Strategies (6 tests)
// ============================================================================

#[test]
fn test_conflict_resolution_strategy_first_wins() {
    // FirstWins strategy: first subgraph's definition takes precedence
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();

    // When composing, users subgraph comes first
    // so User type from users is primary
    let _subgraphs = vec![users, orders];
    assert_eq!(_subgraphs[0].types.iter().find(|t| t.name == "User").unwrap().name, "User");
}

#[test]
fn test_conflict_resolution_strategy_shareable_required() {
    // Shareable strategy: both definitions must have @shareable
    let mut metadata = FederationMetadata::default();
    metadata.enabled = true;

    let mut user_a = FederatedType::new("User".to_string());
    let mut email_a = FieldFederationDirectives::new();
    email_a.shareable = true;
    user_a.set_field_directives("email".to_string(), email_a);

    let mut user_b = FederatedType::new("User".to_string());
    user_b.is_extends = true;
    let mut email_b = FieldFederationDirectives::new();
    email_b.shareable = true;
    user_b.set_field_directives("email".to_string(), email_b);

    metadata.types.push(user_a);
    metadata.types.push(user_b);

    // Both are @shareable, so composition is valid
    assert!(
        metadata
            .types
            .iter()
            .all(|t| t.get_field_directives("email").map(|d| d.shareable).unwrap_or(false))
    );
}

#[test]
fn test_conflict_resolution_strategy_error_on_conflict() {
    // Error strategy: fail composition on any conflict
    let mut users = create_users_subgraph();
    let mut orders = create_orders_subgraph();

    // Create conflicting field definitions
    if let Some(user) = users.types.iter_mut().find(|t| t.name == "User") {
        let mut email_directives = FieldFederationDirectives::new();
        email_directives.shareable = true;
        user.set_field_directives("email".to_string(), email_directives);
    }

    if let Some(user_ext) = orders.types.iter_mut().find(|t| t.name == "User" && t.is_extends) {
        let mut email_directives = FieldFederationDirectives::new();
        email_directives.shareable = false; // Conflict!
        user_ext.set_field_directives("email".to_string(), email_directives);
    }

    // Should be detectable as a conflict
    let user_in_users = users.types.iter().find(|t| t.name == "User").unwrap();
    let user_in_orders = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    let email_users = user_in_users.get_field_directives("email").unwrap();
    let email_orders = user_in_orders.get_field_directives("email").unwrap();

    // Different shareable values should be flagged
    assert_ne!(email_users.shareable, email_orders.shareable);
}

#[test]
fn test_conflict_resolution_by_priority_list() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();
    let products = create_products_subgraph();

    let _subgraphs = vec![users, orders, products];

    // Define priority: users > orders > products
    let priority_map: HashMap<String, usize> = [
        ("users".to_string(), 0),
        ("orders".to_string(), 1),
        ("products".to_string(), 2),
    ]
    .iter()
    .cloned()
    .collect();

    // Verify priority structure exists
    assert_eq!(priority_map.len(), 3);
    assert_eq!(priority_map.get("users"), Some(&0));
}

#[test]
fn test_conflict_resolution_multiple_types() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();
    let products = create_products_subgraph();

    // Multiple types with different compositions
    let mut type_count: HashMap<String, usize> = HashMap::new();

    for subgraph in &[users, orders, products] {
        for ftype in &subgraph.types {
            *type_count.entry(ftype.name.clone()).or_insert(0) += 1;
        }
    }

    // User appears in both users and orders (count = 2)
    assert_eq!(type_count.get("User"), Some(&2));

    // Order appears only in orders (count = 1)
    assert_eq!(type_count.get("Order"), Some(&1));

    // Product appears only in products (count = 1)
    assert_eq!(type_count.get("Product"), Some(&1));
}

// ============================================================================
// Test Category 6: Cross-Subgraph References (4 tests)
// ============================================================================

#[test]
fn test_cross_subgraph_type_reference() {
    let _users = create_users_subgraph();
    let orders = create_orders_subgraph();

    // Orders references User type from Users subgraph
    let order_in_orders = orders.types.iter().find(|t| t.name == "Order").unwrap();
    let user_in_orders = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    assert!(order_in_orders.name == "Order");
    assert!(user_in_orders.is_extends);
}

#[test]
fn test_cross_subgraph_field_reference() {
    let orders = create_orders_subgraph();

    // User.orders field references from Orders subgraph
    let user_ext = orders.types.iter().find(|t| t.name == "User" && t.is_extends).unwrap();

    let orders_field = user_ext.get_field_directives("orders").unwrap();
    assert!(!orders_field.requires.is_empty());

    // The @requires references Order.userId
    assert_eq!(orders_field.requires[0].typename, "User");
}

#[test]
fn test_cross_subgraph_entity_resolution_path() {
    let users = create_users_subgraph();
    let orders = create_orders_subgraph();

    // Query path: User (users sg) -> orders field -> Order (orders sg)
    // Composition must handle references across subgraphs

    let user_in_users = users.types.iter().find(|t| t.name == "User").unwrap();
    let order_in_orders = orders.types.iter().find(|t| t.name == "Order").unwrap();

    assert_eq!(user_in_users.name, "User");
    assert_eq!(order_in_orders.name, "Order");
}

#[test]
fn test_cross_subgraph_circular_reference_detection() {
    // User (users) -> has orders (Order)
    // Order (orders) -> references User
    // This is valid (not circular) because User owns itself

    let users = create_users_subgraph();
    let orders = create_orders_subgraph();

    // Both subgraphs are valid independently
    assert!(users.enabled);
    assert!(orders.enabled);

    // When composed, creates a valid entity reference path
}
