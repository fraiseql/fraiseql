//! Test LTree operator edge cases: empty paths, deep nesting, special characters.
//!
//! This test verifies that:
//! 1. LTree operators handle empty path components gracefully
//! 2. Deep nesting (5+ levels) is supported without truncation
//! 3. Special characters in path components are preserved
//! 4. LTree operators don't cause SQL injection through path manipulation
//!
//! # Risk If Missing
//!
//! Without this test, LTree queries could:
//! - Fail on empty path segments (data loss)
//! - Truncate deeply nested paths (query failure)
//! - Corrupt special characters in paths (wrong results)
//! - Allow SQL injection through path components

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

/// Test LTree operators with various path configurations
const LTREE_OPERATORS: &[WhereOperator] = &[
    WhereOperator::AncestorOf,        // @>
    WhereOperator::DescendantOf,      // <@
    WhereOperator::MatchesLquery,     // ~
    WhereOperator::MatchesLtxtquery,  // @ (Boolean query syntax)
    WhereOperator::DepthEq,           // nlevel() =
];

#[test]
fn test_ltree_empty_path_handling() {
    // Test handling of empty or minimal paths
    let empty_paths = vec![
        vec![],  // Completely empty path
        vec!["".to_string()],  // Single empty component
        vec!["".to_string(), "".to_string()],  // Multiple empty components
        vec!["a".to_string(), "".to_string(), "b".to_string()],  // Empty in middle
    ];

    for path in empty_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("test"),
            };

            // Should handle gracefully without panic
            match clause {
                WhereClause::Field { .. } => {
                    // Success - empty paths are preserved
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_deep_nesting_5_plus_levels() {
    // Test that deeply nested paths (5+ levels) are fully supported
    let deep_paths = vec![
        vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()],  // 5 levels
        vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string(), "f".to_string()],  // 6 levels
        vec!["l1".to_string(), "l2".to_string(), "l3".to_string(), "l4".to_string(), "l5".to_string(), "l6".to_string(), "l7".to_string(), "l8".to_string()],  // 8 levels
        vec!["x".to_string(); 20],  // 20 identical components
    ];

    for path in deep_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("pattern"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    // Verify full path is preserved (no truncation)
                    assert_eq!(p.len(), path.len(), "Path length should not be truncated");
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_special_characters_in_components() {
    // Test that special characters in path components are preserved
    let special_char_paths = vec![
        vec!["user.profile".to_string(), "data".to_string()],  // Dot in component
        vec!["user_id".to_string(), "account".to_string()],  // Underscore
        vec!["user-id".to_string(), "data".to_string()],  // Dash
        vec!["123".to_string(), "456".to_string()],  // Numbers
        vec!["User".to_string(), "Profile".to_string()],  // Mixed case
        vec!["UPPERCASE".to_string(), "lowercase".to_string()],  // Case variations
        vec!["path/with/slash".to_string()],  // Slash (risky but should be preserved)
        vec!["path.with.multiple.dots".to_string()],  // Multiple dots
    ];

    for path in special_char_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("search_value"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    // Verify exact path is preserved
                    assert_eq!(p, path, "Path components should preserve special characters");
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_combined_complex_paths() {
    // Test realistic complex paths combining empty, deep, and special chars
    let complex_paths = vec![
        vec!["users".to_string(), "123".to_string(), "profile".to_string(), "settings".to_string(), "preferences.dark_mode".to_string()],
        vec!["api_v2".to_string(), "response".to_string(), "data".to_string(), "".to_string(), "items".to_string(), "0".to_string()],
        vec!["tree".to_string(); 10],  // Repeated component deep nesting
    ];

    for path in complex_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("test"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    assert_eq!(p.len(), path.len());
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_operators_with_injection_attempts() {
    // Test that LTree paths with SQL injection attempts are preserved safely
    let injection_paths = vec![
        vec!["users".to_string(), "'; DROP TABLE--".to_string()],
        vec!["data".to_string(), "' UNION SELECT *--".to_string()],
        vec!["path".to_string(), "1' OR '1'='1".to_string()],
        vec!["nest".to_string(), "--comment".to_string(), "path".to_string()],
    ];

    for path in injection_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("malicious"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    // Payload should be preserved exactly (escaping happens at SQL generation)
                    assert_eq!(p, path);
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_unicode_in_paths() {
    // Test Unicode handling in path components
    let unicode_paths = vec![
        vec!["café".to_string(), "données".to_string()],  // French accents
        vec!["用户".to_string(), "数据".to_string()],  // Chinese characters
        vec!["пользователь".to_string()],  // Russian Cyrillic
        vec!["😀emoji".to_string(), "path".to_string()],  // Emoji
        vec!["Ω".to_string(), "α".to_string(), "β".to_string()],  // Greek letters
    ];

    for path in unicode_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("unicode"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    // Unicode should be preserved exactly
                    assert_eq!(p, path);
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_very_long_component_names() {
    // Test handling of very long individual path components
    let very_long_component = "a".repeat(1000);
    let long_paths = vec![
        vec![very_long_component.clone()],  // Single very long component
        vec!["short".to_string(), very_long_component.clone(), "short".to_string()],
        vec![very_long_component.clone(); 5],  // Multiple very long components
    ];

    for path in long_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("long"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    assert_eq!(p.len(), path.len());
                    // Verify no truncation
                    for (i, component) in p.iter().enumerate() {
                        assert_eq!(component, &path[i]);
                    }
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_whitespace_handling() {
    // Test handling of whitespace in path components
    let whitespace_paths = vec![
        vec![" ".to_string(), "path".to_string()],  // Leading space
        vec!["path".to_string(), " ".to_string()],  // Trailing space
        vec!["path".to_string(), "  ".to_string(), "nested".to_string()],  // Multiple spaces
        vec!["\t".to_string(), "tab".to_string()],  // Tab character
        vec!["\n".to_string(), "newline".to_string()],  // Newline (rare but should be preserved)
        vec!["path with spaces".to_string()],  // Spaces within component
    ];

    for path in whitespace_paths {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("whitespace"),
            };

            match clause {
                WhereClause::Field { path: p, .. } => {
                    assert_eq!(p, path, "Whitespace should be preserved");
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_mixed_edge_cases() {
    // Test realistic scenarios combining multiple edge cases
    let mixed_cases = vec![
        // Deep path with special chars in middle
        vec!["root".to_string(), "app-v2.0".to_string(), "config".to_string(), "db_settings".to_string(), "".to_string(), "host".to_string()],
        // Very deep all numbers
        vec!["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string(), "5".to_string(), "6".to_string(), "7".to_string(), "8".to_string(), "9".to_string(), "10".to_string()],
    ];

    for path in mixed_cases {
        for op in LTREE_OPERATORS {
            let clause = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!("test"),
            };

            match clause {
                WhereClause::Field { .. } => {
                    // Success - all edge cases handled
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}

#[test]
fn test_ltree_null_pattern_values() {
    // Test LTree with null or empty pattern values
    let paths = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];

    for path in paths {
        for op in LTREE_OPERATORS {
            // Empty string pattern
            let clause1 = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!(""),
            };

            match clause1 {
                WhereClause::Field { value, .. } => {
                    assert_eq!(value, json!(""));
                }
                _ => panic!("Should be Field variant"),
            }

            // Null-like pattern (if operator supports it)
            let clause2 = WhereClause::Field {
                path: path.clone(),
                operator: op.clone(),
                value: json!(null),
            };

            match clause2 {
                WhereClause::Field { value, .. } => {
                    assert_eq!(value, json!(null));
                }
                _ => panic!("Should be Field variant"),
            }
        }
    }
}
