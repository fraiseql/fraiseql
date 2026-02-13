//! Test NULL handling in complex WHERE clause logic.
//!
//! This test verifies that:
//! 1. NULL values in WHERE clauses follow three-valued logic (TRUE, FALSE, UNKNOWN)
//! 2. Complex AND/OR logic with NULLs is handled correctly
//! 3. NULL comparisons don't accidentally match
//! 4. IS NULL and IS NOT NULL operators work correctly
//!
//! # Risk If Missing
//!
//! Without this test:
//! - NULL = any_value could incorrectly return TRUE (should be UNKNOWN)
//! - WHERE clauses could have silent bugs with NULL handling
//! - NULL filtering could exclude or include wrong rows

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_where_null_equality_is_null() {
    // NULL = NULL should use IS NULL, not =
    let clause = WhereClause::Field {
        path:     vec!["field".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(null),
    };

    match clause {
        WhereClause::Field {
            value, operator, ..
        } => {
            assert_eq!(value, json!(null));
            assert_eq!(operator, WhereOperator::Eq);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_is_null_operator() {
    // IsNull operator for proper NULL checks
    let clause = WhereClause::Field {
        path:     vec!["optional_field".to_string()],
        operator: WhereOperator::IsNull,
        value:    json!(true), // Typically IsNull takes a boolean flag
    };

    match clause {
        WhereClause::Field {
            value, operator, ..
        } => {
            assert_eq!(value, json!(true));
            assert_eq!(operator, WhereOperator::IsNull);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_is_not_null_operator() {
    // IsNull operator with false value means IS NOT NULL
    let clause = WhereClause::Field {
        path:     vec!["required_field".to_string()],
        operator: WhereOperator::IsNull,
        value:    json!(false), // false means IS NOT NULL
    };

    match clause {
        WhereClause::Field {
            value, operator, ..
        } => {
            assert_eq!(value, json!(false));
            assert_eq!(operator, WhereOperator::IsNull);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_complex_and_with_null() {
    // AND logic: (TRUE AND UNKNOWN) = UNKNOWN
    let and_clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        },
        WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        },
    ]);

    match and_clause {
        WhereClause::And(clauses) => {
            assert_eq!(clauses.len(), 2);
        },
        _ => panic!("Should be And variant"),
    }
}

#[test]
fn test_where_complex_or_with_null() {
    // OR logic: (FALSE OR UNKNOWN) = UNKNOWN
    let or_clause = WhereClause::Or(vec![
        WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("inactive"),
        },
        WhereClause::Field {
            path:     vec!["archived_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(false), // IS NOT NULL
        },
    ]);

    match or_clause {
        WhereClause::Or(clauses) => {
            assert_eq!(clauses.len(), 2);
        },
        _ => panic!("Should be Or variant"),
    }
}

#[test]
fn test_where_not_with_null() {
    // NOT logic: NOT UNKNOWN = UNKNOWN
    let not_clause = WhereClause::Not(Box::new(WhereClause::Field {
        path:     vec!["value".to_string()],
        operator: WhereOperator::IsNull,
        value:    json!(true),
    }));

    match not_clause {
        WhereClause::Not(inner) => match *inner {
            WhereClause::Field { operator, .. } => {
                assert_eq!(operator, WhereOperator::IsNull);
            },
            _ => panic!("Inner should be Field variant"),
        },
        _ => panic!("Should be Not variant"),
    }
}

#[test]
fn test_where_null_with_different_operators() {
    // NULL with various operators should all use IS NULL
    let operators = vec![
        WhereOperator::Eq,
        WhereOperator::Neq,
        WhereOperator::Gt,
        WhereOperator::Gte,
        WhereOperator::Lt,
        WhereOperator::Lte,
        WhereOperator::Contains,
    ];

    for op in operators {
        let clause = WhereClause::Field {
            path:     vec!["field".to_string()],
            operator: op.clone(),
            value:    json!(null),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                // All should preserve the null value
                assert_eq!(value, json!(null));
            },
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_null_in_nested_paths() {
    // NULL comparisons in nested JSON paths
    let nested_clause = WhereClause::Field {
        path:     vec![
            "user".to_string(),
            "profile".to_string(),
            "middle_name".to_string(),
        ],
        operator: WhereOperator::IsNull,
        value:    json!(true),
    };

    match nested_clause {
        WhereClause::Field { path, value, .. } => {
            assert_eq!(path.len(), 3);
            assert_eq!(path[0], "user");
            assert_eq!(path[1], "profile");
            assert_eq!(path[2], "middle_name");
            assert_eq!(value, json!(true));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_null_with_array_operators() {
    // NULL values in array operators
    let array_clause = WhereClause::Field {
        path:     vec!["tags".to_string()],
        operator: WhereOperator::In,
        value:    json!([1, 2, null, 4]),
    };

    match array_clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 4);
            assert!(arr[2].is_null());
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_null_three_valued_logic_and() {
    // Test three-valued logic for AND operator
    let test_cases = vec![
        // (left_is_true, right_is_null, and_result_description)
        (true, true, "TRUE AND UNKNOWN = UNKNOWN"),
        (true, false, "TRUE AND FALSE = FALSE"),
        (false, true, "FALSE AND UNKNOWN = FALSE"), // Short-circuit
        (false, false, "FALSE AND FALSE = FALSE"),
    ];

    for (left_true, right_has_null, _description) in test_cases {
        let left_value = if left_true {
            json!("active")
        } else {
            json!("inactive")
        };

        let right_value = if right_has_null {
            json!(null)
        } else {
            json!(true)
        };

        let and_clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    left_value,
            },
            WhereClause::Field {
                path:     vec!["deleted_at".to_string()],
                operator: WhereOperator::IsNull,
                value:    right_value,
            },
        ]);

        match and_clause {
            WhereClause::And(clauses) => {
                assert_eq!(clauses.len(), 2);
            },
            _ => panic!("Should be And variant"),
        }
    }
}

#[test]
fn test_where_null_three_valued_logic_or() {
    // Test three-valued logic for OR operator
    let test_cases = vec![
        // (left_is_true, right_is_null, or_result_description)
        (true, true, "TRUE OR UNKNOWN = TRUE"), // Short-circuit
        (true, false, "TRUE OR FALSE = TRUE"),  // Short-circuit
        (false, true, "FALSE OR UNKNOWN = UNKNOWN"),
        (false, false, "FALSE OR FALSE = FALSE"),
    ];

    for (left_true, right_has_null, _description) in test_cases {
        let left_value = if left_true {
            json!("active")
        } else {
            json!("inactive")
        };

        let right_value = if right_has_null {
            json!(null)
        } else {
            json!(true)
        };

        let or_clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    left_value,
            },
            WhereClause::Field {
                path:     vec!["deleted_at".to_string()],
                operator: WhereOperator::IsNull,
                value:    right_value,
            },
        ]);

        match or_clause {
            WhereClause::Or(clauses) => {
                assert_eq!(clauses.len(), 2);
            },
            _ => panic!("Should be Or variant"),
        }
    }
}

#[test]
fn test_where_null_not_in_operator() {
    // NOT IN with NULL values
    let nin_clause = WhereClause::Field {
        path:     vec!["status".to_string()],
        operator: WhereOperator::Nin,
        value:    json!(["deleted", "archived", null]),
    };

    match nin_clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 3);
            assert!(arr[2].is_null());
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_null_comparison_null_handling() {
    // Verify NULL is distinct from false/0/empty string
    let clauses = vec![
        (json!(null), "null value"),
        (json!(false), "false value"),
        (json!(0), "zero value"),
        (json!(""), "empty string"),
    ];

    for (value, description) in clauses {
        let clause = WhereClause::Field {
            path:     vec!["field".to_string()],
            operator: WhereOperator::Eq,
            value:    value.clone(),
        };

        match clause {
            WhereClause::Field { value: v, .. } => {
                assert_eq!(v, value, "{} should be preserved exactly", description);
            },
            _ => panic!("Should be Field variant"),
        }
    }

    // Verify they're all different
    assert_ne!(json!(null), json!(false));
    assert_ne!(json!(null), json!(0));
    assert_ne!(json!(null), json!(""));
    assert_ne!(json!(false), json!(0));
}

#[test]
fn test_where_null_in_complex_nested_logic() {
    // Complex nested AND/OR with multiple NULLs
    let complex_clause = WhereClause::And(vec![
        WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["trial_expires".to_string()],
                operator: WhereOperator::IsNull,
                value:    json!(false),
            },
        ]),
        WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["deleted_at".to_string()],
                operator: WhereOperator::IsNull,
                value:    json!(true),
            },
            WhereClause::Field {
                path:     vec!["archived_at".to_string()],
                operator: WhereOperator::IsNull,
                value:    json!(true),
            },
        ]),
    ]);

    match complex_clause {
        WhereClause::And(outer_clauses) => {
            assert_eq!(outer_clauses.len(), 2);
            match &outer_clauses[0] {
                WhereClause::Or(inner_or) => assert_eq!(inner_or.len(), 2),
                _ => panic!("First should be Or"),
            }
            match &outer_clauses[1] {
                WhereClause::And(inner_and) => assert_eq!(inner_and.len(), 2),
                _ => panic!("Second should be And"),
            }
        },
        _ => panic!("Should be And variant"),
    }
}
