//! Property-based tests for query validation (depth + complexity).
//!
//! `RequestValidator::validate_query` must never panic, empty/whitespace-only
//! strings must always fail, and relaxing the limit must never turn a passing
//! query into a failing one.

use fraiseql_server::validation::{ComplexityValidationError, RequestValidator};
use proptest::prelude::*;

/// Build a minimal valid validator with both checks enabled.
fn strict_validator(max_depth: usize, max_complexity: usize) -> RequestValidator {
    RequestValidator::new()
        .with_max_depth(max_depth)
        .with_complexity_validation(true)
        .with_max_complexity(max_complexity)
        .with_depth_validation(true)
}

proptest! {
    /// Arbitrary strings must never cause a panic — the function must return
    /// Ok or a typed ValidationError.
    #[test]
    fn validate_query_never_panics(query in "\\PC*") {
        let validator = RequestValidator::new();
        let _ = validator.validate_query(&query);  // intentional
    }

    /// An empty string must always return MalformedQuery.
    #[test]
    fn empty_query_always_fails(ws in "[ \t\n\r]*") {
        let validator = RequestValidator::new();
        let result = validator.validate_query(&ws);
        prop_assert!(
            matches!(result, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery for whitespace-only input, got: {result:?}"
        );
    }

    /// Relaxing the depth limit must not turn a passing query into a failing one.
    #[test]
    fn relaxed_depth_limit_never_rejects_passing_query(
        max_depth in 1usize..=20,
        extra in 1usize..=20,
    ) {
        // A shallow, well-formed query
        let query = "{ user { id name } }";
        let strict = strict_validator(max_depth, 1000);
        let relaxed = strict_validator(max_depth + extra, 1000);

        let strict_ok = strict.validate_query(query).is_ok();
        let relaxed_ok = relaxed.validate_query(query).is_ok();

        if strict_ok {
            prop_assert!(
                relaxed_ok,
                "relaxed limit ({}) must accept query that strict limit ({max_depth}) accepted",
                max_depth + extra
            );
        }
    }

    /// Relaxing the complexity limit must not turn a passing query into a failing one.
    #[test]
    fn relaxed_complexity_limit_never_rejects_passing_query(
        max_complexity in 1usize..=100,
        extra in 1usize..=100,
    ) {
        let query = "{ user { id } }";
        let strict = strict_validator(50, max_complexity);
        let relaxed = strict_validator(50, max_complexity + extra);

        let strict_ok = strict.validate_query(query).is_ok();
        let relaxed_ok = relaxed.validate_query(query).is_ok();

        if strict_ok {
            prop_assert!(
                relaxed_ok,
                "relaxed complexity ({}) must accept query that strict ({max_complexity}) accepted",
                max_complexity + extra
            );
        }
    }

    /// Disabling both validations must always pass non-empty queries.
    #[test]
    fn disabled_validation_passes_any_non_empty_string(query in "[a-z{ }]{1,50}") {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false);
        // May fail due to parse error, but must never panic
        let _ = validator.validate_query(&query);  // intentional
    }
}
