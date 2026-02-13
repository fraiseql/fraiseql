//! End-to-End Tests: Field-Level RBAC Pipeline
//!
//! This test suite validates the complete flow from schema authoring through compilation to runtime
//! field filtering.
//!
//! Test scenarios:
//! 1. Python decorator field scopes → compiled schema
//! 2. TypeScript decorator field scopes → compiled schema
//! 3. TOML security config with role definitions → compiled schema
//! 4. Both field scopes and role definitions merge in compiled schema
//! 5. Runtime: field filtering based on user roles
//! 6. Runtime: scope matching with wildcards
//! 7. Runtime: multiple roles aggregate scopes
//! 8. Runtime: public fields always accessible
//! 9. Runtime: error on missing required scope
//! 10. Runtime: E2E pipeline for multi-tenant scenarios

use std::collections::HashMap;

use chrono::Utc;
use fraiseql_core::{
    runtime::{can_access_field, filter_fields},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};

// ============================================================================
// Helpers: Schema and Config Construction
// ============================================================================

/// Helper to create a realistic User type with mixed public/scoped fields
/// Simulates Python decorator output
fn create_user_type_with_scopes() -> TypeDefinition {
    TypeDefinition {
        name:                "User".to_string(),
        fields:              vec![
            // Public fields (no scope required)
            FieldDefinition {
                name:           "id".to_string(),
                field_type:     FieldType::Int,
                nullable:       false,
                default_value:  None,
                description:    Some("User ID (public)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            FieldDefinition {
                name:           "name".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    Some("User name (public)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            // Protected fields
            FieldDefinition {
                name:           "email".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    Some("User email (requires read:User.email)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:User.email".to_string()),
            },
            FieldDefinition {
                name:           "phone".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    Some("User phone (requires read:User.phone)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:User.phone".to_string()),
            },
            // Admin-only fields
            FieldDefinition {
                name:           "salary".to_string(),
                field_type:     FieldType::Int,
                nullable:       true,
                default_value:  None,
                description:    Some("User salary (admin only)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
            },
            FieldDefinition {
                name:           "ssn".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    Some("User SSN (admin only)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
            },
        ],
        description:         Some("User type with field-level scopes".to_string()),
        sql_source:          "users".to_string(),
        jsonb_column:        String::new(),
        sql_projection_hint: None,
        implements:          vec![],
    }
}

/// Helper to create a compiled schema with security config
/// Simulates compiler output merging schema + TOML config
fn create_compiled_schema_with_rbac() -> CompiledSchema {
    let user_type = create_user_type_with_scopes();

    let mut security_config = SecurityConfig::new();

    // Define roles matching a typical RBAC system
    security_config.add_role(RoleDefinition::new(
        "public".to_string(),
        vec![], // No scopes - completely restricted
    ));

    security_config.add_role(RoleDefinition::new(
        "viewer".to_string(),
        vec!["read:User.*".to_string()], // Can read all User fields
    ));

    security_config.add_role(RoleDefinition::new(
        "moderator".to_string(),
        vec!["read:User.*".to_string(), "write:User.name".to_string()], // Can read and moderate
    ));

    security_config.add_role(RoleDefinition::new(
        "admin".to_string(),
        vec!["*".to_string()], // Full access (global wildcard)
    ));

    security_config.default_role = Some("public".to_string());

    CompiledSchema {
        types:         vec![user_type],
        queries:       vec![],
        mutations:     vec![],
        enums:         vec![],
        input_types:   vec![],
        interfaces:    vec![],
        unions:        vec![],
        subscriptions: vec![],
        directives:    vec![],
        observers:     vec![],
        fact_tables:   HashMap::default(),
        federation:    None,
        security:      Some(serde_json::to_value(security_config).unwrap()),
        schema_sdl:    None,
    }
}

/// Helper to create a security context for a specific role
fn create_user_context(role: &str) -> SecurityContext {
    SecurityContext {
        user_id:          format!("user-{}", role),
        roles:            vec![role.to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-123".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

// ============================================================================
// RED Phase Tests: Cycle 6 - E2E Field RBAC Pipeline
// ============================================================================

#[test]
fn test_e2e_schema_field_scopes_compiled() {
    // RED: Schema with field scopes should be preserved through compilation
    // GIVEN: User type with mixed public/scoped fields (Python decorator output)
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    // WHEN: Schema is compiled
    // THEN: Field scopes should be preserved
    let public_fields = user_type.fields.iter().filter(|f| f.requires_scope.is_none()).count();
    let scoped_fields = user_type.fields.iter().filter(|f| f.requires_scope.is_some()).count();

    assert_eq!(public_fields, 2, "Should have 2 public fields (id, name)");
    assert_eq!(scoped_fields, 4, "Should have 4 scoped fields");
}

#[test]
fn test_e2e_role_definitions_from_toml() {
    // RED: TOML role definitions should be parsed and available at runtime
    // GIVEN: Compiled schema with TOML role definitions
    let schema = create_compiled_schema_with_rbac();

    // WHEN: Security config is extracted
    let security_value = schema.security.as_ref().expect("Should have security config");

    // THEN: Role definitions should be present
    let roles = &security_value["role_definitions"];
    assert!(roles.is_array(), "Should have role definitions array");

    let role_array = roles.as_array().unwrap();
    assert_eq!(role_array.len(), 4, "Should have 4 roles (public, viewer, moderator, admin)");

    // Verify specific roles
    let role_names: Vec<&str> = role_array.iter().filter_map(|r| r["name"].as_str()).collect();
    assert!(role_names.contains(&"viewer"));
    assert!(role_names.contains(&"admin"));
}

#[test]
fn test_e2e_viewer_cannot_access_admin_fields() {
    // RED: Viewer role should not access admin-only fields
    // GIVEN: Compiled schema with roles
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let viewer_context = create_user_context("viewer");

    // WHEN: Viewer filters fields
    let accessible = filter_fields(&viewer_context, &security_config, &user_type.fields);

    // THEN: Should not include admin fields
    let field_names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(!field_names.contains(&"salary"), "Viewer should not access salary");
    assert!(!field_names.contains(&"ssn"), "Viewer should not access ssn");

    // But should include public and readable fields
    assert!(field_names.contains(&"id"), "Should have id");
    assert!(field_names.contains(&"name"), "Should have name");
    assert!(field_names.contains(&"email"), "Should have email (viewer has read:User.*)");
}

#[test]
fn test_e2e_admin_accesses_all_fields() {
    // RED: Admin role with global wildcard should access all fields
    // GIVEN: Admin context and full schema
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let admin_context = create_user_context("admin");

    // WHEN: Admin filters fields
    let accessible = filter_fields(&admin_context, &security_config, &user_type.fields);

    // THEN: Should have all fields
    assert_eq!(accessible.len(), user_type.fields.len(), "Admin should access all fields");
}

#[test]
fn test_e2e_public_role_no_scoped_fields() {
    // RED: Public role with no scopes should only access public fields
    // GIVEN: Public role context
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let public_context = create_user_context("public");

    // WHEN: Public user filters fields
    let accessible = filter_fields(&public_context, &security_config, &user_type.fields);

    // THEN: Should only have public fields
    let field_names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(accessible.len(), 2, "Should only have 2 public fields");
    assert!(field_names.contains(&"id"));
    assert!(field_names.contains(&"name"));
}

#[test]
fn test_e2e_multiple_roles_aggregate() {
    // RED: User with multiple roles should aggregate scope access
    // GIVEN: Context with both viewer and moderator roles
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");

    let mut multi_role_context = create_user_context("viewer");
    multi_role_context.roles.push("moderator".to_string());

    // WHEN: User with multiple roles filters fields
    let accessible = filter_fields(&multi_role_context, &security_config, &user_type.fields);

    // THEN: Should have access to all fields both roles can access
    let field_names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(field_names.contains(&"id"));
    assert!(field_names.contains(&"name"));
    assert!(field_names.contains(&"email"), "viewer role grants this");
    assert!(field_names.contains(&"phone"), "viewer role grants this");
    assert!(!field_names.contains(&"salary"), "Neither role grants admin access");
}

#[test]
fn test_e2e_wildcard_matching_read_star() {
    // RED: read:User.* should match read:User.email
    // GIVEN: Viewer with read:User.* scope
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();

    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let viewer_context = create_user_context("viewer");

    // WHEN: Check if viewer can access email
    let can_access = can_access_field(&viewer_context, &security_config, email_field);

    // THEN: Should have access via wildcard matching
    assert!(can_access, "read:User.* should match read:User.email");
}

#[test]
fn test_e2e_global_wildcard_matches_all() {
    // RED: Global wildcard * should match any scope
    // GIVEN: Admin with * scope (global wildcard)
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let admin_context = create_user_context("admin");

    // WHEN: Admin checks access to any field
    for field in &user_type.fields {
        // THEN: Should have access to all
        let can_access = can_access_field(&admin_context, &security_config, field);
        assert!(can_access, "Admin with * should access {} field", field.name);
    }
}

#[test]
fn test_e2e_missing_role_defaults_to_public() {
    // RED: User without defined role should fall back to default role
    // GIVEN: Security config with default role
    let schema = create_compiled_schema_with_rbac();
    let security_value = schema.security.as_ref().expect("Should have security config");

    // WHEN: Check default role
    let default_role = &security_value["default_role"];

    // THEN: Should have default role set
    assert_eq!(default_role, "public", "Should default to public role");
}

#[test]
fn test_e2e_multi_tenant_field_filtering() {
    // RED: Field filtering should work with multi-tenant contexts
    // GIVEN: Multi-tenant context
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");

    let mut tenant_context = create_user_context("viewer");
    tenant_context.tenant_id = Some("tenant-acme".to_string());
    tenant_context
        .attributes
        .insert("region".to_string(), serde_json::Value::String("us-west".to_string()));

    // WHEN: Multi-tenant user filters fields
    let accessible = filter_fields(&tenant_context, &security_config, &user_type.fields);

    // THEN: Field filtering should work independently of tenant context
    let field_names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(field_names.contains(&"email"), "Should still filter based on role");
    assert!(!field_names.contains(&"salary"), "Should still respect role boundaries");
}

#[test]
fn test_e2e_scope_requirement_none_always_accessible() {
    // RED: Fields with no scope requirement should be accessible to anyone
    // GIVEN: Public role (no scopes) and public fields
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let id_field = user_type.fields.iter().find(|f| f.name == "id").unwrap();

    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");
    let public_context = create_user_context("public");

    // WHEN: Public user (zero scopes) checks access to public field
    let can_access = can_access_field(&public_context, &security_config, id_field);

    // THEN: Should have access (public fields are always accessible)
    assert!(can_access, "Public field should be accessible to user with no scopes");
}

// ============================================================================
// Additional E2E Scenarios: Advanced Pipelines
// ============================================================================

#[test]
fn test_e2e_exact_scope_match() {
    // SCENARIO: Field requires exact scope match
    // GIVEN: Viewer role with read:User.email (exact scope)
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();

    let mut security_config = SecurityConfig::new();
    security_config.add_role(RoleDefinition::new(
        "viewer".to_string(),
        vec!["read:User.email".to_string()], // Exact scope only
    ));

    let viewer_context = create_user_context("viewer");

    // WHEN: User with exact scope tries to access field with exact scope requirement
    let can_access = can_access_field(&viewer_context, &security_config, email_field);

    // THEN: Should have access
    assert!(can_access, "Exact scope match should grant access to required field");
}

#[test]
fn test_e2e_partial_wildcard_no_match() {
    // SCENARIO: Partial wildcard should not match unrelated scopes
    // GIVEN: User with read:Post.* (different type)
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();

    let mut security_config = SecurityConfig::new();
    security_config.add_role(RoleDefinition::new(
        "post_viewer".to_string(),
        vec!["read:Post.*".to_string()], // Different type
    ));

    let viewer_context = create_user_context("post_viewer");

    // WHEN: User tries to access User.email with Post.* scope
    let can_access = can_access_field(&viewer_context, &security_config, email_field);

    // THEN: Should not have access
    assert!(!can_access, "read:Post.* should not match read:User.email requirement");
}

#[test]
fn test_e2e_action_prefix_wildcard() {
    // SCENARIO: Action prefix wildcard (admin:*) should match any admin action
    // GIVEN: Admin role with admin:* scope
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let mut security_config = SecurityConfig::new();
    security_config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

    let admin_context = create_user_context("admin");

    // WHEN: Admin checks access to admin-only fields
    for field in &user_type.fields {
        if let Some(scope) = &field.requires_scope {
            if scope.starts_with("admin:") {
                // THEN: Should have access to admin fields
                let can_access = can_access_field(&admin_context, &security_config, field);
                assert!(can_access, "admin:* should match admin field requirement: {}", scope);
            }
        }
    }
}

#[test]
fn test_e2e_role_hierarchy_narrower_access() {
    // SCENARIO: Different roles provide different access levels
    // GIVEN: Viewer (narrow) and Admin (broad)
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let mut security_config = SecurityConfig::new();
    security_config.add_role(RoleDefinition::new(
        "viewer".to_string(),
        vec!["read:User.email".to_string()], // Only email
    ));
    security_config.add_role(RoleDefinition::new(
        "admin".to_string(),
        vec!["*".to_string()], // Everything
    ));

    let viewer_context = create_user_context("viewer");
    let admin_context = create_user_context("admin");

    // WHEN: Viewer filters fields
    let viewer_accessible = filter_fields(&viewer_context, &security_config, &user_type.fields);

    // AND: Admin filters fields
    let admin_accessible = filter_fields(&admin_context, &security_config, &user_type.fields);

    // THEN: Viewer should have fewer accessible fields
    assert!(
        viewer_accessible.len() < admin_accessible.len(),
        "Viewer should have fewer accessible fields than admin"
    );
    assert_eq!(admin_accessible.len(), user_type.fields.len(), "Admin should have all fields");
}

#[test]
fn test_e2e_empty_scopes_means_no_access() {
    // SCENARIO: Role with empty scopes vector should not access any scoped fields
    // GIVEN: Role with empty scopes
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let mut security_config = SecurityConfig::new();
    security_config.add_role(RoleDefinition::new(
        "restricted".to_string(),
        vec![], // No scopes
    ));

    let restricted_context = create_user_context("restricted");

    // WHEN: Restricted user filters fields
    let accessible = filter_fields(&restricted_context, &security_config, &user_type.fields);

    // THEN: Should only have public fields
    assert_eq!(accessible.len(), 2, "Should only have public fields (id, name)");
    for field in &accessible {
        assert!(field.requires_scope.is_none(), "Field {} should be public", field.name);
    }
}

#[test]
fn test_e2e_concurrent_user_contexts() {
    // SCENARIO: Multiple users with different roles should have independent access
    // GIVEN: Schema with two different user contexts
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize security config");

    let viewer_context = create_user_context("viewer");
    let admin_context = create_user_context("admin");

    // WHEN: Multiple users filter independently
    let viewer_fields = filter_fields(&viewer_context, &security_config, &user_type.fields);
    let admin_fields = filter_fields(&admin_context, &security_config, &user_type.fields);

    // THEN: Results should be different
    assert_ne!(
        viewer_fields.len(),
        admin_fields.len(),
        "Viewer and admin should see different field counts"
    );
    assert_eq!(admin_fields.len(), user_type.fields.len(), "Admin sees all fields");
    assert!(viewer_fields.len() < admin_fields.len(), "Viewer sees fewer fields");
}

#[test]
fn test_e2e_field_description_preserved() {
    // SCENARIO: Field metadata should be preserved through compilation
    // GIVEN: Compiled schema with field descriptions
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    // WHEN: Access field with description
    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();

    // THEN: Description should be present
    assert!(email_field.description.is_some(), "Field should have description");
    assert!(
        email_field.description.as_ref().unwrap().contains("read:User.email"),
        "Description should mention scope requirement"
    );
}

#[test]
fn test_e2e_schema_type_definitions_preserved() {
    // SCENARIO: Field types should be preserved
    // GIVEN: Compiled schema
    let schema = create_compiled_schema_with_rbac();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    // WHEN: Check field types
    let id_field = user_type.fields.iter().find(|f| f.name == "id").unwrap();
    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();

    // THEN: Types should be correct
    assert_eq!(id_field.field_type, FieldType::Int, "id should be Int");
    assert_eq!(email_field.field_type, FieldType::String, "email should be String");
}
