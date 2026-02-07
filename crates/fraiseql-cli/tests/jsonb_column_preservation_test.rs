//! Tests for `jsonb_column` field preservation during schema compilation
//!
//! Verifies that:
//! 1. `jsonb_column` field is preserved during schema compilation
//! 2. `jsonb_column` defaults to "data" when not specified
//! 3. `QueryDefinition` supports `jsonb_column` field
//!
//! Issue #268: fraiseql-cli compile drops `jsonb_column` from queries

use serde_json::json;

#[test]
fn test_jsonb_column_preserved_in_query() {
    // RED: Test that jsonb_column survives schema compilation
    let schema_json = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "email", "type": "String", "nullable": false},
                    {"name": "profile", "type": "String", "nullable": true}
                ]
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_user",
                "jsonb_column": "data"
            }
        ]
    });

    // Parse intermediate schema
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    // Verify IntermediateQuery has jsonb_column
    let intermediate_query = &intermediate.queries[0];
    assert_eq!(intermediate_query.name, "users");
    assert_eq!(intermediate_query.jsonb_column, Some("data".to_string()),
        "IntermediateQuery should preserve jsonb_column field");

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify QueryDefinition has jsonb_column
    let compiled_query = &compiled.queries[0];
    assert_eq!(compiled_query.name, "users");
    assert_eq!(compiled_query.jsonb_column, "data",
        "QueryDefinition should preserve jsonb_column after compilation");
}

#[test]
fn test_jsonb_column_defaults_to_data() {
    // RED: Test that jsonb_column defaults to "data" when not specified
    let schema_json = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "email", "type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_user"
            }
        ]
    });

    // Parse intermediate schema
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    // Verify IntermediateQuery defaults to "data"
    let intermediate_query = &intermediate.queries[0];
    assert_eq!(intermediate_query.name, "users");
    assert_eq!(intermediate_query.jsonb_column, None,
        "IntermediateQuery should have None when not specified (will default during conversion)");

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify QueryDefinition defaults to "data"
    let compiled_query = &compiled.queries[0];
    assert_eq!(compiled_query.name, "users");
    assert_eq!(compiled_query.jsonb_column, "data",
        "QueryDefinition should default jsonb_column to 'data' when not specified");
}

#[test]
fn test_jsonb_column_custom_value() {
    // RED: Test that custom jsonb_column values are preserved
    let schema_json = json!({
        "types": [
            {
                "name": "Order",
                "sql_source": "v_order",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "details", "type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "orders",
                "return_type": "Order",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_order",
                "jsonb_column": "metadata"
            }
        ]
    });

    // Parse intermediate schema
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify custom jsonb_column is preserved
    let compiled_query = &compiled.queries[0];
    assert_eq!(compiled_query.name, "orders");
    assert_eq!(compiled_query.jsonb_column, "metadata",
        "Custom jsonb_column value should be preserved, not default to 'data'");
}

#[test]
fn test_jsonb_column_in_compiled_schema_json() {
    // RED: Test that jsonb_column appears in compiled schema JSON output
    let schema_json = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "data", "type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_user",
                "jsonb_column": "data"
            }
        ]
    });

    // Parse intermediate schema
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Serialize compiled schema to JSON
    let compiled_json = serde_json::to_value(&compiled)
        .expect("Failed to serialize compiled schema");

    // Verify jsonb_column appears in JSON output
    let query_json = &compiled_json["queries"][0];
    assert_eq!(query_json["name"], "users");
    assert_eq!(query_json["jsonb_column"], "data",
        "Compiled schema JSON should include jsonb_column, not null or missing");
}

#[test]
fn test_multiple_queries_with_different_jsonb_columns() {
    // RED: Test that each query can have its own jsonb_column
    let schema_json = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "name", "type": "String", "nullable": false}
                ]
            },
            {
                "name": "Order",
                "sql_source": "v_order",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "details", "type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_user",
                "jsonb_column": "user_data"
            },
            {
                "name": "orders",
                "return_type": "Order",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_order",
                "jsonb_column": "order_metadata"
            }
        ]
    });

    // Parse and convert
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify each query preserves its own jsonb_column
    assert_eq!(compiled.queries[0].name, "users");
    assert_eq!(compiled.queries[0].jsonb_column, "user_data");

    assert_eq!(compiled.queries[1].name, "orders");
    assert_eq!(compiled.queries[1].jsonb_column, "order_metadata");
}
