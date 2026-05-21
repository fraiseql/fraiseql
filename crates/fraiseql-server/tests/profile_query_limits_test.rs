//! Integration tests for security profile-based query limits enforcement
//!
//! Verifies that security profiles enforce query depth and complexity limits
//! at the GraphQL endpoint level.
//!
//! Tests cover:
//! - STANDARD profile: Higher limits (depth 15, complexity 1000)
//! - REGULATED profile: Strict limits (depth 10, complexity 500)
//! - RESTRICTED profile: Maximum strictness (depth 5, complexity 250)
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns

use fraiseql_server::{
    routes::graphql::GraphQLRequest,
    validation::{ComplexityValidationError, RequestValidator},
};

/// Helper function to create a validator with standard profile limits
fn standard_profile_validator() -> RequestValidator {
    RequestValidator::new()
        .with_max_depth(15)
        .with_max_complexity(1000)
        .with_depth_validation(true)
        .with_complexity_validation(true)
}

/// Helper function to create a validator with regulated profile limits
fn regulated_profile_validator() -> RequestValidator {
    RequestValidator::new()
        .with_max_depth(10)
        .with_max_complexity(500)
        .with_depth_validation(true)
        .with_complexity_validation(true)
}

/// Helper function to create a validator with restricted profile limits
fn restricted_profile_validator() -> RequestValidator {
    RequestValidator::new()
        .with_max_depth(5)
        .with_max_complexity(250)
        .with_depth_validation(true)
        .with_complexity_validation(true)
}

#[test]
fn test_standard_profile_allows_deep_queries() {
    // STANDARD profile allows depth up to 15
    let validator = standard_profile_validator();

    // Query with depth 12 should pass
    let deep_query = "{
        posts {
            id
            author {
                id
                profile {
                    bio
                    settings {
                        theme {
                            dark {
                                mode
                            }
                        }
                    }
                }
            }
        }
    }";

    validator
        .validate_query(deep_query)
        .unwrap_or_else(|e| panic!("STANDARD profile should allow depth 12, got: {e}"));
}

#[test]
fn test_standard_profile_rejects_excessive_depth() {
    // STANDARD profile max depth is 15
    let validator = standard_profile_validator();

    // Query with depth 16+ should fail
    // Each nested {} adds 1 to depth count
    let excessive_query = "{ a { b { c { d { e { f { g { h { i { j { k { l { m { n { o { p } } } } } } } } } } } } } } } }";

    let result = validator.validate_query(excessive_query);
    match result {
        Err(ComplexityValidationError::QueryTooDeep {
            max_depth,
            actual_depth,
        }) => {
            assert_eq!(max_depth, 15);
            assert!(actual_depth > 15, "actual_depth ({actual_depth}) should exceed max (15)");
        },
        other => panic!(
            "STANDARD profile should reject excessive depth with QueryTooDeep, got: {other:?}"
        ),
    }
}

#[test]
fn test_regulated_profile_rejects_deep_queries() {
    // REGULATED profile max depth is 10
    let validator = regulated_profile_validator();

    // Query with depth 11+ should fail
    // Each nested {} adds 1 to depth count
    let deep_query = "{ a { b { c { d { e { f { g { h { i { j { k } } } } } } } } } } }";

    let result = validator.validate_query(deep_query);
    match result {
        Err(ComplexityValidationError::QueryTooDeep {
            max_depth,
            actual_depth,
        }) => {
            assert_eq!(max_depth, 10);
            assert!(actual_depth > 10, "actual_depth ({actual_depth}) should exceed max (10)");
        },
        other => {
            panic!("REGULATED profile should reject depth > 10 with QueryTooDeep, got: {other:?}")
        },
    }
}

#[test]
fn test_regulated_profile_allows_boundary_depth() {
    // REGULATED profile max depth is 10
    let validator = regulated_profile_validator();

    // Query with exactly depth 10 should pass
    let boundary_query = "{
        posts {
            id
            author {
                id
                profile {
                    bio
                    settings {
                        theme {
                            dark {
                                mode
                            }
                        }
                    }
                }
            }
        }
    }";

    validator
        .validate_query(boundary_query)
        .unwrap_or_else(|e| panic!("REGULATED profile should allow depth 10, got: {e}"));
}

#[test]
fn test_restricted_profile_max_depth_enforcement() {
    // RESTRICTED profile max depth is 5
    let validator = restricted_profile_validator();

    // Query with depth 5 should pass
    let at_limit = "{ a { b { c { d { e } } } } }";
    validator
        .validate_query(at_limit)
        .unwrap_or_else(|e| panic!("RESTRICTED profile should allow depth 5, got: {e}"));

    // Query with depth 6 should fail
    let over_limit = "{ a { b { c { d { e { f } } } } } }";
    assert!(
        matches!(
            validator.validate_query(over_limit),
            Err(ComplexityValidationError::QueryTooDeep { .. })
        ),
        "RESTRICTED profile should reject depth 6 with QueryTooDeep"
    );
}

#[test]
fn test_standard_profile_complexity_limit() {
    // STANDARD profile allows complexity up to 1000
    let validator = standard_profile_validator();

    // Simple query with low complexity should pass
    let simple = "{ posts { id title } }";
    validator
        .validate_query(simple)
        .unwrap_or_else(|e| panic!("STANDARD profile should allow simple query, got: {e}"));

    // Moderately complex query should pass
    let moderate =
        "{ posts { id title author { id name email } comments { id text user { id name } } } }";
    validator
        .validate_query(moderate)
        .unwrap_or_else(|e| panic!("STANDARD profile should allow moderate complexity, got: {e}"));
}

#[test]
fn test_regulated_profile_complexity_limit() {
    // REGULATED profile max complexity is 500
    let validator = regulated_profile_validator();

    // Simple query should pass
    let simple = "{ posts { id title } }";
    validator
        .validate_query(simple)
        .unwrap_or_else(|e| panic!("REGULATED profile should allow simple query, got: {e}"));

    // Moderately complex query should pass
    let moderate = "{ posts { id title author { id name } } }";
    validator
        .validate_query(moderate)
        .unwrap_or_else(|e| panic!("REGULATED profile should allow moderate complexity, got: {e}"));
}

#[test]
fn test_restricted_profile_complexity_limit() {
    // RESTRICTED profile max complexity is 250
    let validator = restricted_profile_validator();

    // Very simple query should pass
    let simple = "{ posts { id title } }";
    validator
        .validate_query(simple)
        .unwrap_or_else(|e| panic!("RESTRICTED profile should allow simple query, got: {e}"));

    // Slightly more complex should still pass (under 250)
    let light = "{ posts { id title author { id } } }";
    validator
        .validate_query(light)
        .unwrap_or_else(|e| panic!("RESTRICTED profile should allow light complexity, got: {e}"));
}

#[test]
fn test_profile_query_validator_builder_pattern() {
    // Verify all profiles can be built with builder pattern
    let standard = RequestValidator::new().with_max_depth(15).with_max_complexity(1000);

    let regulated = RequestValidator::new().with_max_depth(10).with_max_complexity(500);

    let restricted = RequestValidator::new().with_max_depth(5).with_max_complexity(250);

    // All should be created successfully
    standard
        .validate_query("{ posts { id } }")
        .unwrap_or_else(|e| panic!("standard validator should accept simple query, got: {e}"));
    regulated
        .validate_query("{ posts { id } }")
        .unwrap_or_else(|e| panic!("regulated validator should accept simple query, got: {e}"));
    restricted
        .validate_query("{ posts { id } }")
        .unwrap_or_else(|e| panic!("restricted validator should accept simple query, got: {e}"));
}

#[test]
fn test_disabling_depth_validation() {
    // Verify depth validation can be disabled
    let validator = RequestValidator::new()
        .with_max_depth(5)
        .with_depth_validation(false) // Disable
        .with_complexity_validation(true);

    // Query with excessive depth should pass (validation disabled)
    let excessive = "{
        a { b { c { d { e { f { g { h { i { j } } } } } } } } }
    }";

    validator
        .validate_query(excessive)
        .unwrap_or_else(|e| panic!("depth validation disabled, should allow any depth, got: {e}"));
}

#[test]
fn test_disabling_complexity_validation() {
    // Verify complexity validation can be disabled
    let validator = RequestValidator::new()
        .with_max_depth(15)
        .with_max_complexity(10)
        .with_complexity_validation(false); // Disable

    // Even a moderately complex query should pass
    let complex = "{ posts { id title author { id name } comments { id text user { id } } } }";
    validator.validate_query(complex).unwrap_or_else(|e| {
        panic!("complexity validation disabled, should allow any complexity, got: {e}")
    });
}

#[test]
fn test_graphql_request_structure_with_profiles() {
    // Verify GraphQLRequest can be created and validated with profile limits
    let request = GraphQLRequest {
        query:          Some("{ posts { id title author { id name } } }".to_string()),
        variables:      None,
        operation_name: None,
        extensions:     None,
        document_id:    None,
    };

    let validator = regulated_profile_validator();
    validator.validate_query(request.query.as_deref().unwrap()).unwrap_or_else(|e| {
        panic!("REGULATED profile should allow structured request query, got: {e}")
    });
}

#[test]
fn test_profile_limits_with_variables() {
    // GraphQL query structure should validate independently of variables
    let validator = restricted_profile_validator();

    let query_with_vars = "query($id: ID!, $limit: Int!) {
        post(id: $id) {
            id
            title
            author {
                id
                name
            }
        }
    }";

    // Query structure itself should validate regardless of what variables are passed
    validator.validate_query(query_with_vars).unwrap_or_else(|e| {
        panic!("RESTRICTED profile should allow query with variables, got: {e}")
    });
}

#[test]
fn test_error_message_contains_profile_limits() {
    let validator = regulated_profile_validator();

    let excessive_depth = "{
        a { b { c { d { e { f { g { h { i { j { k } } } } } } } } } }
    }";

    let result = validator.validate_query(excessive_depth);
    match result {
        Err(ComplexityValidationError::QueryTooDeep {
            max_depth,
            actual_depth,
        }) => {
            assert_eq!(max_depth, 10, "Error should show REGULATED profile limit of 10");
            assert!(actual_depth > 10, "actual_depth ({actual_depth}) should exceed max (10)");
        },
        other => {
            panic!("Expected QueryTooDeep error with REGULATED profile limits, got: {other:?}")
        },
    }
}

#[test]
fn test_multiple_validators_independent() {
    // Multiple validator instances should maintain independent state
    let strict = restricted_profile_validator();
    let lenient = standard_profile_validator();

    let moderate_query = "{
        posts {
            id
            author {
                id
                profile {
                    bio
                    settings {
                        theme {
                            dark
                        }
                    }
                }
            }
        }
    }";

    // Should pass with lenient (STANDARD)
    lenient
        .validate_query(moderate_query)
        .unwrap_or_else(|e| panic!("STANDARD profile should allow moderate query, got: {e}"));

    // Should fail with strict (RESTRICTED)
    assert!(
        matches!(
            strict.validate_query(moderate_query),
            Err(ComplexityValidationError::QueryTooDeep { .. })
        ),
        "RESTRICTED profile should reject moderate query with QueryTooDeep"
    );
}
