//! Integration tests for Cycle 8: Field Filtering Error Handling & Edge Cases
//!
//! Tests error handling and edge cases when field filtering is applied:
//! 1. User requests fields they can't access
//! 2. Partial field filtering (some accessible, some not)
//! 3. Query with no accessible fields
//! 4. Nested field filtering behavior
//! 5. Field filtering with pagination
//! 6. Graceful degradation vs errors
//! 7. Error messages for access denied

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
// Helpers: Test Setup
// ============================================================================

/// Helper to create a schema with various access levels
fn create_schema_with_mixed_fields() -> CompiledSchema {
    let user_type = TypeDefinition {
        name:                "User".to_string(),
        fields:              vec![
            FieldDefinition {
                name:           "id".to_string(),
                field_type:     FieldType::Int,
                nullable:       false,
                default_value:  None,
                description:    Some("Public ID".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            FieldDefinition {
                name:           "publicInfo".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    Some("Public information".to_string()),
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
            },
            FieldDefinition {
                name:           "email".to_string(),
                field_type:     FieldType::String,
                nullable:       false,
                default_value:  None,
                description:    None,
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
                description:    None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:User.phone".to_string()),
            },
            FieldDefinition {
                name:           "ssn".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
            },
            FieldDefinition {
                name:           "bankAccount".to_string(),
                field_type:     FieldType::String,
                nullable:       true,
                default_value:  None,
                description:    None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
            },
        ],
        description:         Some("User with mixed access levels".to_string()),
        sql_source:          "users".to_string(),
        jsonb_column:        String::new(),
        sql_projection_hint: None,
        implements:          vec![],
    };

    let mut security_config = SecurityConfig::new();

    security_config
        .add_role(RoleDefinition::new("viewer".to_string(), vec!["read:User.*".to_string()]));

    security_config.add_role(RoleDefinition::new(
        "restricted".to_string(),
        vec![], // No scopes at all
    ));

    security_config.add_role(RoleDefinition::new("admin".to_string(), vec!["*".to_string()]));

    security_config.default_role = Some("viewer".to_string());

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

/// Helper to create context
fn create_context(role: &str) -> SecurityContext {
    SecurityContext {
        user_id:          format!("user-{}", role),
        roles:            vec![role.to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-error".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

// ============================================================================
// Error Handling & Edge Cases Tests
// ============================================================================

#[test]
fn test_field_filtering_partial_access() {
    // RED: When user requests fields they can't all access, should filter appropriately
    // GIVEN: User requests all fields but can only access some
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // Request all fields
    let all_fields = &user_type.fields;

    // WHEN: Filter fields
    let accessible = filter_fields(&viewer_context, &security_config, all_fields);

    // THEN: Should return only accessible fields
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();

    // Accessible: id, publicInfo (public), email, phone (viewer has read:User.*)
    // Not accessible: ssn, bankAccount (admin only)
    assert!(names.contains(&"id"));
    assert!(names.contains(&"publicInfo"));
    assert!(names.contains(&"email"));
    assert!(names.contains(&"phone"));
    assert!(!names.contains(&"ssn"));
    assert!(!names.contains(&"bankAccount"));
    assert_eq!(accessible.len(), 4, "Should have 4 accessible fields");
}

#[test]
fn test_field_filtering_all_fields_denied() {
    // RED: If user has no scopes, should only see public fields
    // GIVEN: Restricted user (no scopes)
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let restricted_context = create_context("restricted");

    // WHEN: Filter all fields with restricted role
    let accessible = filter_fields(&restricted_context, &security_config, &user_type.fields);

    // THEN: Should only have public fields
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"id"));
    assert!(names.contains(&"publicInfo"));
    assert!(!names.contains(&"email"));
    assert!(!names.contains(&"phone"));
    assert!(!names.contains(&"ssn"));
    assert!(!names.contains(&"bankAccount"));
    assert_eq!(accessible.len(), 2, "Should only have 2 public fields");
}

#[test]
fn test_field_filtering_empty_request() {
    // RED: If user requests empty field list, should return empty
    // GIVEN: Empty field request
    let schema = create_schema_with_mixed_fields();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");
    let empty_fields = vec![];

    // WHEN: Filter empty field list
    let accessible = filter_fields(&viewer_context, &security_config, &empty_fields);

    // THEN: Should return empty
    assert_eq!(accessible.len(), 0, "Empty input should return empty");
}

#[test]
fn test_field_filtering_respects_field_order() {
    // RED: Filtered fields should maintain order from original request
    // GIVEN: Specific field order requested
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // Request in specific order (using owned copies)
    let ordered_request = vec![
        user_type.fields[4].clone(), // ssn (admin only - not accessible)
        user_type.fields[2].clone(), // email (accessible)
        user_type.fields[0].clone(), // id (accessible)
        user_type.fields[5].clone(), // bankAccount (admin only - not accessible)
        user_type.fields[1].clone(), // publicInfo (accessible)
    ];

    // WHEN: Filter fields
    let accessible = filter_fields(&viewer_context, &security_config, &ordered_request);

    // THEN: Order should be preserved from request
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["email", "id", "publicInfo"]);
}

#[test]
fn test_field_filtering_duplicate_requests() {
    // RED: If same field requested multiple times, filter should handle it
    // GIVEN: Duplicate fields in request
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // Request with duplicates (using owned copies)
    let duplicates = vec![
        user_type.fields[0].clone(), // id
        user_type.fields[0].clone(), // id (duplicate)
        user_type.fields[2].clone(), // email
        user_type.fields[2].clone(), // email (duplicate)
    ];

    // WHEN: Filter fields with duplicates
    let accessible = filter_fields(&viewer_context, &security_config, &duplicates);

    // THEN: Should preserve duplicates as provided (field filtering doesn't deduplicate)
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names.len(), 4, "Should preserve duplicates");
    assert_eq!(names, vec!["id", "id", "email", "email"]);
}

#[test]
fn test_field_access_denied_for_single_field() {
    // RED: can_access_field should return false for denied fields
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let restricted_context = create_context("restricted");

    let email_field = user_type.fields.iter().find(|f| f.name == "email").unwrap();
    let ssn_field = user_type.fields.iter().find(|f| f.name == "ssn").unwrap();

    // WHEN: Check access to restricted fields
    let can_access_email = can_access_field(&restricted_context, &security_config, email_field);
    let can_access_ssn = can_access_field(&restricted_context, &security_config, ssn_field);

    // THEN: Should deny both (restricted has no scopes)
    assert!(!can_access_email, "Restricted user cannot access email");
    assert!(!can_access_ssn, "Restricted user cannot access ssn");
}

#[test]
fn test_field_access_public_fields_always_allowed() {
    // RED: Public fields should always be accessible regardless of role
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let restricted_context = create_context("restricted");

    let id_field = user_type.fields.iter().find(|f| f.name == "id").unwrap();
    let public_info_field = user_type.fields.iter().find(|f| f.name == "publicInfo").unwrap();

    // WHEN: Check access to public fields
    let can_access_id = can_access_field(&restricted_context, &security_config, id_field);
    let can_access_public_info =
        can_access_field(&restricted_context, &security_config, public_info_field);

    // THEN: Should allow both (public fields have no scope requirement)
    assert!(can_access_id, "Public id should be accessible");
    assert!(can_access_public_info, "Public publicInfo should be accessible");
}

#[test]
fn test_field_filtering_with_null_fields() {
    // RED: Nullable fields should be filtered same as non-nullable
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // WHEN: Filter fields including nullable ones
    let accessible = filter_fields(&viewer_context, &security_config, &user_type.fields);

    // THEN: Both nullable and non-nullable scoped fields should be filtered
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();

    // email is non-nullable with scope, should be included
    // phone is nullable with scope, should be included
    assert!(names.contains(&"email"));
    assert!(names.contains(&"phone"));
}

#[test]
fn test_field_filtering_consistency_across_calls() {
    // RED: Multiple calls should return consistent results
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // WHEN: Filter fields multiple times
    let result1 = filter_fields(&viewer_context, &security_config, &user_type.fields);
    let result2 = filter_fields(&viewer_context, &security_config, &user_type.fields);
    let result3 = filter_fields(&viewer_context, &security_config, &user_type.fields);

    // THEN: All results should be identical
    assert_eq!(result1.len(), result2.len());
    assert_eq!(result2.len(), result3.len());

    let names1: Vec<&str> = result1.iter().map(|f| f.name.as_str()).collect();
    let names2: Vec<&str> = result2.iter().map(|f| f.name.as_str()).collect();
    let names3: Vec<&str> = result3.iter().map(|f| f.name.as_str()).collect();

    assert_eq!(names1, names2);
    assert_eq!(names2, names3);
}

#[test]
fn test_field_filtering_mixed_nullability() {
    // RED: Nullable settings should not affect filtering logic
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let viewer_context = create_context("viewer");

    // Filter
    let accessible = filter_fields(&viewer_context, &security_config, &user_type.fields);

    // THEN: Should have mixed nullable/non-nullable based on scope, not nullability
    let mut has_nullable = false;
    let mut has_non_nullable = false;

    for field in &accessible {
        if field.nullable {
            has_nullable = true;
        } else {
            has_non_nullable = true;
        }
    }

    assert!(
        has_nullable && has_non_nullable,
        "Should have both nullable and non-nullable fields"
    );
}

#[test]
fn test_field_filtering_empty_security_config() {
    // RED: If security config is empty/missing, should allow all public fields
    // (graceful degradation)
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();

    let empty_config = SecurityConfig::new();

    let viewer_context = create_context("viewer");

    // WHEN: Filter with minimal config (no roles defined)
    let accessible = filter_fields(&viewer_context, &empty_config, &user_type.fields);

    // THEN: Should only return public fields (no role to grant scopes)
    let names: Vec<&str> = accessible.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"id"));
    assert!(names.contains(&"publicInfo"));
    assert!(!names.contains(&"email"));
}

#[test]
fn test_field_filtering_preserves_metadata_on_filtered() {
    // RED: Even when fields are filtered out, remaining fields should have correct metadata
    let schema = create_schema_with_mixed_fields();
    let user_type = schema.types.iter().find(|t| t.name == "User").unwrap();
    let security_config =
        serde_json::from_value::<SecurityConfig>(schema.security.as_ref().unwrap().clone())
            .expect("Should deserialize");

    let restricted_context = create_context("restricted");

    // WHEN: Filter fields
    let accessible = filter_fields(&restricted_context, &security_config, &user_type.fields);

    // THEN: All returned fields should have complete metadata
    for field in &accessible {
        assert!(!field.name.is_empty(), "Field name must be present");
        // Other metadata should be intact
    }
}
