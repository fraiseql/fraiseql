//! Unit tests for the test infrastructure itself

use crate::common::*;

// Test that the common module exports work
#[test]
fn test_common_module_exports() {
    // This test verifies that the common module exports are accessible
    // If this compiles and runs, the module structure is correct
    assert!(true);
}

// Test basic JSON test values
#[test]
fn test_json_test_values() {
    let simple = JsonTestValues::simple_object();
    assert!(simple.is_object());
    assert_eq!(simple["key"], "value");
    assert_eq!(simple["number"], 42);
}

// Test sample schema SQL is valid
#[test]
fn test_sample_schema_sql() {
    let users_sql = SampleSchema::users_table_sql();
    assert!(users_sql.contains("CREATE TABLE"));
    assert!(users_sql.contains("users"));
    assert!(users_sql.contains("id SERIAL PRIMARY KEY"));
}

// Test sample data SQL is valid
#[test]
fn test_sample_data_sql() {
    let users_data = SampleData::insert_users_sql();
    assert!(users_data.contains("INSERT INTO users"));
    assert!(users_data.contains("Alice"));
    assert!(users_data.contains("Bob"));
}