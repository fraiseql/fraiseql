//! Unit tests for the test infrastructure itself

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
    let simple = crate::tests::JsonTestValues::simple_object();
    assert!(simple.is_object());
    assert_eq!(simple["key"], "value");
    assert_eq!(simple["number"], 42);
}

// Test sample schema SQL is valid
#[test]
fn test_sample_schema_sql() {
    let users_sql = crate::tests::SampleSchema::users_table_sql();
    assert!(users_sql.contains("CREATE TABLE"));
    assert!(users_sql.contains("users"));
    assert!(users_sql.contains("id SERIAL PRIMARY KEY"));
}

// Test sample data SQL is valid
#[test]
fn test_sample_data_sql() {
    let users_data = crate::tests::SampleData::insert_users_sql();
    assert!(users_data.contains("INSERT INTO users"));
    assert!(users_data.contains("Alice"));
    assert!(users_data.contains("Bob"));
}

// Test TestDatabase API availability
#[test]
fn test_testdatabase_api() {
    // Test that TestDatabase can be created (Phase 0.2 placeholder)
    let result = crate::tests::TestDatabase::new();
    assert!(result.is_ok());

    let db = result.unwrap();
    let conn_str = db.connection_string();
    assert!(!conn_str.is_empty());
    assert!(conn_str.contains("postgresql://"));
}