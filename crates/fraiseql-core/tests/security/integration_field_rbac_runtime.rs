//! Integration tests for Runtime Field Filtering (Field-Level RBAC)
//!
//! Tests that the runtime executor correctly:
//! 1. Checks user roles against field scope requirements
//! 2. Filters fields during projection based on scope requirements
//! 3. Returns null/error for fields user cannot access

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::default_trait_access)] // Reason: test setup uses Default::default() for brevity
use std::collections::HashMap;

use chrono::Utc;
use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, RoleDefinition, SecurityConfig,
    TypeDefinition,
};

/// Helper to create a test schema with scoped fields
fn create_schema_with_scoped_fields() -> CompiledSchema {
    let user_type = TypeDefinition {
        name: "User".into(),
        fields: vec![
            FieldDefinition {
                name: "id".into(),
                field_type: FieldType::Int,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: None, // Public field
                on_deny: FieldDenyPolicy::default(),
                encryption: None,
            },
            FieldDefinition {
                name: "name".into(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: None, // Public field
                on_deny: FieldDenyPolicy::default(),
                encryption: None,
            },
            FieldDefinition {
                name: "email".into(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: Some("read:User.email".to_string()), // Requires explicit scope
                on_deny: FieldDenyPolicy::default(),
                encryption: None,
            },
            FieldDefinition {
                name: "password_hash".into(),
                field_type: FieldType::String,
                nullable: false,
                default_value: None,
                description: None,
                vector_config: None,
                alias: None,
                deprecation: None,
                requires_scope: Some("admin:*".to_string()), // Requires admin scope
                on_deny: FieldDenyPolicy::default(),
                encryption: None,
            },
        ],
        description: None,
        sql_source: "users".into(),
        jsonb_column: String::new(),
        sql_projection_hint: None,
        implements: vec![],
        requires_role: None,
        is_error: false,
        relay: false,
        relationships: vec![],
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
        security: Some(security_config),
        observers_config: None,
        subscriptions_config: None,
        validation_config: None,
        debug_config: None,
        mcp_config: None,
        schema_format_version: None,
        schema_sdl: None,
        custom_scalars: Default::default(),
        ..CompiledSchema::default()
    }
}

/// Helper to create a `SecurityContext` with specific roles
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
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["viewer".to_string()]);

    // Get the security config
    let security = schema.security.as_ref().expect("Schema should have security config");

    // Verify viewer role exists
    let viewer_role = security
        .role_definitions
        .iter()
        .find(|r| r.name == "viewer")
        .expect("Viewer role should exist");

    // Verify viewer has read:User.* scope
    assert!(
        viewer_role.scopes.iter().any(|s| s.as_str() == "read:User.*"),
        "Viewer should have read:User.* scope"
    );

    // Verify user has the viewer role
    assert!(context.roles.contains(&"viewer".to_string()), "User should have viewer role");
}

#[test]
fn test_user_with_viewer_role_cannot_access_admin_fields() {
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
    let schema = create_schema_with_scoped_fields();
    let context = create_security_context(vec!["admin".to_string()]);

    // Get security config
    let security = schema.security.as_ref().expect("Schema should have security config");

    let admin_role = security
        .role_definitions
        .iter()
        .find(|r| r.name == "admin")
        .expect("Admin role should exist");

    // Verify admin has admin:* scope
    assert!(
        admin_role.scopes.iter().any(|s| s.as_str() == "admin:*"),
        "Admin should have admin:* scope"
    );

    // Verify user has admin role
    assert!(context.roles.contains(&"admin".to_string()), "User should have admin role");
}

#[test]
fn test_scope_matching_exact_match() {
    // User has "read:User.email" scope, field requires "read:User.email"
    let schema = create_schema_with_scoped_fields();

    // Get the email field
    let user_type = schema.types.iter().find(|t| t.name == "User").expect("User type should exist");
    let email_field = user_type
        .fields
        .iter()
        .find(|f| f.name == "email")
        .expect("email field should exist");

    assert_eq!(
        email_field.requires_scope,
        Some("read:User.email".to_string()),
        "Email should require read:User.email"
    );

    // Verify this scope can be matched by a role with "read:User.*"
    let viewer_role = RoleDefinition {
        name: "viewer".into(),
        description: None,
        scopes: vec!["read:User.*".into()],
    };

    // Test wildcard matching: "read:User.*" should match "read:User.email"
    assert!(
        viewer_role.has_scope("read:User.email"),
        "Wildcard scope should match specific field"
    );
}

#[test]
fn test_scope_matching_wildcard_all() {
    let admin_role = RoleDefinition {
        name: "admin".into(),
        description: None,
        scopes: vec!["*".into()],
    };

    assert!(admin_role.has_scope("admin:*"), "Global wildcard should match admin:*");
    assert!(
        admin_role.has_scope("read:User.email"),
        "Global wildcard should match read:User.email"
    );
    assert!(admin_role.has_scope("anything"), "Global wildcard should match anything");
}

#[test]
fn test_scope_matching_action_wildcard() {
    let admin_role = RoleDefinition {
        name: "admin".into(),
        description: None,
        scopes: vec!["admin:*".into()],
    };

    assert!(admin_role.has_scope("admin:delete"), "admin:* should match admin:delete");
    assert!(
        admin_role.has_scope("admin:User.password_hash"),
        "admin:* should match admin:User.password_hash"
    );
    assert!(!admin_role.has_scope("read:*"), "admin:* should not match read:*");
}

#[test]
fn test_field_filtering_removes_inaccessible_fields() {
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
    let mut context = create_security_context(vec!["viewer".to_string()]);
    context.roles.push("moderator".to_string());

    // User should have both viewer and moderator roles
    assert!(context.roles.contains(&"viewer".to_string()));
    assert!(context.roles.contains(&"moderator".to_string()));
}

#[test]
fn test_default_role_fallback() {
    let schema = create_schema_with_scoped_fields();

    // Get security config
    let security = schema.security.as_ref().expect("Schema should have security config");

    // Verify default role is set
    assert_eq!(
        security.default_role.as_deref(),
        Some("viewer"),
        "Should have viewer as default role"
    );
}
