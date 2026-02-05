//! Custom assertion helpers for analytics tests
//!
//! These helpers are shared across multiple test files, so not all may be used by every test.
#![allow(dead_code)]

use serde_json::Value;

/// Assert aggregate result structure
pub fn assert_aggregate_result(result: &Value, query_name: &str) {
    assert!(result["data"].is_object(), "Expected 'data' object");
    assert!(result["data"][query_name].is_array(), "Expected '{}' array", query_name);
}

/// Assert aggregate row has required fields
pub fn assert_aggregate_row_has_fields(row: &Value, fields: &[&str]) {
    for field in fields {
        assert!(row.get(field).is_some(), "Expected field '{}' in row: {:?}", field, row);
    }
}

/// Assert aggregate result count matches expected
pub fn assert_result_count(result: &Value, query_name: &str, expected: usize) {
    let actual = result["data"][query_name].as_array().unwrap().len();
    assert_eq!(actual, expected, "Expected {} results, got {}", expected, actual);
}

/// Assert numeric field value is close (for floating point comparisons)
pub fn assert_numeric_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() < epsilon,
        "Expected {} ± {}, got {}",
        expected,
        epsilon,
        actual
    );
}

/// Assert SQL contains expected clauses
pub fn assert_sql_contains(sql: &str, expected: &[&str]) {
    for clause in expected {
        assert!(
            sql.contains(clause),
            "Expected SQL to contain '{}'\nActual SQL: {}",
            clause,
            sql
        );
    }
}

/// Assert array is sorted in ascending order
pub fn assert_sorted_asc<T: PartialOrd + std::fmt::Debug>(values: &[T]) {
    for i in 1..values.len() {
        assert!(values[i] >= values[i - 1], "Array not sorted ascending: {:?}", values);
    }
}

/// Assert array is sorted in descending order
pub fn assert_sorted_desc<T: PartialOrd + std::fmt::Debug>(values: &[T]) {
    for i in 1..values.len() {
        assert!(values[i] <= values[i - 1], "Array not sorted descending: {:?}", values);
    }
}
