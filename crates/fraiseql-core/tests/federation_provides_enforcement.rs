//! Phase 1, Cycle 4c: @provides Directive Enforcement Tests
//!
//! Tests for @provides directive validation at runtime:
//! - @provides fields should be present in resolver results
//! - Warning (not error) if @provides contract is broken
//! - @provides is a hint about what fields the resolver provides
//! - Validation across both database and HTTP resolvers
//!
//! RED PHASE: These tests validate @provides contract enforcement

use fraiseql_core::federation::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection, KeyDirective,
};
use serde_json::json;

// ============================================================================
// Test: @provides Contract Declaration
// ============================================================================

#[test]
fn test_provides_field_present_in_result() {
    // TEST: @provides fields that are present in result
    // GIVEN: Order.shippingEstimate provides "weight"
    // WHEN: The resolver returns weight in the result
    // THEN: Contract is fulfilled, no warning

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    // Result contains the promised field
    let result = json!({
        "id": "456",
        "weight": 2.5,
        "shippingEstimate": 15.99
    });

    let validation = validate_provides_contract(&metadata, "Order", "shippingEstimate", &result);
    assert!(
        validation.warnings.is_empty(),
        "Should have no warnings when @provides field is present"
    );
    assert!(validation.success);
}

#[test]
fn test_provides_field_missing_in_result() {
    // TEST: @provides fields missing from result triggers warning
    // GIVEN: Order.shippingEstimate provides "weight"
    // WHEN: The resolver result does not include weight
    // THEN: Warning is generated (not error - @provides is informational)

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    // Result missing the weight field
    let result = json!({
        "id": "456",
        "shippingEstimate": 15.99
    });

    let validation = validate_provides_contract(&metadata, "Order", "shippingEstimate", &result);
    assert!(
        !validation.warnings.is_empty(),
        "Should have warning when @provides field is missing"
    );
    assert!(validation.success, "@provides is informational, should not fail");
    assert!(
        validation.warnings[0].to_lowercase().contains("weight"),
        "Warning should mention missing field"
    );
}

#[test]
fn test_provides_multiple_fields() {
    // TEST: Multiple @provides declarations
    // GIVEN: Field provides multiple fields
    // WHEN: Validating result
    // THEN: Each promised field is validated

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "details".to_string(),
        FieldFederationDirectives::new()
            .add_provides(FieldPathSelection {
                path:     vec!["weight".to_string()],
                typename: "Order".to_string(),
            })
            .add_provides(FieldPathSelection {
                path:     vec!["dimensions".to_string()],
                typename: "Order".to_string(),
            }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    // Missing one of two provided fields
    let result = json!({
        "id": "456",
        "weight": 2.5
        // Missing dimensions
    });

    let validation = validate_provides_contract(&metadata, "Order", "details", &result);
    assert!(!validation.warnings.is_empty(), "Should warn about missing dimensions");
    assert!(validation.success, "Should not fail");
}

#[test]
fn test_provides_no_directives() {
    // TEST: Fields without @provides have no validation
    // GIVEN: Field has no @provides directive
    // WHEN: Validating result
    // THEN: Should pass regardless of content

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    // name field has no @provides directive

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    let result = json!({
        "id": "123",
        "name": "Alice"
    });

    let validation = validate_provides_contract(&metadata, "User", "name", &result);
    assert!(validation.success);
    assert!(validation.warnings.is_empty());
}

#[test]
fn test_provides_nested_field_path() {
    // TEST: @provides can include nested paths like "address.city"
    // GIVEN: Field provides nested "address.city"
    // WHEN: Validating result with nested object
    // THEN: Should validate nested structure

    let mut user_type = FederatedType::new("User".to_string());
    user_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    user_type.set_field_directives(
        "profile".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["address".to_string(), "city".to_string()],
            typename: "User".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![user_type],
    };

    // Result with nested structure
    let result = json!({
        "id": "123",
        "address": {
            "city": "San Francisco",
            "zip": "94105"
        }
    });

    let validation = validate_provides_contract(&metadata, "User", "profile", &result);
    assert!(validation.success);
    assert!(validation.warnings.is_empty(), "Should find nested field");
}

// ============================================================================
// Test: Warning Generation and Context
// ============================================================================

#[test]
fn test_provides_warning_context() {
    // TEST: @provides warnings should include helpful context
    // GIVEN: Field promises "weight" but result doesn't include it
    // WHEN: Validation runs
    // THEN: Warning includes type, field, promised field

    let mut order_type = FederatedType::new("Order".to_string());
    order_type.keys.push(KeyDirective {
        fields:     vec!["id".to_string()],
        resolvable: true,
    });
    order_type.set_field_directives(
        "shippingEstimate".to_string(),
        FieldFederationDirectives::new().add_provides(FieldPathSelection {
            path:     vec!["weight".to_string()],
            typename: "Order".to_string(),
        }),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![order_type],
    };

    let result = json!({"id": "456"});

    let validation = validate_provides_contract(&metadata, "Order", "shippingEstimate", &result);
    assert!(!validation.warnings.is_empty());

    let warning = &validation.warnings[0];
    assert!(
        warning.to_lowercase().contains("order")
            || warning.to_lowercase().contains("shippingestimate")
            || warning.to_lowercase().contains("weight"),
        "Warning should mention type, field, or promised field: {}",
        warning
    );
}

// ============================================================================
// Helper types and functions for @provides validation
// ============================================================================

/// Result of @provides contract validation
#[derive(Debug, Clone)]
struct ProvidesValidationResult {
    /// Whether validation passed (always true for @provides as it's informational)
    pub success:  bool,
    /// Any warnings about unfulfilled @provides contracts
    pub warnings: Vec<String>,
}

/// Validate @provides contract for a resolver result
///
/// Checks that a resolver result includes all fields promised by @provides
/// directives. Unlike @requires, @provides violations generate warnings, not
/// errors, since @provides is a hint about what a resolver provides.
///
/// The @provides directive is a contract between the resolver and the router,
/// declaring which fields the resolver will include in its response. This is
/// mainly informational for documentation and caching hints, not a strict
/// requirement.
///
/// # Arguments
/// * `metadata` - Federation metadata with type and directive definitions
/// * `typename` - Type name being resolved (e.g., "Order")
/// * `field_name` - Field name being resolved (e.g., "shippingEstimate")
/// * `result` - The resolver result (JSON value returned by resolver)
///
/// # Returns
/// `ProvidesValidationResult` with success=true and optional warnings about
/// fields that were promised but not delivered
///
/// # Example
/// ```ignore
/// // Order.shippingEstimate @provides weight but result doesn't include it
/// let result = json!({"id": "123"});
/// let validation = validate_provides_contract(&metadata, "Order", "shippingEstimate", &result);
/// // Returns: success=true, warnings=["Contract Warning: @provides field missing..."]
/// ```
fn validate_provides_contract(
    metadata: &FederationMetadata,
    typename: &str,
    field_name: &str,
    result: &serde_json::Value,
) -> ProvidesValidationResult {
    let mut warnings = Vec::new();

    // Find the type in metadata
    if let Some(federated_type) = metadata.types.iter().find(|t| t.name == typename) {
        // Check if field has @provides directives
        if let Some(directives) = federated_type.get_field_directives(field_name) {
            // Validate each promised field is present
            for promised in &directives.provides {
                if !has_field_in_result(result, &promised.path) {
                    let field_path = promised.path.join(".");
                    warnings.push(format!(
                        "Contract Warning: @provides field missing from result\n\
                         Type: {}\n\
                         Field: {}\n\
                         Promised: {}\n\
                         Issue: Field {}.{} declared @provides({}) but result does not include it\n\
                         Suggestion: Add {} to resolver result or remove @provides declaration",
                        typename,
                        field_name,
                        field_path,
                        typename,
                        field_name,
                        field_path,
                        field_path
                    ));
                }
            }
        }
    }

    // @provides is informational, never causes validation failure
    ProvidesValidationResult {
        success: true,
        warnings,
    }
}

/// Check if a field path exists in a JSON result
///
/// Supports both simple field names (e.g., ["weight"]) and nested paths
/// (e.g., ["address", "city"] for result.address.city).
///
/// # Arguments
/// * `result` - JSON value to search
/// * `path` - Field path components to locate
///
/// # Returns
/// `true` if the field/path exists in the result, `false` otherwise
///
/// # Example
/// ```ignore
/// let result = json!({"user": {"address": {"city": "SF"}}});
/// assert!(has_field_in_result(&result, &["user".into(), "address".into(), "city".into()]));
/// assert!(!has_field_in_result(&result, &["user".into(), "email".into()]));
/// ```
fn has_field_in_result(result: &serde_json::Value, path: &[String]) -> bool {
    if path.is_empty() {
        return false;
    }

    // For simple single-component paths
    if path.len() == 1 {
        return result.get(&path[0]).is_some();
    }

    // For nested paths, traverse each component
    let mut current = result;
    for component in path {
        current = match current.get(component) {
            Some(value) => value,
            None => return false,
        };
    }

    true
}
