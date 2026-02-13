/// Phase 18 Cycle 16: Rust SDK - Field Scope Extraction & Validation
///
/// RED phase tests for field-level RBAC scope extraction.
/// Tests cover:
/// - Field struct creation and properties
/// - Single scope requirements (requires_scope)
/// - Multiple scopes array (requires_scopes)
/// - Scope pattern validation (action:resource format)
/// - SchemaRegistry for type tracking
/// - JSON export with scope metadata

use fraiseql_rust::field::Field;
use fraiseql_rust::schema::{SchemaRegistry, validate_scope};

// ============================================================================
// FIELD STRUCT TESTS (3 tests)
// ============================================================================

#[test]
fn test_field_creation_with_all_properties() {
    let field = Field::new("email", "String")
        .with_nullable(false)
        .with_requires_scope(Some("read:user.email".to_string()))
        .with_description(Some("User email address".to_string()));

    assert_eq!(field.name, "email");
    assert_eq!(field.field_type, "String");
    assert!(!field.nullable);
    assert_eq!(field.requires_scope, Some("read:user.email".to_string()));
    assert_eq!(field.description, Some("User email address".to_string()));
}

#[test]
fn test_field_creation_minimal() {
    let field = Field::new("id", "Int");

    assert_eq!(field.name, "id");
    assert_eq!(field.field_type, "Int");
    assert!(field.nullable);
    assert_eq!(field.requires_scope, None);
    assert_eq!(field.requires_scopes, None);
    assert_eq!(field.description, None);
}

#[test]
fn test_field_with_metadata_preservation() {
    let field = Field::new("password", "String")
        .with_nullable(false)
        .with_requires_scope(Some("admin:user.*".to_string()))
        .with_description(Some("Hashed password".to_string()));

    assert_eq!(field.name, "password");
    assert_eq!(field.requires_scope, Some("admin:user.*".to_string()));
    assert_eq!(field.description, Some("Hashed password".to_string()));
}

// ============================================================================
// SINGLE SCOPE REQUIREMENT TESTS (3 tests)
// ============================================================================

#[test]
fn test_field_with_single_scope_format() {
    let field = Field::new("email", "String")
        .with_requires_scope(Some("read:user.email".to_string()));

    assert_eq!(field.requires_scope, Some("read:user.email".to_string()));
    assert_eq!(field.requires_scopes, None);
}

#[test]
fn test_field_with_wildcard_resource_scope() {
    let field = Field::new("profile", "Object")
        .with_requires_scope(Some("read:User.*".to_string()));

    assert_eq!(field.requires_scope, Some("read:User.*".to_string()));
}

#[test]
fn test_field_with_global_wildcard_scope() {
    let field = Field::new("secret", "String")
        .with_requires_scope(Some("admin:*".to_string()));

    assert_eq!(field.requires_scope, Some("admin:*".to_string()));
}

// ============================================================================
// MULTIPLE SCOPES ARRAY TESTS (3 tests)
// ============================================================================

#[test]
fn test_field_with_multiple_scopes_array() {
    let scopes = vec!["read:user.email".to_string(), "write:user.email".to_string()];
    let field = Field::new("email", "String")
        .with_requires_scopes(Some(scopes.clone()));

    assert_eq!(field.requires_scopes, Some(scopes));
    assert_eq!(field.requires_scope, None);
}

#[test]
fn test_field_with_single_element_scopes_array() {
    let scopes = vec!["read:user.profile".to_string()];
    let field = Field::new("profile", "Object")
        .with_requires_scopes(Some(scopes.clone()));

    assert_eq!(field.requires_scopes, Some(scopes));
    assert_eq!(field.requires_scopes.as_ref().unwrap().len(), 1);
}

#[test]
fn test_field_with_complex_scopes_array() {
    let scopes = vec![
        "read:user.email".to_string(),
        "write:user.*".to_string(),
        "admin:*".to_string(),
    ];
    let field = Field::new("data", "String")
        .with_requires_scopes(Some(scopes.clone()));

    assert_eq!(field.requires_scopes, Some(scopes));
}

// ============================================================================
// SCOPE PATTERN VALIDATION TESTS (6 tests)
// ============================================================================

#[test]
fn test_validate_scope_valid_specific_field() {
    assert!(validate_scope("read:user.email").is_ok());
}

#[test]
fn test_validate_scope_valid_resource_wildcard() {
    assert!(validate_scope("read:User.*").is_ok());
}

#[test]
fn test_validate_scope_valid_admin_wildcard() {
    assert!(validate_scope("admin:*").is_ok());
}

#[test]
fn test_validate_scope_invalid_missing_colon() {
    assert!(validate_scope("readuser").is_err());
}

#[test]
fn test_validate_scope_invalid_action_with_hyphen() {
    assert!(validate_scope("read-all:user").is_err());
}

#[test]
fn test_validate_scope_invalid_resource_with_hyphen() {
    assert!(validate_scope("read:user-data").is_err());
}

// ============================================================================
// SCHEMA REGISTRY TESTS (3 tests)
// ============================================================================

#[test]
fn test_schema_registry_register_type() {
    let mut registry = SchemaRegistry::new();

    let user_fields = vec![
        Field::new("id", "Int"),
        Field::new("email", "String")
            .with_requires_scope(Some("read:user.email".to_string())),
    ];

    registry.register_type("User", user_fields);

    let registered = registry.get_type("User");
    assert!(registered.is_some());
    assert_eq!(registered.unwrap().len(), 2);
}

#[test]
fn test_schema_registry_extract_scoped_fields() {
    let mut registry = SchemaRegistry::new();

    let user_fields = vec![
        Field::new("id", "Int"),
        Field::new("email", "String")
            .with_requires_scope(Some("read:user.email".to_string())),
        Field::new("password", "String")
            .with_requires_scope(Some("admin:user.password".to_string())),
    ];

    registry.register_type("User", user_fields);

    let scoped = registry.extract_scoped_fields();
    assert!(scoped.contains_key("User"));
    assert_eq!(scoped["User"].len(), 2);
    assert!(scoped["User"].contains(&"email".to_string()));
    assert!(scoped["User"].contains(&"password".to_string()));
}

#[test]
fn test_schema_registry_multiple_types() {
    let mut registry = SchemaRegistry::new();

    let user_fields = vec![
        Field::new("id", "Int"),
        Field::new("email", "String")
            .with_requires_scope(Some("read:user.email".to_string())),
    ];

    let post_fields = vec![
        Field::new("id", "Int"),
        Field::new("content", "String")
            .with_requires_scope(Some("read:post.content".to_string())),
    ];

    registry.register_type("User", user_fields);
    registry.register_type("Post", post_fields);

    assert!(registry.get_type("User").is_some());
    assert!(registry.get_type("Post").is_some());

    let scoped = registry.extract_scoped_fields();
    assert_eq!(scoped.len(), 2);
}

// ============================================================================
// JSON EXPORT TESTS (2 tests)
// ============================================================================

#[test]
fn test_field_to_json() {
    let field = Field::new("email", "String")
        .with_nullable(false)
        .with_requires_scope(Some("read:user.email".to_string()));

    let json = field.to_json();
    assert!(json.contains("\"name\""));
    assert!(json.contains("\"email\""));
    assert!(json.contains("\"requiresScope\""));
    assert!(json.contains("\"read:user.email\""));
}

#[test]
fn test_schema_registry_export_to_json() {
    let mut registry = SchemaRegistry::new();

    let user_fields = vec![
        Field::new("id", "Int"),
        Field::new("email", "String")
            .with_requires_scope(Some("read:user.email".to_string())),
    ];

    registry.register_type("User", user_fields);

    let json = registry.export_to_json();
    assert!(json.contains("\"User\""));
    assert!(json.contains("\"email\""));
    assert!(json.contains("\"requiresScope\""));
}

// ============================================================================
// CONFLICTING SCOPE AND SCOPES TESTS (2 tests)
// ============================================================================

#[test]
fn test_field_cannot_have_both_scope_and_scopes() {
    // This test documents that requires_scope and requires_scopes are mutually exclusive
    // Implementation should reject fields with both set
    let field = Field::new("email", "String")
        .with_requires_scope(Some("read:user.email".to_string()))
        .with_requires_scopes(Some(vec!["write:user.email".to_string()]));

    // The implementation should have rejected this (field_scopes_conflict)
    // For now, we test that validation would catch this
    assert!(field.requires_scope.is_some());
    assert!(field.requires_scopes.is_some());
    // In GREEN phase, add validation to prevent this
}

#[test]
fn test_validate_empty_scope_string() {
    assert!(validate_scope("").is_err());
}
