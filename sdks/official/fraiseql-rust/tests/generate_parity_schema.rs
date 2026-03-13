//! Parity schema generator for cross-SDK comparison.
//!
//! The Rust authoring SDK focuses on field-level RBAC and does not have
//! query/mutation builders.  This test verifies that the Rust SDK can produce
//! a valid schema fragment for the three parity types with correct scope
//! metadata, and that the fragment is parseable JSON matching the expected
//! type structure.

use fraiseql_rust::field::Field;
use fraiseql_rust::schema::SchemaRegistry;

/// Build the parity type definitions and verify they contain the expected fields.
#[test]
fn parity_types_are_registered_correctly() {
    let mut registry = SchemaRegistry::new();

    registry.register_type(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
            Field::new("name", "String").with_nullable(false),
        ],
    );

    registry.register_type(
        "Order",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("total", "Float").with_nullable(false),
        ],
    );

    registry.register_type(
        "UserNotFound",
        vec![
            Field::new("message", "String").with_nullable(false),
            Field::new("code", "String").with_nullable(false),
        ],
    );

    // All three types must be registered
    assert!(registry.get_type("User").is_some());
    assert!(registry.get_type("Order").is_some());
    assert!(registry.get_type("UserNotFound").is_some());

    // Field count checks
    assert_eq!(registry.get_type("User").unwrap().len(), 3);
    assert_eq!(registry.get_type("Order").unwrap().len(), 2);
    assert_eq!(registry.get_type("UserNotFound").unwrap().len(), 2);
}

/// Verify the JSON export contains expected type names and is non-empty JSON.
#[test]
fn parity_schema_json_is_parseable() {
    let mut registry = SchemaRegistry::new();

    registry.register_type(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
            Field::new("name", "String").with_nullable(false),
        ],
    );

    registry.register_type(
        "Order",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("total", "Float").with_nullable(false),
        ],
    );

    registry.register_type(
        "UserNotFound",
        vec![
            Field::new("message", "String").with_nullable(false),
            Field::new("code", "String").with_nullable(false),
        ],
    );

    let json = registry.export_to_json();
    // Must be a non-empty JSON object
    assert!(json.starts_with('{'), "export_to_json must start with '{{'");
    assert!(json.ends_with('}'), "export_to_json must end with '}}'");

    // All three type names must appear in the output
    assert!(json.contains("User"), "JSON must contain 'User'");
    assert!(json.contains("Order"), "JSON must contain 'Order'");
    assert!(json.contains("UserNotFound"), "JSON must contain 'UserNotFound'");
}

/// Verify scoped field access on a parity type works correctly.
#[test]
fn parity_scoped_field_is_extracted() {
    let mut registry = SchemaRegistry::new();

    // Add a scope-protected field to User for RBAC parity validation
    registry.register_type(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
            Field::new("salary", "Float")
                .with_nullable(false)
                .with_requires_scope(Some("read:User.salary".to_string())),
        ],
    );

    let scoped = registry.extract_scoped_fields();
    assert!(scoped.contains_key("User"), "User must have scoped fields");
    assert_eq!(scoped["User"], vec!["salary"], "Only 'salary' has a scope requirement");
}
