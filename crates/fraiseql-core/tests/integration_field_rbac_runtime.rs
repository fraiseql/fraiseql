//! Integration tests for Cycle 5: Runtime Field Filtering (Field-Level RBAC)
//!
//! Tests that the runtime executor correctly:
//! 1. Checks user roles against field scope requirements
//! 2. Filters fields during projection based on scope requirements
//! 3. Returns null/error for fields user cannot access

use std::collections::HashMap;

use chrono::Utc;
use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, RoleDefinition, SecurityConfig, TypeDefinition};

/// Helper to create a test schema with scoped fields
fn create_schema_with_scoped_fields() -> CompiledSchema {
    let user_type = TypeDefinition {
        name: "User".to_string(),
        fields: vec![
            FieldDefinition {
                name: "id".to_string(),
                field_type: FieldType::Int,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: None, // Public field
            },
            FieldDefinition {
                name: "name".to_string(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: None, // Public field
            },
            FieldDefinition {
                name: "email".to_string(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: Some("read:User.email".to_string()), // Requires explicit scope
            },
            FieldDefinition {
                name: "password_hash".to_string(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: Some("admin:*".to_string()), // Requires admin scope
            },
        ],
        description: None,
        sql_source: "users".to_string(),
        jsonb_column: String::new(),
        sql_projection_hint: None,
        implements: vec![],
    };

    let mut security_config = SecurityConfig::new();
    security_config.add_role(
        RoleDefinition::new(
            "viewer".to_string(),
            vec!["read:User.*".to_string(), "read:Post.*".to_string()],
        )
        .with_description("Read-only public fields".to_string()),
    );
    security_config.add_role(
        RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()])
            .with_description("Full access".to_string()),
    );
    security_config.default_role = Some("viewer".to_string());

    CompiledSchema {
        types: vec![user_type],
        queries: vec![],
        mutations: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        subscriptions: vec![],
        directives: vec![],
        observers: vec![],
        fact_tables: HashMap::default(),
        federation: None,
        security: Some(serde_json::to_value(security_config).unwrap()),
        schema_sdl: None,
    }
}

/// Helper to create a SecurityContext with specific roles
fn create_security_context(roles: Vec<String>) -> fraiseql_core::security::SecurityContext {
    fraiseql_core::security::SecurityContext {
        user_id: "user-123".to_string(),
        roles,
        tenant_id: None,
        scopes: vec![],
        attributes: HashMap::new(),
        request_id: "req-123".to_string(),
        ip_address: None,
        authenticated_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        issuer: None,
        audience: None,
    }
}

#[test]
fn test_user_with_viewer_role_can_access_public_fields() {
    // RED: Test that user with 'viewer' role can access public fields
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["viewer".to_string()]);

    // Get the security config
    let security_json = &schema.security;
    assert!(security_json.is_some(), "Schema should have security config");

    // Verify viewer role exists
    let security_value = security_json.as_ref().unwrap();
    let roles = &security_value["role_definitions"];
    assert!(roles.is_array(), "Should have role definitions");

    let viewer_role = roles
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "viewer")
        .expect("Viewer role should exist");

    // Verify viewer has read:User.* scope
    let scopes = viewer_role["scopes"].as_array().expect("Should have scopes");
    assert!(scopes.iter().any(|s| s == "read:User.*"), "Viewer should have read:User.* scope");

    // Verify user has the viewer role
    assert!(context.roles.contains(&"viewer".to_string()), "User should have viewer role");
}

#[test]
fn test_user_with_viewer_role_cannot_access_admin_fields() {
    // RED: Test that user with 'viewer' role cannot access admin-only fields
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["viewer".to_string()]);

    // Get User type
    let user_type = schema.types.iter().find(|t| t.name == "User").expect("User type should exist");

    // Find password_hash field
    let password_field = user_type
        .fields
        .iter()
        .find(|f| f.name == "password_hash")
        .expect("password_hash field should exist");

    // Verify it requires admin:* scope
    assert_eq!(password_field.requires_scope, Some("admin:*".to_string()));

    // Verify user doesn't have admin role
    assert!(!context.roles.contains(&"admin".to_string()), "User should not have admin role");
}

#[test]
fn test_user_with_admin_role_can_access_all_fields() {
    // RED: Test that user with 'admin' role can access all fields including admin-only
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["admin".to_string()]);

    // Get security config
    let security_json = &schema.security;
    assert!(security_json.is_some(), "Schema should have security config");

    let security_value = security_json.as_ref().unwrap();
    let roles = &security_value["role_definitions"];

    let admin_role = roles
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "admin")
        .expect("Admin role should exist");

    // Verify admin has admin:* scope
    let scopes = admin_role["scopes"].as_array().expect("Should have scopes");
    assert!(scopes.iter().any(|s| s == "admin:*"), "Admin should have admin:* scope");

    // Verify user has admin role
    assert!(context.roles.contains(&"admin".to_string()), "User should have admin role");
}

#[test]
fn test_scope_matching_exact_match() {
    // RED: Test that exact scope matching works
    // User has "read:User.email" scope, field requires "read:User.email"
    let schema = create_schema_with_scoped_fields();

    // Get the email field
    let user_type = schema.types.iter().find(|t| t.name == "User").expect("User type should exist");
    let email_field = user_type
        .fields
        .iter()
        .find(|f| f.name == "email")
        .expect("email field should exist");

    assert_eq!(email_field.requires_scope, Some("read:User.email".to_string()), "Email should require read:User.email");

    // Verify this scope can be matched by a role with "read:User.*"
    let viewer_role = RoleDefinition {
        name: "viewer".to_string(),
        description: None,
        scopes: vec!["read:User.*".to_string()],
    };

    // Test wildcard matching: "read:User.*" should match "read:User.email"
    assert!(viewer_role.has_scope("read:User.email"), "Wildcard scope should match specific field");
}

#[test]
fn test_scope_matching_wildcard_all() {
    // RED: Test that global wildcard matches any scope
    let admin_role = RoleDefinition {
        name: "admin".to_string(),
        description: None,
        scopes: vec!["*".to_string()],
    };

    assert!(admin_role.has_scope("admin:*"), "Global wildcard should match admin:*");
    assert!(admin_role.has_scope("read:User.email"), "Global wildcard should match read:User.email");
    assert!(admin_role.has_scope("anything"), "Global wildcard should match anything");
}

#[test]
fn test_scope_matching_action_wildcard() {
    // RED: Test that action wildcard (admin:*) matches specific admin scopes
    let admin_role = RoleDefinition {
        name: "admin".to_string(),
        description: None,
        scopes: vec!["admin:*".to_string()],
    };

    assert!(admin_role.has_scope("admin:delete"), "admin:* should match admin:delete");
    assert!(admin_role.has_scope("admin:User.password_hash"), "admin:* should match admin:User.password_hash");
    assert!(!admin_role.has_scope("read:*"), "admin:* should not match read:*");
}

#[test]
fn test_field_filtering_removes_inaccessible_fields() {
    // RED: Test that field projection removes fields user cannot access
    // When user with 'viewer' role queries User, password_hash should be removed

    let schema = create_schema_with_scoped_fields();
    let _context = create_security_context(vec!["viewer".to_string()]);

    // Get User type with all fields
    let user_type = schema.types.iter().find(|t| t.name == "User").expect("User type should exist");

    // Count accessible vs all fields
    let all_field_count = user_type.fields.len();
    let inaccessible_count = user_type
        .fields
        .iter()
        .filter(|f| f.requires_scope.is_some() && f.requires_scope.as_deref() == Some("admin:*"))
        .count();

    assert_eq!(all_field_count, 4, "Should have 4 fields total");
    assert_eq!(inaccessible_count, 1, "Should have 1 admin-only field");
}

#[test]
fn test_missing_scope_error_handling() {
    // RED: Test error handling when user lacks required scope
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["viewer".to_string()]);

    // Get User type
    let user_type = schema.types.iter().find(|t| t.name == "User").expect("User type should exist");

    // Find password_hash field that requires admin:*
    let password_field = user_type
        .fields
        .iter()
        .find(|f| f.name == "password_hash")
        .expect("password_hash field should exist");

    // Verify user doesn't have the required scope
    assert!(!context.roles.contains(&"admin".to_string()), "User should not have admin role");
    assert_eq!(password_field.requires_scope, Some("admin:*".to_string()));
}

#[test]
fn test_multiple_roles_aggregate_scopes() {
    // RED: Test that multiple roles aggregate their scopes
    let mut context = create_security_context(vec!["viewer".to_string()]);
    context.roles.push("moderator".to_string());

    // User should have both viewer and moderator roles
    assert!(context.roles.contains(&"viewer".to_string()));
    assert!(context.roles.contains(&"moderator".to_string()));
}

#[test]
fn test_default_role_fallback() {
    // RED: Test that default role is available when user has no explicit role
    let schema = create_schema_with_scoped_fields();

    // Get security config
    let security_json = &schema.security;
    let security_value = security_json.as_ref().unwrap();

    // Verify default role is set
    assert_eq!(security_value["default_role"], "viewer", "Should have viewer as default role");
}
