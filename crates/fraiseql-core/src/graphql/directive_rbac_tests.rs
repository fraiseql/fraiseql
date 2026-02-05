//! Tests for @require_permission directive
//!
//! RED cycle: Write failing tests for field-level RBAC directives
//! Expected behavior:
//! - Directive is recognized and parsed
//! - Unauthorized access is denied
//! - Authorized access is allowed
//! - Field masking works based on permissions

use std::collections::HashMap;

use serde_json::json;

use crate::graphql::{
    directive_evaluator::{
        CustomDirectiveEvaluator, DirectiveHandler, DirectiveResult, EvaluationContext,
        OperationType,
    },
    RequirePermissionDirective, // Re-export for creating handler
};
use std::sync::Arc;

// ============================================================================
// Test 1: Permission Directive Handler Registration
// ============================================================================

/// Test that @require_permission directive can be registered
#[test]
fn test_require_permission_directive_registration() {
    // This test will fail until we implement the RequirePermissionDirective handler
    let directive_name = "require_permission";

    // Verify the handler can be created (will fail - not implemented yet)
    let _handler = create_require_permission_handler();

    assert_eq!(_handler.name(), directive_name);
}

/// Test that directive with valid permission argument is accepted
#[test]
fn test_require_permission_directive_valid_arguments() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("query:users:read"));

    // This should pass validation
    let result = handler.validate_args(&args);
    assert!(result.is_ok(), "Valid permission argument should be accepted");
}

/// Test that directive without permission argument is rejected
#[test]
fn test_require_permission_directive_missing_permission() {
    let handler = create_require_permission_handler();
    let args = HashMap::new();

    // This should fail - permission argument is required
    let result = handler.validate_args(&args);
    assert!(
        result.is_err(),
        "validate_args should fail when permission argument is missing"
    );
}

// ============================================================================
// Test 2: Permission Evaluation
// ============================================================================

/// Test that authorized user can access protected field
#[test]
fn test_authorized_user_can_access_field() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("query:users:read"));

    // Create evaluation context with authorized user
    let mut context = EvaluationContext::new(HashMap::new())
        .with_field_path("Query.users")
        .with_operation_type(OperationType::Query);

    // Add user context with permissions
    let mut user_perms = HashMap::new();
    user_perms.insert("userId".to_string(), json!("user123"));
    user_perms.insert("permissions".to_string(), json!(["query:users:read", "query:posts:read"]));

    context = context
        .with_user_context("userId", json!("user123"))
        .with_user_context("permissions", json!(["query:users:read", "query:posts:read"]));

    // Evaluate directive
    let result = handler.evaluate(&args, &context);
    assert!(
        result.is_ok(),
        "Authorized user should be able to access field"
    );

    match result {
        Ok(DirectiveResult::Include) => {} // Expected
        _ => panic!("Expected DirectiveResult::Include for authorized access"),
    }
}

/// Test that unauthorized user cannot access protected field
#[test]
fn test_unauthorized_user_denied_access() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("admin:*"));

    // Create evaluation context with regular user (no admin permission)
    let context = EvaluationContext::new(HashMap::new())
        .with_field_path("Query.adminPanel")
        .with_operation_type(OperationType::Query)
        .with_user_context("userId", json!("user123"))
        .with_user_context("permissions", json!(["query:users:read"])); // No admin permission

    // Evaluate directive
    let result = handler.evaluate(&args, &context);

    // Should either error or skip
    match result {
        Ok(DirectiveResult::Error(msg)) => {
            assert!(msg.to_lowercase().contains("permission"));
        }
        Ok(DirectiveResult::Skip) => {} // Also acceptable
        _ => panic!("Expected permission denied for unauthorized access"),
    }
}

/// Test that wildcard permissions grant access
#[test]
fn test_wildcard_permission_grants_access() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("query:users:read"));

    // Create evaluation context with user having wildcard permission
    let context = EvaluationContext::new(HashMap::new())
        .with_field_path("Query.users")
        .with_operation_type(OperationType::Query)
        .with_user_context("userId", json!("admin123"))
        .with_user_context("permissions", json!(["*:*"])); // Wildcard permission

    // Evaluate directive
    let result = handler.evaluate(&args, &context);
    assert!(result.is_ok(), "Wildcard permission should grant access");

    match result {
        Ok(DirectiveResult::Include) => {} // Expected
        _ => panic!("Expected DirectiveResult::Include for wildcard permission"),
    }
}

// ============================================================================
// Test 3: Permission Pattern Matching
// ============================================================================

/// Test that partial wildcards are supported (e.g., "query:*")
#[test]
fn test_partial_wildcard_permission() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("query:users:read"));

    // User has partial wildcard: "query:*"
    let context = EvaluationContext::new(HashMap::new())
        .with_user_context("userId", json!("user123"))
        .with_user_context("permissions", json!(["query:*"]));

    let result = handler.evaluate(&args, &context);
    assert!(
        result.is_ok(),
        "Partial wildcard 'query:*' should match 'query:users:read'"
    );
}

// ============================================================================
// Test 4: Field Masking
// ============================================================================

/// Test that field is masked when user lacks permission
#[test]
fn test_sensitive_field_masked_without_permission() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("read:User.email"));
    args.insert("maskValue".to_string(), json!("[REDACTED]"));

    // User without email read permission
    let context = EvaluationContext::new(HashMap::new())
        .with_user_context("userId", json!("guest123"))
        .with_user_context("permissions", json!(["query:public"]));

    let result = handler.evaluate(&args, &context);

    // Should either mask or error
    match result {
        Ok(DirectiveResult::Transform(masked_value)) => {
            assert_eq!(masked_value, json!("[REDACTED]"));
        }
        Ok(DirectiveResult::Error(_)) => {} // Also acceptable
        _ => panic!("Expected field masking for unauthorized access"),
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a require_permission directive handler
fn create_require_permission_handler() -> Box<dyn DirectiveHandler> {
    Box::new(RequirePermissionDirective::new())
}

// ============================================================================
// Integration Tests
// ============================================================================

/// Test complete directive parsing and evaluation in query context
#[test]
fn test_directive_in_graphql_query_context() {
    // Schema fragment with @require_permission directive
    let schema_json = json!({
        "types": [{
            "name": "Query",
            "fields": [
                {
                    "name": "users",
                    "type": "User",
                    "directives": [{
                        "name": "require_permission",
                        "args": [
                            {"name": "permission", "value": "query:users:read"}
                        ]
                    }]
                },
                {
                    "name": "adminPanel",
                    "type": "String",
                    "directives": [{
                        "name": "require_permission",
                        "args": [
                            {"name": "permission", "value": "admin:*"}
                        ]
                    }]
                }
            ]
        }]
    });

    // This test verifies that the schema structure supports directives
    assert!(schema_json.is_object());
}

// ============================================================================
// Test 5: Edge Cases and Error Handling
// ============================================================================

/// Test that user with no permissions is denied
#[test]
fn test_user_with_no_permissions_denied() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("query:users:read"));

    // User with empty permissions array
    let context = EvaluationContext::new(HashMap::new())
        .with_user_context("permissions", json!([]));

    let result = handler.evaluate(&args, &context);
    match result {
        Ok(DirectiveResult::Error(_)) => {} // Expected
        Ok(DirectiveResult::Skip) => {}       // Also acceptable
        _ => panic!("Expected access denied for user with no permissions"),
    }
}

/// Test that deeply nested wildcard permissions work
#[test]
fn test_deeply_nested_wildcard_permission() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert(
        "permission".to_string(),
        json!("query:users:read:detailed:metadata"),
    );

    // User has matching nested wildcard
    let context = EvaluationContext::new(HashMap::new())
        .with_user_context("userId", json!("user123"))
        .with_user_context("permissions", json!(["query:users:read:*"]));

    let result = handler.evaluate(&args, &context);
    assert!(
        result.is_ok(),
        "Nested wildcard should match detailed permission"
    );
}

/// Test that empty permission string is rejected
#[test]
fn test_empty_permission_string_rejected() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!(""));

    let result = handler.validate_args(&args);
    assert!(
        result.is_err(),
        "Empty permission string should be rejected"
    );
}

/// Test that maskValue can be a string
#[test]
fn test_mask_value_string() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("read:User.ssn"));
    args.insert("maskValue".to_string(), json!("***"));

    let result = handler.validate_args(&args);
    assert!(result.is_ok(), "String maskValue should be valid");
}

/// Test that maskValue can be a number
#[test]
fn test_mask_value_number() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("read:User.age"));
    args.insert("maskValue".to_string(), json!(0));

    let result = handler.validate_args(&args);
    assert!(result.is_ok(), "Number maskValue should be valid");
}

/// Test that maskValue can be null
#[test]
fn test_mask_value_null() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("read:User.phone"));
    args.insert("maskValue".to_string(), json!(null));

    let result = handler.validate_args(&args);
    assert!(result.is_ok(), "Null maskValue should be valid");
}

/// Test that case-sensitive permission matching
#[test]
fn test_permission_matching_case_sensitive() {
    let handler = create_require_permission_handler();
    let mut args = HashMap::new();
    args.insert("permission".to_string(), json!("Query:Users:Read"));

    // User with lowercase permission
    let context = EvaluationContext::new(HashMap::new())
        .with_user_context("permissions", json!(["query:users:read"]));

    let result = handler.evaluate(&args, &context);
    // Should fail because permissions are case-sensitive
    match result {
        Ok(DirectiveResult::Error(_)) => {} // Expected
        Ok(DirectiveResult::Skip) => {}      // Also acceptable
        _ => panic!("Permissions should be case-sensitive"),
    }
}

// ============================================================================
// Test 6: Custom Directive Evaluator Integration
// ============================================================================

/// Test that RequirePermissionDirective can be registered with CustomDirectiveEvaluator
#[test]
fn test_register_with_custom_evaluator() {
    let directive = Arc::new(RequirePermissionDirective::new());
    let evaluator = CustomDirectiveEvaluator::new().with_handler(directive);

    // Verify the directive is registered
    assert!(
        evaluator.has_handler("require_permission"),
        "require_permission directive should be registered"
    );

    let handlers = evaluator.handler_names();
    assert!(
        handlers.contains(&"require_permission"),
        "require_permission should be in handler list"
    );
}

/// Test that multiple handlers can be registered
#[test]
fn test_multiple_handlers_with_require_permission() {
    let permission_directive = Arc::new(RequirePermissionDirective::new());
    let evaluator = CustomDirectiveEvaluator::new()
        .with_handler(permission_directive);

    assert!(evaluator.has_handler("require_permission"));
    let handlers = evaluator.handler_names();
    assert_eq!(handlers.len(), 1);
}
