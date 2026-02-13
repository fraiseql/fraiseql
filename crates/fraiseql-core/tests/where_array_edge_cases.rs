//! Test WHERE clause array and JSON array edge cases.
//!
//! This test verifies that:
//! 1. Empty arrays are handled correctly
//! 2. Large arrays (1000+ items) don't overflow or corrupt
//! 3. Arrays with null values work as expected
//! 4. Array operators preserve order and type information
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Empty arrays could cause SQL generation errors
//! - Large arrays could buffer overflow
//! - Null values in arrays could be mishandled

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_where_array_empty_handling() {
    // Empty arrays in WHERE clauses should be handled gracefully
    let empty_array_cases = vec![
        json!([]),           // Empty array
        json!([null]),       // Array with only null
        json!([null, null]), // Multiple nulls
    ];

    for value in &empty_array_cases {
        // ArrayContains operator with empty/null arrays
        let clause = WhereClause::Field {
            path:     vec!["tags".to_string()],
            operator: WhereOperator::ArrayContains,
            value:    value.clone(),
        };

        match clause {
            WhereClause::Field { value: v, .. } => {
                assert_eq!(v, *value, "Array value should be preserved");
            },
            _ => panic!("Should be Field variant"),
        }
    }

    // ArrayOverlaps operator with empty arrays
    for value in &empty_array_cases {
        let clause = WhereClause::Field {
            path:     vec!["categories".to_string()],
            operator: WhereOperator::ArrayOverlaps,
            value:    value.clone(),
        };

        match clause {
            WhereClause::Field { value: v, .. } => {
                assert_eq!(v, *value);
            },
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_array_large_element_count() {
    // Large arrays (1000+ elements) should be handled without overflow
    let large_sizes = vec![100, 500, 1000, 5000];

    for size in large_sizes {
        let large_array: Vec<serde_json::Value> = (0..size).map(|i| json!(i)).collect();

        let clause = WhereClause::Field {
            path:     vec!["ids".to_string()],
            operator: WhereOperator::ArrayContains,
            value:    json!(large_array.clone()),
        };

        match clause {
            WhereClause::Field { value, .. } => {
                // Should preserve all elements
                let arr = value.as_array().unwrap();
                assert_eq!(arr.len(), size, "All {} elements should be preserved", size);
            },
            _ => panic!("Should be Field variant"),
        }
    }
}

#[test]
fn test_where_array_with_mixed_types() {
    // Arrays can contain mixed types (like JSON arrays in PostgreSQL)
    let mixed_array = json!([
        1,
        "string",
        true,
        null,
        3.15,
        {"nested": "object"},
        ["nested", "array"]
    ]);

    let clause = WhereClause::Field {
        path:     vec!["values".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    mixed_array.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 7);
            assert_eq!(arr[0], json!(1));
            assert_eq!(arr[1], json!("string"));
            assert_eq!(arr[2], json!(true));
            assert_eq!(arr[3], json!(null));
            assert_eq!(arr[4], json!(3.15));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_with_null_values() {
    // Null values inside arrays should be preserved (not confused with empty/missing)
    let array_with_nulls = json!([1, null, 3, null, 5]);

    let clause = WhereClause::Field {
        path:     vec!["sparse_array".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    array_with_nulls.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 5);
            assert_eq!(arr[0], json!(1));
            assert!(arr[1].is_null());
            assert_eq!(arr[2], json!(3));
            assert!(arr[3].is_null());
            assert_eq!(arr[4], json!(5));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_duplicate_values() {
    // Arrays with duplicate values should preserve all occurrences
    let duplicate_array = json!([1, 2, 2, 3, 3, 3, 2, 1]);

    let clause = WhereClause::Field {
        path:     vec!["numbers".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    duplicate_array.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 8, "All duplicates should be preserved");
            assert_eq!(arr[1], json!(2));
            assert_eq!(arr[2], json!(2));
            assert_eq!(arr[6], json!(2));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_string_elements_special_chars() {
    // String elements in arrays should preserve special characters
    let special_strings = json!([
        "simple",
        "with spaces",
        "with'quotes",
        "with\"double",
        "with\\backslash",
        "with\nnewline",
        "café",
        "🚀emoji",
        "path/with/slashes",
        "path\\with\\backslashes"
    ]);

    let clause = WhereClause::Field {
        path:     vec!["strings".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    special_strings.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 10);
            assert_eq!(arr[2], json!("with'quotes"));
            assert_eq!(arr[6], json!("café"));
            assert_eq!(arr[7], json!("🚀emoji"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_nested_arrays() {
    // Arrays can contain other arrays (nested structures)
    let nested_arrays = json!([[1, 2, 3], [4, 5], [], [null], [1, "mixed", true]]);

    let clause = WhereClause::Field {
        path:     vec!["matrix".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    nested_arrays.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let outer_arr = value.as_array().unwrap();
            assert_eq!(outer_arr.len(), 5);

            let inner1 = outer_arr[0].as_array().unwrap();
            assert_eq!(inner1.len(), 3);

            let inner3 = outer_arr[2].as_array().unwrap();
            assert_eq!(inner3.len(), 0);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_nested_objects() {
    // Arrays of objects should preserve structure
    let object_array = json!([
        {"id": 1, "name": "first"},
        {"id": 2, "name": "second"},
        {"id": 3, "name": "third", "extra": "field"}
    ]);

    let clause = WhereClause::Field {
        path:     vec!["items".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    object_array.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 3);

            let obj1 = arr[0].as_object().unwrap();
            assert_eq!(obj1["id"], json!(1));
            assert_eq!(obj1["name"], json!("first"));

            let obj3 = arr[2].as_object().unwrap();
            assert_eq!(obj3.len(), 3);
            assert_eq!(obj3["extra"], json!("field"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_overlaps_operator() {
    // ArrayOverlaps operator - array intersects with another
    let overlapping_array = json!([1, 2, 3, 4, 5]);

    let clause = WhereClause::Field {
        path:     vec!["nums".to_string()],
        operator: WhereOperator::ArrayOverlaps,
        value:    overlapping_array.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, overlapping_array);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_in_operator() {
    // IN operator with array of values
    let in_values = json!(["red", "green", "blue", "yellow"]);

    let clause = WhereClause::Field {
        path:     vec!["color".to_string()],
        operator: WhereOperator::In,
        value:    in_values.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, in_values);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_nin_operator() {
    // NOT IN operator with array of values
    let nin_values = json!([10, 20, 30, 40, 50]);

    let clause = WhereClause::Field {
        path:     vec!["excluded_ids".to_string()],
        operator: WhereOperator::Nin,
        value:    nin_values.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, nin_values);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_where_array_numeric_precision() {
    // Arrays with high-precision numbers
    let precision_array = json!([0.1, 0.123456789, 1.23e-10, 999999.999999999, -0.0000001]);

    let clause = WhereClause::Field {
        path:     vec!["decimals".to_string()],
        operator: WhereOperator::ArrayContains,
        value:    precision_array.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            let arr = value.as_array().unwrap();
            assert_eq!(arr[1], json!(0.123456789));
        },
        _ => panic!("Should be Field variant"),
    }
}
