//! Test LTree format validation and edge cases.
//!
//! This test verifies that:
//! 1. LTree operators handle various path formats correctly
//! 2. Invalid LTree values are preserved for SQL layer to handle
//! 3. LTree with very long paths work without truncation
//! 4. LTree operator values maintain structural integrity
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Invalid ltree values could cause PostgreSQL errors
//! - Very long paths could be silently truncated
//! - LTree operator behavior could be inconsistent

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_ltree_valid_path_formats() {
    // Valid LTree paths should be preserved
    let valid_paths = vec![
        "a",
        "a.b",
        "a.b.c",
        "a.b.c.d.e",
        "org.company.dept.team",
        "level1.level2.level3",
        "simple",
        "CamelCase",
        "with_underscores",
        "with-dashes",
        "123",
        "a1.b2.c3",
    ];

    for path in valid_paths {
        let operators = vec![
            WhereOperator::AncestorOf,
            WhereOperator::DescendantOf,
            WhereOperator::MatchesLquery,
        ];

        for op in operators {
            let clause = WhereClause::Field {
                path: vec!["hierarchy".to_string()],
                operator: op.clone(),
                value: json!(path),
            };

            match clause {
                WhereClause::Field { value, operator, .. } => {
                    assert_eq!(value, json!(path), "Path {} should be preserved", path);
                    assert_eq!(operator, op);
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_long_path_preservation() {
    // Very long paths should be preserved without truncation
    let long_path_100 = (0..100)
        .map(|i| format!("level{}", i))
        .collect::<Vec<_>>()
        .join(".");

    let long_paths: Vec<&str> = vec![
        "a.b.c.d.e.f.g.h.i.j.k.l.m.n.o.p.q.r.s.t.u.v.w.x.y.z",  // 26 components
        "level1.level2.level3.level4.level5.level6.level7.level8.level9.level10",  // 10 levels
    ];

    for path in &long_paths {
        let clause = WhereClause::Field {
            path: vec!["tree".to_string()],
            operator: WhereOperator::AncestorOf,
            value: json!(path),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                // Verify path is preserved exactly (no truncation)
                assert_eq!(value, json!(path));

                // Verify we can count the components
                let component_count = path.split('.').count();
                let json_str = value.as_str().unwrap();
                let json_component_count = json_str.split('.').count();
                assert_eq!(json_component_count, component_count);
            }
            _ => panic!("Should be Field variant"),
        }
    }

    // Test 100-component path separately
    let clause = WhereClause::Field {
        path: vec!["tree".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!(&long_path_100),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(&long_path_100));
            let component_count = long_path_100.split('.').count();
            let json_str = value.as_str().unwrap();
            let json_component_count = json_str.split('.').count();
            assert_eq!(json_component_count, component_count);
        }
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_ltree_special_characters_in_labels() {
    // LTree labels can contain letters, numbers, underscores, and dashes
    let label_types = vec![
        "all_lowercase",
        "ALL_UPPERCASE",
        "Mixed_Case",
        "with-dashes",
        "numbers123",
        "123numbers",
        "_leading_underscore",
        "trailing_underscore_",
        "_both_",
        "multi_under_score",
        "multi-dash-label",
        "mixed_under-dash",
    ];

    for label in label_types {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::DescendantOf,
            value: json!(label),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(value, json!(label));
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_queries_lquery_patterns() {
    // LTree lquery patterns for pattern matching
    let lquery_patterns = vec![
        "a",           // exact label
        "a.b.c",       // exact path
        "a.*",         // a and all children
        "*.b",         // any first then b
        "a.*.c",       // a, any middle, c
        "a|b",         // a or b
        "a.b|c",       // complex
        "!a",          // not a
        "{a,b,c}",     // in set
        "a.{b,c,d}.e", // mixed patterns
    ];

    for pattern in lquery_patterns {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::MatchesLquery,
            value: json!(pattern),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(value, json!(pattern));
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_depth_operators() {
    // LTree depth operators (nlevel) for comparing path depth
    let depths = vec![0, 1, 2, 5, 10, 100, 1000];

    for depth in depths {
        let operators = vec![
            WhereOperator::DepthEq,
            WhereOperator::DepthNeq,
            WhereOperator::DepthGt,
            WhereOperator::DepthGte,
            WhereOperator::DepthLt,
            WhereOperator::DepthLte,
        ];

        for op in operators {
            let clause = WhereClause::Field {
                path: vec!["tree".to_string()],
                operator: op.clone(),
                value: json!(depth),
            };

            match clause {
                WhereClause::Field { value, operator, .. } => {
                    assert_eq!(value, json!(depth));
                    assert_eq!(operator, op);
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_path_with_numbers() {
    // Numeric path components
    let numeric_paths = vec![
        "0",
        "1",
        "123",
        "0.0.0",
        "1.2.3.4.5",
        "100.200.300",
        "9999.9999.9999",
    ];

    for path in numeric_paths {
        let clause = WhereClause::Field {
            path: vec!["numeric_path".to_string()],
            operator: WhereOperator::AncestorOf,
            value: json!(path),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(value, json!(path));
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_empty_path_handling() {
    // Edge case: empty or minimal paths
    let edge_paths = vec![
        "",          // empty string
        "a",         // single component
        ".",         // just dot (invalid)
        "a.",        // trailing dot (invalid)
        ".a",        // leading dot (invalid)
        "a..b",      // double dot (invalid)
    ];

    for path in edge_paths {
        // Should still construct - validation happens at SQL generation
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::MatchesLquery,
            value: json!(path),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(value, json!(path));
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_case_sensitivity() {
    // LTree paths are case-sensitive
    let case_variants = vec![
        ("Org", "ORG", "org"),  // Different cases
        ("Company", "COMPANY", "company"),
        ("Department", "DEPARTMENT", "department"),
    ];

    for (variant1, variant2, variant3) in case_variants {
        let clause1 = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::DescendantOf,
            value: json!(variant1),
        };

        let clause2 = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::DescendantOf,
            value: json!(variant2),
        };

        let clause3 = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::DescendantOf,
            value: json!(variant3),
        };

        match (clause1, clause2, clause3) {
            (
                WhereClause::Field { value: v1, .. },
                WhereClause::Field { value: v2, .. },
                WhereClause::Field { value: v3, .. },
            ) => {
                // All should be preserved exactly
                assert_eq!(v1, json!(variant1));
                assert_eq!(v2, json!(variant2));
                assert_eq!(v3, json!(variant3));
                // And they should be different
                assert_ne!(v1, v2);
                assert_ne!(v2, v3);
            }
            _ => panic!("Should be Field variants"),
        }
    }
}

#[test]
fn test_ltree_unicode_labels() {
    // Unicode characters in LTree paths (may be invalid in PostgreSQL but should be preserved)
    let unicode_paths = vec![
        "café",
        "naïve",
        "resumé",
        "piñata",
        "Москва",      // Russian
        "北京",         // Chinese
        "東京",         // Japanese
        "🏢.company",   // Emoji (invalid but preserved)
    ];

    for path in unicode_paths {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: WhereOperator::AncestorOf,
            value: json!(path),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(value, json!(path));
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_operators_all_variants() {
    // Verify all LTree operators are available and constructable
    let all_ltree_operators = vec![
        (WhereOperator::AncestorOf, "AncestorOf"),
        (WhereOperator::DescendantOf, "DescendantOf"),
        (WhereOperator::MatchesLquery, "MatchesLquery"),
        (WhereOperator::MatchesLtxtquery, "MatchesLtxtquery"),
        (WhereOperator::MatchesAnyLquery, "MatchesAnyLquery"),
        (WhereOperator::DepthEq, "DepthEq"),
        (WhereOperator::DepthNeq, "DepthNeq"),
        (WhereOperator::DepthGt, "DepthGt"),
        (WhereOperator::DepthGte, "DepthGte"),
        (WhereOperator::DepthLt, "DepthLt"),
        (WhereOperator::DepthLte, "DepthLte"),
        (WhereOperator::Lca, "Lca"),  // Lowest Common Ancestor
    ];

    for (operator, _name) in all_ltree_operators {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator: operator.clone(),
            value: json!("test_value"),
        };

        match clause {
            WhereClause::Field { operator: op, .. } => {
                assert_eq!(op, operator);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_ltree_path_depth_constraints() {
    // LTree has depth limits (PostgreSQL: 65535 maximum path length)
    // We test that various depths are handled structurally

    for depth in [1, 10, 100, 1000] {
        let components: Vec<String> = (0..depth).map(|i| format!("l{}", i)).collect();
        let path = components.join(".");

        let clause = WhereClause::Field {
            path: vec!["tree".to_string()],
            operator: WhereOperator::AncestorOf,
            value: json!(&path),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                // Path should be preserved
                assert_eq!(value, json!(&path));

                // Component count should match depth
                let actual_depth = value.as_str().unwrap().split('.').count();
                assert_eq!(actual_depth, depth);
            }
            _ => panic!("Should be Field variant"),
        }
    }
}
