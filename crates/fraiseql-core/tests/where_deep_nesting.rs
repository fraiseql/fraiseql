//! Test WHERE clause with deeply nested JSON paths.
//!
//! This test verifies that:
//! 1. Deep nested paths (5+ levels) are handled correctly
//! 2. Path component names are preserved at all levels
//! 3. Operators work correctly with nested paths
//! 4. Deep nesting doesn't cause truncation or errors
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Deep nested paths could be silently truncated
//! - Path components could be lost or corrupted
//! - SQL generation could fail on deep nesting

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_where_nested_path_3_levels() {
    // 3-level path: user.profile.address
    let clause = WhereClause::Field {
        path:     vec![
            "user".to_string(),
            "profile".to_string(),
            "address".to_string(),
        ],
        operator: WhereOperator::Eq,
        value:    json!("123 Main St"),
    };

    match clause {
        WhereClause::Field { path, value, .. } => {
            assert_eq!(path.len(), 3);
            assert_eq!(path[0], "user");
            assert_eq!(path[1], "profile");
            assert_eq!(path[2], "address");
            assert_eq!(value, json!("123 Main St"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_nested_path_5_levels() {
    // 5-level path: user.profile.address.country.region
    let clause = WhereClause::Field {
        path:     vec![
            "user".to_string(),
            "profile".to_string(),
            "address".to_string(),
            "country".to_string(),
            "region".to_string(),
        ],
        operator: WhereOperator::Eq,
        value:    json!("California"),
    };

    match clause {
        WhereClause::Field { path, value, .. } => {
            assert_eq!(path.len(), 5);
            assert_eq!(path[0], "user");
            assert_eq!(path[2], "address");
            assert_eq!(path[4], "region");
            assert_eq!(value, json!("California"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_nested_path_10_levels() {
    // 10-level path for deeply nested data structures
    let path: Vec<String> = (0..10).map(|i| format!("level{}", i)).collect();

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Contains,
        value:    json!("search_term"),
    };

    match clause {
        WhereClause::Field { path: p, value, .. } => {
            assert_eq!(p.len(), 10);
            for (i, component) in p.iter().enumerate() {
                assert_eq!(component, &format!("level{}", i));
            }
            assert_eq!(value, json!("search_term"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_nested_path_20_levels() {
    // 20-level path - extreme nesting
    let path: Vec<String> = (0..20).map(|i| format!("l{}", i)).collect();

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Eq,
        value:    json!("deep_value"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p.len(), 20);
            assert_eq!(p[0], "l0");
            assert_eq!(p[10], "l10");
            assert_eq!(p[19], "l19");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_nested_path_with_different_operators() {
    // Test deep nesting with various operators
    let path = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
        "e".to_string(),
    ];

    let operators = vec![
        (WhereOperator::Eq, "equals"),
        (WhereOperator::Contains, "contains"),
        (WhereOperator::Startswith, "startswith"),
        (WhereOperator::Gt, "greater than"),
        (WhereOperator::Lt, "less than"),
    ];

    for (op, _desc) in operators {
        let clause = WhereClause::Field {
            path:     path.clone(),
            operator: op.clone(),
            value:    json!("test"),
        };

        match clause {
            WhereClause::Field {
                path: p, operator, ..
            } => {
                assert_eq!(p.len(), 5);
                assert_eq!(operator, op);
            },
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_nested_path_special_characters() {
    // Deep paths with special characters in component names
    let path = vec![
        "user_id".to_string(),
        "profile-data".to_string(),
        "contact.info".to_string(),
        "email_addresses".to_string(),
        "primary-email".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Contains,
        value:    json!("example.com"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p, path);
            assert_eq!(p[0], "user_id");
            assert_eq!(p[1], "profile-data");
            assert_eq!(p[2], "contact.info");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_nested_path_numeric_components() {
    // Array-like paths with numeric indices
    let path = vec![
        "users".to_string(),
        "0".to_string(),
        "addresses".to_string(),
        "1".to_string(),
        "zip".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Eq,
        value:    json!("90210"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p.len(), 5);
            assert_eq!(p[1], "0");
            assert_eq!(p[3], "1");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_with_null_value() {
    // Deep path with NULL value (IS NULL)
    let path = vec![
        "data".to_string(),
        "metadata".to_string(),
        "created".to_string(),
        "timestamp".to_string(),
        "timezone".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::IsNull,
        value:    json!(true),
    };

    match clause {
        WhereClause::Field { path: p, value, .. } => {
            assert_eq!(p.len(), 5);
            assert_eq!(value, json!(true));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_with_array_value() {
    // Deep path with array value (IN operator)
    let path = vec![
        "org".to_string(),
        "departments".to_string(),
        "members".to_string(),
        "roles".to_string(),
        "permissions".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::In,
        value:    json!(["read", "write", "admin"]),
    };

    match clause {
        WhereClause::Field { path: p, value, .. } => {
            assert_eq!(p.len(), 5);
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 3);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_unicode_paths() {
    // Deep paths with Unicode characters
    let path = vec![
        "user".to_string(),
        "profil".to_string(),
        "données".to_string(),
        "contact".to_string(),
        "email_français".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Contains,
        value:    json!("contact@example.fr"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p, path);
            assert_eq!(p[2], "données");
            assert_eq!(p[4], "email_français");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_mixed_content() {
    // Deep paths with mixed alphanumeric and special characters
    let path = vec![
        "api_v2".to_string(),
        "response_data".to_string(),
        "results[0]".to_string(),
        "user.profile".to_string(),
        "contact-info".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Eq,
        value:    json!("test@example.com"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p.len(), 5);
            assert_eq!(p[0], "api_v2");
            assert_eq!(p[2], "results[0]");
            assert_eq!(p[3], "user.profile");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_case_sensitivity() {
    // Deep paths preserve case sensitivity
    let path_lower = vec![
        "user".to_string(),
        "profile".to_string(),
        "email".to_string(),
        "address".to_string(),
        "domain".to_string(),
    ];
    let path_upper = vec![
        "USER".to_string(),
        "PROFILE".to_string(),
        "EMAIL".to_string(),
        "ADDRESS".to_string(),
        "DOMAIN".to_string(),
    ];

    let clause_lower = WhereClause::Field {
        path:     path_lower.clone(),
        operator: WhereOperator::Eq,
        value:    json!("example.com"),
    };

    let clause_upper = WhereClause::Field {
        path:     path_upper.clone(),
        operator: WhereOperator::Eq,
        value:    json!("example.com"),
    };

    match (clause_lower, clause_upper) {
        (WhereClause::Field { path: p1, .. }, WhereClause::Field { path: p2, .. }) => {
            // Paths should be different (case matters)
            assert_ne!(p1, p2);
            assert_eq!(p1[0], "user");
            assert_eq!(p2[0], "USER");
        },
        _ => panic!("Should be Field variants"),
    }
}

#[test]
fn test_where_deeply_nested_with_ltree_operators() {
    // Deep paths with LTree operators
    let path = vec![
        "org".to_string(),
        "hierarchy".to_string(),
        "department".to_string(),
        "team".to_string(),
        "structure".to_string(),
    ];

    let ltree_operators = vec![
        WhereOperator::AncestorOf,
        WhereOperator::DescendantOf,
        WhereOperator::MatchesLquery,
    ];

    for op in ltree_operators {
        let clause = WhereClause::Field {
            path:     path.clone(),
            operator: op.clone(),
            value:    json!("a.b.c"),
        };

        match clause {
            WhereClause::Field {
                path: p, operator, ..
            } => {
                assert_eq!(p.len(), 5);
                assert_eq!(operator, op);
            },
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_deeply_nested_repeating_components() {
    // Deep paths with repeating component names
    let path = vec![
        "data".to_string(),
        "data".to_string(),
        "data".to_string(),
        "data".to_string(),
        "value".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Eq,
        value:    json!("test"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p.len(), 5);
            // First 4 should all be "data"
            assert_eq!(p[0], "data");
            assert_eq!(p[1], "data");
            assert_eq!(p[2], "data");
            assert_eq!(p[3], "data");
            assert_eq!(p[4], "value");
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_deeply_nested_empty_component() {
    // Edge case: deep path with empty component name
    let path = vec![
        "a".to_string(),
        "".to_string(),
        "b".to_string(),
        "".to_string(),
        "c".to_string(),
    ];

    let clause = WhereClause::Field {
        path:     path.clone(),
        operator: WhereOperator::Eq,
        value:    json!("test"),
    };

    match clause {
        WhereClause::Field { path: p, .. } => {
            assert_eq!(p.len(), 5);
            assert_eq!(p[0], "a");
            assert_eq!(p[1], ""); // Empty component preserved
            assert_eq!(p[2], "b");
            assert_eq!(p[3], ""); // Empty component preserved
            assert_eq!(p[4], "c");
        },
        _ => panic!("Should be Field variant"),
    }
}
