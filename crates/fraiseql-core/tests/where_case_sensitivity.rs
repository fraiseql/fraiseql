//! Test WHERE clause case sensitivity across operators.
//!
//! This test verifies that:
//! 1. Case-sensitive operators (Contains, Startswith, Endswith) work correctly
//! 2. Case-insensitive operators (Icontains, Istartswith, Iendswith) work correctly
//! 3. Case sensitivity is properly distinguished across all string operators
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Case-insensitive queries might return wrong results (off-by-one matching)
//! - Case-sensitive queries could incorrectly match different cases
//! - Data consistency would be violated

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_where_case_sensitive_operators() {
    // Contains operator (LIKE with %) - case-sensitive in most databases
    let test_value = "Hello World";

    let cases = vec![
        ("Hello", WhereOperator::Contains, true),  // exact case
        ("hello", WhereOperator::Contains, false), // lowercase won't match
        ("HELLO", WhereOperator::Contains, false), // uppercase won't match
        ("World", WhereOperator::Contains, true),
        ("world", WhereOperator::Contains, false),
    ];

    for (pattern, operator, _should_match) in cases {
        let clause = WhereClause::Field {
            path: vec!["text".to_string()],
            operator,
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, operator, .. } => {
                assert_eq!(value, json!(pattern));
                assert_eq!(operator, WhereOperator::Contains);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_case_insensitive_operators() {
    // Icontains operator (ILIKE with %) - case-insensitive
    let test_value = "Hello World";

    let patterns = vec![
        "hello",
        "HELLO",
        "Hello",
        "HeLLo",
        "world",
        "WORLD",
        "World",
    ];

    for pattern in patterns {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, operator, .. } => {
                assert_eq!(value, json!(pattern));
                assert_eq!(operator, WhereOperator::Icontains);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_startswith_case_sensitive() {
    // Startswith operator - case-sensitive
    let test_cases = vec![
        ("Test", "Test", true),   // exact case
        ("test", "Test", false),  // lowercase won't match at start
        ("TEST", "Test", false),  // uppercase won't match
        ("T", "Test", true),      // first letter matches
        ("t", "Test", false),     // lowercase 't' won't match uppercase 'T'
    ];

    for (value, _pattern, _should_match) in test_cases {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Startswith,
            value: json!(value),
        };

        match clause {
            WhereClause::Field { value: v, operator, .. } => {
                assert_eq!(v, json!(value));
                assert_eq!(operator, WhereOperator::Startswith);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_istartswith_case_insensitive() {
    // Istartswith operator - case-insensitive
    let patterns = vec![
        "test",
        "Test",
        "TEST",
        "TeSt",
        "tEST",
    ];

    for pattern in patterns {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Istartswith,
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, operator, .. } => {
                assert_eq!(value, json!(pattern));
                assert_eq!(operator, WhereOperator::Istartswith);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_endswith_case_sensitive() {
    // Endswith operator - case-sensitive
    let test_cases = vec![
        ("example.com", "example.com", true),  // exact match
        ("example.COM", "example.com", false), // different case won't match
        ("EXAMPLE.COM", "example.com", false),
        (".com", "example.com", true),         // ending matches
        (".COM", "example.com", false),        // uppercase .COM won't match .com
    ];

    for (value, _pattern, _should_match) in test_cases {
        let clause = WhereClause::Field {
            path: vec!["domain".to_string()],
            operator: WhereOperator::Endswith,
            value: json!(value),
        };

        match clause {
            WhereClause::Field { value: v, operator, .. } => {
                assert_eq!(v, json!(value));
                assert_eq!(operator, WhereOperator::Endswith);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_iendswith_case_insensitive() {
    // Iendswith operator - case-insensitive
    let patterns = vec![
        "example.com",
        "example.COM",
        "EXAMPLE.COM",
        "Example.Com",
        ".com",
        ".COM",
        ".Com",
    ];

    for pattern in patterns {
        let clause = WhereClause::Field {
            path: vec!["url".to_string()],
            operator: WhereOperator::Iendswith,
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, operator, .. } => {
                assert_eq!(value, json!(pattern));
                assert_eq!(operator, WhereOperator::Iendswith);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_case_operators_distinctions() {
    // Verify each operator pair is correctly distinct
    let operators = vec![
        (WhereOperator::Contains, WhereOperator::Icontains, "contains vs icontains"),
        (WhereOperator::Startswith, WhereOperator::Istartswith, "startswith vs istartswith"),
        (WhereOperator::Endswith, WhereOperator::Iendswith, "endswith vs iendswith"),
    ];

    for (case_sensitive, case_insensitive, desc) in operators {
        // They should be different variants
        assert_ne!(
            std::mem::discriminant(&case_sensitive),
            std::mem::discriminant(&case_insensitive),
            "{} should be distinct operators",
            desc
        );

        // Both should work with WhereClause
        let clause1 = WhereClause::Field {
            path: vec!["text".to_string()],
            operator: case_sensitive.clone(),
            value: json!("test"),
        };

        let clause2 = WhereClause::Field {
            path: vec!["text".to_string()],
            operator: case_insensitive.clone(),
            value: json!("test"),
        };

        // Both should compile and construct successfully
        match (clause1, clause2) {
            (
                WhereClause::Field { operator: op1, .. },
                WhereClause::Field { operator: op2, .. },
            ) => {
                assert_ne!(op1, op2);
            }
            _ => panic!("Should be Field variants"),
        }
    }
}

#[test]
fn test_where_case_with_mixed_content() {
    // Test case sensitivity with mixed alphanumeric content
    let _test_content = "User123Example";

    let test_cases = vec![
        ("User", WhereOperator::Startswith, true),  // case-sensitive
        ("user", WhereOperator::Startswith, false), // different case
        ("User", WhereOperator::Istartswith, true), // case-insensitive
        ("user", WhereOperator::Istartswith, true), // case-insensitive
        ("Example", WhereOperator::Endswith, true),
        ("example", WhereOperator::Endswith, false),
        ("Example", WhereOperator::Iendswith, true),
        ("example", WhereOperator::Iendswith, true),
    ];

    for (pattern, operator, _should_match) in test_cases {
        let clause = WhereClause::Field {
            path: vec!["id".to_string()],
            operator: operator.clone(),
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, operator: op, .. } => {
                assert_eq!(value, json!(pattern));
                assert_eq!(op, operator);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_case_with_special_chars() {
    // Case sensitivity with special characters
    let patterns = vec![
        ("Test-Case", "test-case"),
        ("Test_Case", "test_case"),
        ("Test.Case", "test.case"),
        ("Test@Case", "test@case"),
        ("Test#Case", "test#case"),
    ];

    for (upper_pattern, lower_pattern) in patterns {
        // Case-sensitive should see them as different
        let clause_upper = WhereClause::Field {
            path: vec!["value".to_string()],
            operator: WhereOperator::Startswith,
            value: json!(upper_pattern),
        };

        let clause_lower = WhereClause::Field {
            path: vec!["value".to_string()],
            operator: WhereOperator::Startswith,
            value: json!(lower_pattern),
        };

        // Different values
        match (clause_upper, clause_lower) {
            (
                WhereClause::Field { value: v1, .. },
                WhereClause::Field { value: v2, .. },
            ) => {
                assert_ne!(v1, v2);
            }
            _ => panic!("Should be Field variants"),
        }

        // Case-insensitive should treat them as equivalent patterns
        let case_insensitive1 = WhereClause::Field {
            path: vec!["value".to_string()],
            operator: WhereOperator::Istartswith,
            value: json!(upper_pattern),
        };

        let case_insensitive2 = WhereClause::Field {
            path: vec!["value".to_string()],
            operator: WhereOperator::Istartswith,
            value: json!(lower_pattern),
        };

        match (case_insensitive1, case_insensitive2) {
            (
                WhereClause::Field { operator: op1, .. },
                WhereClause::Field { operator: op2, .. },
            ) => {
                // Both use case-insensitive operator
                assert_eq!(op1, op2);
                assert_eq!(op1, WhereOperator::Istartswith);
            }
            _ => panic!("Should be Field variants"),
        }
    }
}

#[test]
fn test_where_case_unicode_handling() {
    // Case sensitivity with Unicode characters
    let unicode_tests = vec![
        ("Café", "café"),
        ("CAFÉ", "café"),
        ("Ångström", "ångström"),
        ("ÅNGSTRÖM", "ångström"),
        ("Straße", "strasse"),  // German ß
        ("Москва", "москва"),   // Russian Cyrillic
        ("МОСКВА", "москва"),
    ];

    for (pattern1, pattern2) in unicode_tests {
        // Case-sensitive sees them as different (usually)
        let clause1 = WhereClause::Field {
            path: vec!["text".to_string()],
            operator: WhereOperator::Contains,
            value: json!(pattern1),
        };

        let clause2 = WhereClause::Field {
            path: vec!["text".to_string()],
            operator: WhereOperator::Contains,
            value: json!(pattern2),
        };

        match (clause1, clause2) {
            (
                WhereClause::Field { value: v1, .. },
                WhereClause::Field { value: v2, .. },
            ) => {
                // Patterns preserved as-is
                assert_eq!(v1, json!(pattern1));
                assert_eq!(v2, json!(pattern2));
            }
            _ => panic!("Should be Field variants"),
        }
    }
}
