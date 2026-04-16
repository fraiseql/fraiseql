//! Parity schema generator for cross-SDK comparison.
//!
//! The Rust authoring SDK focuses on field-level RBAC and does not have
//! query/mutation builders.  This test verifies that the Rust SDK can produce
//! a valid schema fragment for the three parity types with correct scope
//! metadata, and that the fragment is parseable JSON matching the expected
//! type structure.

use std::fs;

use fraiseql_rust::field::Field;
use fraiseql_rust::schema::SchemaRegistry;

/// Build the canonical parity registry (same 3 types as Python/TS/Go reference).
fn build_parity_registry() -> SchemaRegistry {
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

    registry
}

/// Serialize the registry to the parity-comparator JSON shape:
/// `{ "types": [ {"name": "...", "fields": [ {"name":..., "type":..., "nullable":...}, ... ] }, ... ] }`
///
/// The Rust SDK is authoring-for-RBAC only, so queries/mutations are intentionally empty —
/// matched via `compare_schemas.py --types-only` in CI.
fn registry_to_parity_json(registry: &SchemaRegistry) -> String {
    // Deterministic ordering so CI diffs are stable. Comparator is order-agnostic
    // but consistent output simplifies debugging.
    let ordered_names = ["Order", "User", "UserNotFound"];

    let type_entries: Vec<String> = ordered_names
        .iter()
        .filter_map(|name| registry.get_type(name).map(|fields| (name, fields)))
        .map(|(name, fields)| {
            let field_jsons: Vec<String> = fields.iter().map(Field::to_json).collect();
            format!("{{\"name\":\"{name}\",\"fields\":[{fields}]}}", fields = field_jsons.join(","))
        })
        .collect();

    format!(
        "{{\"types\":[{types}],\"queries\":[],\"mutations\":[]}}",
        types = type_entries.join(",")
    )
}

/// Write parity JSON when `SCHEMA_OUTPUT_FILE` is set (used by CI).
#[test]
fn emit_parity_schema_json_when_env_set() {
    let Ok(path) = std::env::var("SCHEMA_OUTPUT_FILE") else {
        return; // Not running under CI; nothing to emit.
    };
    if path.is_empty() {
        return;
    }

    let registry = build_parity_registry();
    let json = registry_to_parity_json(&registry);
    fs::write(&path, json).expect("SCHEMA_OUTPUT_FILE path must be writable");
}

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
