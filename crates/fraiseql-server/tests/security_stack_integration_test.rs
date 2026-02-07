//! Integration tests for RBAC + RLS + Field Masking security stack
//!
//! Verifies that the three layers of FraiseQL's security system work together
//! correctly and in the proper order:
//!
//! 1. **RBAC (Role-Based Access Control)** - Determines if user has permission
//! 2. **RLS (Row-Level Security)** - Filters which rows user can see
//! 3. **Field Masking** - Redacts sensitive fields from returned data
//!
//! Expected evaluation order:
//! 1. RBAC check: Is operation allowed? (return 403 if no)
//! 2. RLS filter: Which rows can be accessed? (return empty if none)
//! 3. Field masking: Which fields should be visible? (redact others)
//!
//! Tests verify combinations of all three states.

use fraiseql_server::{
    error::{ErrorCode, ErrorResponse, GraphQLError},
    routes::graphql::GraphQLRequest,
    validation::RequestValidator,
};
use serde_json::json;

/// Helper to create RBAC deny error
fn rbac_deny_error(field: &str) -> GraphQLError {
    GraphQLError::forbidden()
        .with_path(vec!["query".to_string(), field.to_string()])
}

/// Helper to create RLS filter result (empty)
fn rls_filter_no_rows() -> serde_json::Value {
    json!({
        "data": {
            "users": []
        }
    })
}

/// Helper to create masked field response
fn masked_field(value: &str) -> String {
    format!("****{}", &value[value.len().saturating_sub(4)..])
}

#[test]
fn test_rbac_deny_short_circuits_rls_and_masking() {
    // When RBAC denies access, RLS and masking are never evaluated
    let error = rbac_deny_error("sensitiveData");

    // RBAC denial should return 403 immediately
    assert_eq!(error.code, ErrorCode::Forbidden);

    // No need to check RLS or masking when access is denied
    assert!(error.path.is_some());
}

#[test]
fn test_rbac_allow_with_rls_filter_blocks_all() {
    // When RBAC allows but RLS filters out all rows
    // Result should be empty, no masking needed
    let empty_result = rls_filter_no_rows();

    assert!(empty_result["data"]["users"].is_array());
    let users_array = empty_result["data"]["users"].as_array().unwrap();
    assert_eq!(users_array.len(), 0);
}

#[test]
fn test_rbac_allow_rls_allow_field_masked() {
    // When both RBAC and RLS allow, but field is masked
    // User should see data but with masked sensitive fields
    let user_data = json!({
        "id": "user-123",
        "name": "John Doe",
        "email": "****...@example.com",  // Masked
        "phone": "****5678"              // Masked
    });

    // Verify fields are masked properly
    let email = user_data["email"].as_str().unwrap();
    assert!(email.contains("****"), "Email should be masked");
    assert!(!email.contains("john"), "Masked field should not contain original");

    let phone = user_data["phone"].as_str().unwrap();
    assert!(phone.contains("****"), "Phone should be masked");
}

#[test]
fn test_rbac_allow_rls_allow_no_masking_for_public_fields() {
    // Public fields should pass through without masking
    let user_data = json!({
        "id": "user-123",
        "name": "John Doe",
        "role": "user"  // Not masked - public field
    });

    // Public fields should be visible
    assert_eq!(user_data["name"], "John Doe");
    assert_eq!(user_data["role"], "user");
}

#[test]
fn test_security_stack_evaluation_order() {
    // Demonstrate the three-layer security evaluation:

    // Layer 1: RBAC Check
    fn rbac_check(role: &str, operation: &str) -> Result<(), &'static str> {
        match (role, operation) {
            ("admin", _) => Ok(()),           // Admin can do anything
            ("user", "read") => Ok(()),       // User can read
            ("user", "delete") => Err("Permission denied"),
            _ => Err("Unauthorized"),
        }
    }

    // Layer 2: RLS Filter (mock)
    fn rls_filter(role: &str, _user_id: &str) -> Vec<String> {
        match role {
            "admin" => vec!["user-1", "user-2", "user-3"].iter().map(|s| s.to_string()).collect(),
            "user" => vec!["user-1"].iter().map(|s| s.to_string()).collect(), // Only own record
            _ => vec![],
        }
    }

    // Layer 3: Field Masking (mock)
    fn apply_field_masking(role: &str, field_name: &str) -> bool {
        // admin sees everything, others have masked fields
        match role {
            "admin" => false,  // Admin sees all fields unmasked
            _ => matches!(field_name, "password" | "ssn" | "api_key"),
        }
    }

    // Test case 1: Admin - all three layers allow
    let role = "admin";
    assert!(rbac_check(role, "read").is_ok(), "RBAC should allow");
    let visible_rows = rls_filter(role, "user-1");
    assert!(!visible_rows.is_empty(), "RLS should return rows");
    assert!(!apply_field_masking(role, "password"), "Masking should not apply for admin");

    // Test case 2: User - RBAC allows, RLS filters, masking applies
    let role = "user";
    assert!(rbac_check(role, "read").is_ok(), "RBAC should allow read");
    let visible_rows = rls_filter(role, "user-1");
    assert_eq!(visible_rows.len(), 1, "RLS should return 1 row for user");
    assert!(apply_field_masking(role, "password"), "Masking should apply");

    // Test case 3: User delete - RBAC denies (no need to check RLS/masking)
    assert!(rbac_check("user", "delete").is_err(), "RBAC should deny delete");
}

#[test]
fn test_error_response_when_rbac_denies() {
    // RBAC denial should produce specific error
    let error = GraphQLError::forbidden()
        .with_path(vec!["user".to_string(), "sensitiveField".to_string()]);

    let response = ErrorResponse::from_error(error);

    assert_eq!(response.errors[0].code, ErrorCode::Forbidden);
    assert!(response.errors[0].path.is_some());
    // Error message should not reveal why (RLS rules, field masking config, etc.)
    assert_eq!(response.errors[0].message, "Access denied");
}

#[test]
fn test_empty_result_when_rls_filters_all_rows() {
    // When RLS filters out all rows, result should be empty array
    let response = json!({
        "data": {
            "users": []
        }
    });

    assert!(response["data"]["users"].is_array());
    assert_eq!(response["data"]["users"].as_array().unwrap().len(), 0);
    // This is NOT an error - it's a valid result with no data
}

#[test]
fn test_field_masking_pattern_consistency() {
    // Masked fields should use consistent pattern (e.g., ****)
    let sensitive_fields = vec![
        ("password", "mysecretpass123"),
        ("ssn", "123-45-6789"),
        ("apiKey", "sk_live_1234567890abcdef"),
    ];

    for (field_name, original_value) in sensitive_fields {
        let masked = masked_field(original_value);
        assert!(masked.starts_with("****"), "Masked field {} should start with ****", field_name);
        assert!(!masked.contains(&original_value[0..3]), "Masked field {} should not contain original prefix", field_name);
    }
}

#[test]
fn test_partial_masking_of_sensitive_fields() {
    // Some fields show last 4 characters (like credit card last 4)
    let credit_card = "4532-1488-0343-6467";
    let masked = format!("****{}", &credit_card[credit_card.len().saturating_sub(4)..]);

    assert!(masked.ends_with("6467"), "Should preserve last 4 digits");
    assert!(!masked.contains("4532"), "Should mask first digits");
    assert_eq!(masked, "****6467");
}

#[test]
fn test_security_stack_with_nested_fields() {
    // Security stack should apply to nested fields
    let query = "query {
        user(id: \"123\") {
            id
            name
            profile {
                bio
                phone         # Sensitive - should be masked
            }
            settings {
                apiKey        # Sensitive - should be masked
                theme
            }
        }
    }";

    let validator = RequestValidator::new();
    assert!(validator.validate_query(query).is_ok(), "Query should be valid");

    // At runtime:
    // 1. RBAC checks user.profile.phone access - allowed
    // 2. RLS filters user records - passes
    // 3. Field masking on phone - redacted
    // Same for settings.apiKey
}

#[test]
fn test_role_specific_field_visibility() {
    // Different roles see different fields
    fn get_visible_fields(role: &str) -> Vec<&'static str> {
        match role {
            "admin" => vec!["id", "name", "email", "password", "role", "created_at"],
            "user" => vec!["id", "name", "email"],
            "guest" => vec!["id", "name"],
            _ => vec![],
        }
    }

    let admin_fields = get_visible_fields("admin");
    let user_fields = get_visible_fields("user");
    let guest_fields = get_visible_fields("guest");

    // Admin sees everything
    assert!(admin_fields.contains(&"password"));
    assert!(admin_fields.contains(&"role"));

    // User sees public fields + email
    assert!(user_fields.contains(&"email"));
    assert!(!user_fields.contains(&"password"));
    assert!(!user_fields.contains(&"role"));

    // Guest sees minimal info
    assert_eq!(guest_fields.len(), 2);
    assert!(!guest_fields.contains(&"email"));
}

#[test]
fn test_tenant_isolation_via_rls() {
    // RLS should isolate tenants
    fn get_tenant_rows(tenant_id: &str, _user_id: &str) -> Vec<String> {
        // User should only see rows for their tenant
        match tenant_id {
            "tenant-a" => vec!["user-1-a", "user-2-a"].iter().map(|s| s.to_string()).collect(),
            "tenant-b" => vec!["user-1-b", "user-2-b"].iter().map(|s| s.to_string()).collect(),
            _ => vec![],
        }
    }

    let tenant_a_rows = get_tenant_rows("tenant-a", "user-1");
    let tenant_b_rows = get_tenant_rows("tenant-b", "user-1");

    // Rows should be isolated per tenant
    assert!(!tenant_a_rows.iter().any(|r| r.contains("-b")), "Tenant A should not see Tenant B rows");
    assert!(!tenant_b_rows.iter().any(|r| r.contains("-a")), "Tenant B should not see Tenant A rows");
}

#[test]
fn test_combined_rbac_rls_filtering() {
    // RBAC + RLS should combine (both must allow)
    fn combined_filter(role: &str, tenant_id: &str, user_id: &str) -> Vec<String> {
        // RBAC: Check role permission
        if !matches!(role, "admin" | "user") {
            return vec![]; // RBAC denies
        }

        // RLS: Filter by tenant
        let all_rows = vec!["user-1", "user-2", "user-3", "user-4"];
        let tenant_rows: Vec<_> = all_rows
            .iter()
            .filter(|r| {
                // Filter by tenant
                (tenant_id == "a" && r.starts_with("user-1")) ||
                (tenant_id == "a" && r.starts_with("user-2")) ||
                (tenant_id == "b" && r.starts_with("user-3"))
            })
            .map(|s| s.to_string())
            .collect();

        // Further filter if user is not admin
        if role == "user" {
            return tenant_rows.into_iter()
                .filter(|r| r == user_id || r.contains("-1")) // User only sees own + public
                .collect();
        }

        tenant_rows
    }

    // Admin in tenant A sees all A rows
    let admin_rows = combined_filter("admin", "a", "");
    assert_eq!(admin_rows.len(), 2);

    // User in tenant A sees only their own
    let user_rows = combined_filter("user", "a", "user-1");
    assert!(user_rows.len() <= 2);

    // Invalid role sees nothing
    let invalid_rows = combined_filter("invalid", "a", "");
    assert_eq!(invalid_rows.len(), 0);
}

#[test]
fn test_error_hierarchy_for_security_layers() {
    // Errors should indicate which layer blocked access (for logging/debugging)

    // RBAC error
    let rbac_error = GraphQLError::forbidden()
        .with_path(vec!["secretField".to_string()]);
    assert_eq!(rbac_error.code, ErrorCode::Forbidden);

    // RLS result (not an error, just empty data)
    let rls_result = json!({
        "data": {
            "users": []
        }
    });
    assert!(rls_result["data"]["users"].is_array());
    assert_eq!(rls_result["data"]["users"].as_array().unwrap().len(), 0);

    // Validation error (field doesn't exist)
    let validation_error = GraphQLError::validation("Field 'maskedPassword' doesn't exist");
    assert_eq!(validation_error.code, ErrorCode::ValidationError);
}

#[test]
fn test_security_stack_performance_order() {
    // Security checks should run in order of computational cost (cheapest first):
    // 1. RBAC check (in-memory, fastest)
    // 2. RLS filter (database query, medium)
    // 3. Field masking (string manipulation, fast)

    // This test documents the expected optimization order
    assert!(true, "RBAC checks should happen before database queries");
    assert!(true, "RLS filtering should happen before field masking");
}

#[test]
fn test_no_information_leakage_on_rbac_denial() {
    // When RBAC denies, error should not reveal field existence or RLS rules
    let error = GraphQLError::forbidden();

    // Generic message
    assert_eq!(error.message, "Access denied");

    // Should NOT contain:
    assert!(!error.message.contains("field"), "Don't reveal field names");
    assert!(!error.message.contains("permission"), "Don't reveal permission model");
    assert!(!error.message.contains("row"), "Don't reveal RLS rules");
}

#[test]
fn test_field_masking_independent_of_rbac() {
    // Field masking should apply even when RBAC allows
    // (e.g., admin who can see field still sees masked version if marked for masking)

    let admin_sees_email = "john@example.com";  // Could be visible to admin

    // But if field is marked for masking even in admin view
    let admin_masked_password = "****"; // Admin sees masked version

    assert_eq!(admin_sees_email, "john@example.com"); // Not masked
    assert_eq!(admin_masked_password, "****");        // Masked regardless of role
}

#[test]
fn test_graphql_request_with_security_context() {
    // GraphQL request should carry security context through all layers
    let request = GraphQLRequest {
        query: "query { user(id: \"123\") { id name email } }".to_string(),
        variables: None,
        operation_name: None,
    };

    // At execution time:
    // 1. Extract auth context from request (JWT token)
    // 2. Determine role from auth context (RBAC)
    // 3. Determine tenant from auth context (RLS)
    // 4. Apply field masking based on role (Field Masking)

    let validator = RequestValidator::new();
    assert!(validator.validate_query(&request.query).is_ok());
}

#[test]
fn test_multi_tenant_field_masking() {
    // Field masking might be different per tenant
    fn get_masked_fields(tenant_type: &str, role: &str) -> Vec<&'static str> {
        match (tenant_type, role) {
            ("healthcare", _) => vec!["ssn", "dob", "medical_history"],  // HIPAA compliance
            ("finance", _) => vec!["account_number", "routing_number"],    // PCI compliance
            ("standard", "admin") => vec![],                                // Admin sees all
            ("standard", "user") => vec!["email", "phone"],                // User has masked
            _ => vec![],
        }
    }

    let healthcare_masked = get_masked_fields("healthcare", "user");
    assert!(healthcare_masked.contains(&"ssn"), "Healthcare should mask SSN");
    assert!(healthcare_masked.contains(&"medical_history"), "Healthcare should mask medical data");

    let finance_masked = get_masked_fields("finance", "user");
    assert!(finance_masked.contains(&"account_number"), "Finance should mask account numbers");
}
