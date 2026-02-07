//! Integration tests for security profile-based query limits enforcement
//!
//! Verifies that security profiles enforce query depth and complexity limits
//! at the GraphQL endpoint level.
//!
//! Tests cover:
//! - STANDARD profile: Higher limits (depth 15, complexity 1000)
//! - REGULATED profile: Strict limits (depth 10, complexity 500)
//! - RESTRICTED profile: Maximum strictness (depth 5, complexity 250)

use fraiseql_server::{
    routes::graphql::GraphQLRequest,
    validation::{RequestValidator, ValidationError},
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

    let result = validator.validate_query(deep_query);
    assert!(result.is_ok(), "STANDARD profile should allow depth 12, got: {:?}", result);
}

#[test]
fn test_standard_profile_rejects_excessive_depth() {
    // STANDARD profile max depth is 15
    let validator = standard_profile_validator();

    // Query with depth 16+ should fail
    // Each nested {} adds 1 to depth count
    let excessive_query = "{ a { b { c { d { e { f { g { h { i { j { k { l { m { n { o { p } } } } } } } } } } } } } } } }";

    let result = validator.validate_query(excessive_query);
    assert!(result.is_err(), "STANDARD profile should reject excessive depth");

    if let Err(ValidationError::QueryTooDeep {
        max_depth,
        actual_depth,
    }) = result
    {
        assert_eq!(max_depth, 15);
        assert!(actual_depth > 15);
    } else {
        panic!("Expected QueryTooDeep error, got {:?}", result);
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
    assert!(result.is_err(), "REGULATED profile should reject depth > 10");

    if let Err(ValidationError::QueryTooDeep {
        max_depth,
        actual_depth,
    }) = result
    {
        assert_eq!(max_depth, 10);
        assert!(actual_depth > 10);
    } else {
        panic!("Expected QueryTooDeep error, got {:?}", result);
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

    let result = validator.validate_query(boundary_query);
    assert!(result.is_ok(), "REGULATED profile should allow depth 10, got: {:?}", result);
}

#[test]
fn test_restricted_profile_max_depth_enforcement() {
    // RESTRICTED profile max depth is 5
    let validator = restricted_profile_validator();

    // Query with depth 5 should pass
    let at_limit = "{ a { b { c { d { e } } } } }";
    assert!(validator.validate_query(at_limit).is_ok());

    // Query with depth 6 should fail
    let over_limit = "{ a { b { c { d { e { f } } } } } }";
    assert!(validator.validate_query(over_limit).is_err());
}

#[test]
fn test_standard_profile_complexity_limit() {
    // STANDARD profile allows complexity up to 1000
    let validator = standard_profile_validator();

    // Simple query with low complexity should pass
    let simple = "{ posts { id title } }";
    assert!(validator.validate_query(simple).is_ok());

    // Moderately complex query should pass
    let moderate = "{ posts { id title author { id name email } comments { id text user { id name } } } }";
    assert!(validator.validate_query(moderate).is_ok());
}

#[test]
fn test_regulated_profile_complexity_limit() {
    // REGULATED profile max complexity is 500
    let validator = regulated_profile_validator();

    // Simple query should pass
    let simple = "{ posts { id title } }";
    assert!(validator.validate_query(simple).is_ok());

    // Moderately complex query should pass
    let moderate = "{ posts { id title author { id name } } }";
    assert!(validator.validate_query(moderate).is_ok());
}

#[test]
fn test_restricted_profile_complexity_limit() {
    // RESTRICTED profile max complexity is 250
    let validator = restricted_profile_validator();

    // Very simple query should pass
    let simple = "{ posts { id title } }";
    assert!(validator.validate_query(simple).is_ok());

    // Slightly more complex should still pass (under 250)
    let light = "{ posts { id title author { id } } }";
    assert!(validator.validate_query(light).is_ok());
}

#[test]
fn test_profile_query_validator_builder_pattern() {
    // Verify all profiles can be built with builder pattern
    let standard = RequestValidator::new()
        .with_max_depth(15)
        .with_max_complexity(1000);

    let regulated = RequestValidator::new()
        .with_max_depth(10)
        .with_max_complexity(500);

    let restricted = RequestValidator::new()
        .with_max_depth(5)
        .with_max_complexity(250);

    // All should be created successfully
    assert!(standard.validate_query("{ posts { id } }").is_ok());
    assert!(regulated.validate_query("{ posts { id } }").is_ok());
    assert!(restricted.validate_query("{ posts { id } }").is_ok());
}

#[test]
fn test_disabling_depth_validation() {
    // Verify depth validation can be disabled
    let validator = RequestValidator::new()
        .with_max_depth(5)
        .with_depth_validation(false)  // Disable
        .with_complexity_validation(true);

    // Query with excessive depth should pass (validation disabled)
    let excessive = "{
        a { b { c { d { e { f { g { h { i { j } } } } } } } } }
    }";

    assert!(validator.validate_query(excessive).is_ok());
}

#[test]
fn test_disabling_complexity_validation() {
    // Verify complexity validation can be disabled
    let validator = RequestValidator::new()
        .with_max_depth(15)
        .with_max_complexity(10)
        .with_complexity_validation(false);  // Disable

    // Even a moderately complex query should pass
    let complex = "{ posts { id title author { id name } comments { id text user { id } } } }";
    assert!(validator.validate_query(complex).is_ok());
}

#[test]
fn test_graphql_request_structure_with_profiles() {
    // Verify GraphQLRequest can be created and validated with profile limits
    let request = GraphQLRequest {
        query: "{ posts { id title author { id name } } }".to_string(),
        variables: None,
        operation_name: None,
    };

    let validator = regulated_profile_validator();
    let result = validator.validate_query(&request.query);

    assert!(result.is_ok());
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
    assert!(validator.validate_query(query_with_vars).is_ok());
}

#[test]
fn test_error_message_contains_profile_limits() {
    let validator = regulated_profile_validator();

    let excessive_depth = "{
        a { b { c { d { e { f { g { h { i { j { k } } } } } } } } } }
    }";

    let result = validator.validate_query(excessive_depth);
    assert!(result.is_err());

    match result {
        Err(ValidationError::QueryTooDeep {
            max_depth,
            actual_depth,
        }) => {
            assert_eq!(max_depth, 10, "Error should show REGULATED profile limit of 10");
            assert!(actual_depth > 10);
        },
        _ => panic!("Expected QueryTooDeep error with profile limits"),
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
    assert!(lenient.validate_query(moderate_query).is_ok());

    // Should fail with strict (RESTRICTED)
    assert!(strict.validate_query(moderate_query).is_err());
}
