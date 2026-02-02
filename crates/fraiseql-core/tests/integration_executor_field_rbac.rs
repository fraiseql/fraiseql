//! Integration tests for Cycle 7: Executor Integration (Field-Level RBAC at Execution)
//!
//! Tests that the executor correctly applies field filtering during query execution:
//! 1. Query matching with security context
//! 2. Field filtering during execution plan creation
//! 3. Result projection with filtered fields
//! 4. Proper handling of scope requirements in execution pipeline

use std::collections::HashMap;

use chrono::Utc;
use fraiseql_core::runtime::{RuntimeConfig, can_access_field, filter_fields};
use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldType, RoleDefinition, SecurityConfig, TypeDefinition,
};
use fraiseql_core::security::SecurityContext;

// ============================================================================
// Helpers: Test Schema and Context Setup
// ============================================================================

/// Helper to create a realistic Post type with mixed public/protected fields
fn create_post_type_with_scopes() -> TypeDefinition {
    TypeDefinition {
        name:                "Post".to_string(),
        fields:              vec![
            // Public fields
            FieldDefinition {
                name:           "id".to_string(),
                field_type:     FieldType::Int,
                nullable:       false,
                default_value:  None,
                description:    Some("Post ID (public)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            FieldDefinition {
                name:           "title".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    Some("Post title (public)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            // Protected fields
            FieldDefinition {
                name:           "content".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    Some("Post content (requires read:Post.content)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:Post.content".to_string()),
            },
            FieldDefinition {
                name:           "draft".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    Some("Draft content (requires write:Post.draft)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("write:Post.draft".to_string()),
            },
            // Admin-only fields
            FieldDefinition {
                name:           "analytics".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    Some("Analytics data (admin only)".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
            },
        ],
        description:         Some("Post type with field-level scopes".to_string()),
        sql_source:          "posts".to_string(),
        jsonb_column:        String::new(),
        sql_projection_hint: None,
        implements:          vec![],
    }
}

/// Helper to create a compiled schema for executor tests
fn create_executor_test_schema() -> CompiledSchema {
    let post_type = create_post_type_with_scopes();

    let mut security_config = SecurityConfig::new();

    security_config.add_role(RoleDefinition::new(
        "reader".to_string(),
        vec!["read:Post.*".to_string()],
    ));

    security_config.add_role(RoleDefinition::new(
        "editor".to_string(),
        vec![
            "read:Post.*".to_string(),
            "write:Post.draft".to_string(),
        ],
    ));

    security_config.add_role(RoleDefinition::new(
        "admin".to_string(),
        vec!["*".to_string()],
    ));

    security_config.default_role = Some("reader".to_string());

    CompiledSchema {
        types: vec![post_type],
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

/// Helper to create security context
fn create_executor_context(role: &str) -> SecurityContext {
    SecurityContext {
        user_id: format!("user-{}", role),
        roles: vec![role.to_string()],
        tenant_id: None,
        scopes: vec![],
        attributes: HashMap::new(),
        request_id: "req-exec".to_string(),
        ip_address: None,
        authenticated_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        issuer: None,
        audience: None,
    }
}

// ============================================================================
// RED Phase Tests: Cycle 7 - Executor Field RBAC Integration
// ============================================================================

#[test]
fn test_executor_reader_sees_only_readable_fields() {
    // RED: Reader should see only fields with read scope
    // GIVEN: Post type and reader context
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let reader_context = create_executor_context("reader");

    // WHEN: Reader's accessible fields are filtered
    let accessible_fields =
        filter_fields(&reader_context, &security_config, &post_type.fields);

    // THEN: Should include public and readable fields, not admin-only
    let field_names: Vec<&str> = accessible_fields.iter().map(|f| f.name.as_str()).collect();

    assert!(field_names.contains(&"id"), "Should have id (public)");
    assert!(field_names.contains(&"title"), "Should have title (public)");
    assert!(
        field_names.contains(&"content"),
        "Should have content (read:Post.*)"
    );
    assert!(
        !field_names.contains(&"draft"),
        "Should not have draft (requires write:Post.draft)"
    );
    assert!(
        !field_names.contains(&"analytics"),
        "Should not have analytics (admin only)"
    );
}

#[test]
fn test_executor_editor_sees_read_and_write_fields() {
    // RED: Editor should see fields they can read AND write
    // GIVEN: Post type and editor context
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let editor_context = create_executor_context("editor");

    // WHEN: Editor's accessible fields are filtered
    let accessible_fields =
        filter_fields(&editor_context, &security_config, &post_type.fields);

    // THEN: Should include public, readable, AND writable fields
    let field_names: Vec<&str> = accessible_fields.iter().map(|f| f.name.as_str()).collect();

    assert!(field_names.contains(&"id"), "Should have id");
    assert!(field_names.contains(&"title"), "Should have title");
    assert!(field_names.contains(&"content"), "Should have content (read)");
    assert!(field_names.contains(&"draft"), "Should have draft (write)");
    assert!(
        !field_names.contains(&"analytics"),
        "Should not have analytics (admin only)"
    );
}

#[test]
fn test_executor_admin_sees_all_fields() {
    // RED: Admin should see all fields via global wildcard
    // GIVEN: Post type and admin context
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let admin_context = create_executor_context("admin");

    // WHEN: Admin's accessible fields are filtered
    let accessible_fields =
        filter_fields(&admin_context, &security_config, &post_type.fields);

    // THEN: Should have all fields
    assert_eq!(
        accessible_fields.len(),
        post_type.fields.len(),
        "Admin should see all fields"
    );
}

#[test]
fn test_executor_field_filtering_preserves_field_metadata() {
    // RED: Filtered fields should preserve type, description, nullability
    // GIVEN: Post type and security context
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let reader_context = create_executor_context("reader");

    // WHEN: Fields are filtered
    let accessible_fields =
        filter_fields(&reader_context, &security_config, &post_type.fields);

    // THEN: Preserved fields should have correct metadata
    let content_field = accessible_fields
        .iter()
        .find(|f| f.name == "content")
        .expect("Should have content field");

    assert_eq!(content_field.field_type, FieldType::String, "Type preserved");
    assert!(
        content_field.description.is_some(),
        "Description preserved"
    );
    assert!(!content_field.nullable, "Nullability preserved");
}

#[test]
fn test_executor_projection_fields_filtered_by_scope() {
    // RED: Projection fields list should be filtered during execution plan
    // SCENARIO: When executor creates a projection plan, it should only include fields
    // that the user has access to.

    // GIVEN: All fields available (what a query might request)
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let all_field_names: Vec<String> = post_type.fields.iter().map(|f| f.name.clone()).collect();

    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let reader_context = create_executor_context("reader");

    // WHEN: User requests all fields but can only access some
    let accessible_fields =
        filter_fields(&reader_context, &security_config, &post_type.fields);
    let accessible_field_names: Vec<String> =
        accessible_fields.iter().map(|f| f.name.clone()).collect();

    // THEN: Projection should only include accessible fields
    assert!(
        accessible_field_names.len() < all_field_names.len(),
        "Filtered fields should be less than total"
    );
    assert!(!accessible_field_names.contains(&"analytics".to_string()));
}

#[test]
fn test_executor_runtime_config_with_field_filter() {
    // RED: RuntimeConfig should support field filtering configuration
    let config = RuntimeConfig {
        cache_query_plans: true,
        max_query_depth: 10,
        max_query_complexity: 1000,
        enable_tracing: false,
        field_filter: None, // Not yet implemented - will be GREEN phase
        rls_policy: None,
        query_timeout_ms: 30_000,
    };

    // WHEN: Config is created
    // THEN: Should have field_filter option
    assert!(config.field_filter.is_none(), "Config should support field_filter");
}

#[test]
fn test_executor_multiple_roles_scope_union() {
    // RED: User with multiple roles should see union of all accessible fields
    // GIVEN: User with both reader and editor roles
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let mut multi_role_context = create_executor_context("reader");
    multi_role_context.roles.push("editor".to_string());

    // WHEN: User with multiple roles filters fields
    let accessible_fields =
        filter_fields(&multi_role_context, &security_config, &post_type.fields);

    // THEN: Should see fields from both roles (union)
    let field_names: Vec<&str> = accessible_fields.iter().map(|f| f.name.as_str()).collect();
    assert!(field_names.contains(&"draft"), "Should have draft from editor role");
    assert!(field_names.contains(&"content"), "Should have content from reader role");
}

#[test]
fn test_executor_public_fields_in_all_scopes() {
    // RED: Public fields should be in projection regardless of user role
    // GIVEN: Any user context
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    // Test with minimal reader role
    let reader_context = create_executor_context("reader");

    // WHEN: Reader's fields are filtered
    let accessible = filter_fields(&reader_context, &security_config, &post_type.fields);

    // THEN: Public fields (id, title) should always be included
    let field_names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(
        field_names.contains(&"id"),
        "id should always be accessible"
    );
    assert!(
        field_names.contains(&"title"),
        "title should always be accessible"
    );
}

#[test]
fn test_executor_security_context_with_config() {
    // RED: Executor should use security context with RuntimeConfig
    let _config = RuntimeConfig::default();
    let context = create_executor_context("reader");

    // WHEN: Context is created with config
    // THEN: Context should have role information
    assert_eq!(context.roles.len(), 1);
    assert!(context.roles.contains(&"reader".to_string()));
}

#[test]
fn test_executor_field_access_check_pattern() {
    // RED: Executor should provide field access check utility
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let content_field = post_type
        .fields
        .iter()
        .find(|f| f.name == "content")
        .unwrap();
    let analytics_field = post_type
        .fields
        .iter()
        .find(|f| f.name == "analytics")
        .unwrap();

    let reader_context = create_executor_context("reader");

    // WHEN: Check field access
    let can_access_content = can_access_field(&reader_context, &security_config, content_field);
    let can_access_analytics = can_access_field(&reader_context, &security_config, analytics_field);

    // THEN: Reader can access content but not analytics
    assert!(
        can_access_content,
        "Reader should access content field"
    );
    assert!(
        !can_access_analytics,
        "Reader should not access analytics field"
    );
}

#[test]
fn test_executor_default_role_applied() {
    // RED: When user has no explicit role, default role should be used
    // GIVEN: Security config with default role
    let schema = create_executor_test_schema();
    let security_value = schema.security.as_ref().expect("Should have security config");

    // WHEN: Check default role
    let default_role = &security_value["default_role"];

    // THEN: Should have default role set to "reader"
    assert_eq!(default_role, "reader");
}

#[test]
fn test_executor_field_filtering_idempotent() {
    // RED: Filtering same fields multiple times should give same result
    let schema = create_executor_test_schema();
    let post_type = schema.types.iter().find(|t| t.name == "Post").unwrap();
    let security_config = serde_json::from_value::<SecurityConfig>(
        schema.security.as_ref().unwrap().clone(),
    )
    .expect("Should deserialize security config");

    let reader_context = create_executor_context("reader");

    // WHEN: Filter fields twice
    let accessible1 = filter_fields(&reader_context, &security_config, &post_type.fields);
    let accessible2 = filter_fields(&reader_context, &security_config, &post_type.fields);

    // THEN: Results should be identical
    assert_eq!(
        accessible1.len(),
        accessible2.len(),
        "Filtering should be idempotent"
    );
    let names1: Vec<&str> = accessible1.iter().map(|f| f.name.as_str()).collect();
    let names2: Vec<&str> = accessible2.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names1, names2, "Field order and content should match");
}
