#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Tests for `jsonb_column` field preservation during schema compilation
//!
//! Verifies that:
//! 1. `jsonb_column` field is preserved during schema compilation
//! 2. `jsonb_column` defaults to "data" when not specified
//! 3. `QueryDefinition` supports `jsonb_column` field
//! 4. a `jsonb_column` other than `data` (or the empty flat-column shape) is refused: the runtime
//!    reads its documents from `data`
//!
//! Issue #268: fraiseql-cli compile drops `jsonb_column` from queries

use serde_json::json;

#[test]
fn test_jsonb_column_preserved_in_query() {
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
    assert_eq!(
        intermediate_query.jsonb_column,
        Some("data".to_string()),
        "IntermediateQuery should preserve jsonb_column field"
    );

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify QueryDefinition has jsonb_column
    let compiled_query = &compiled.queries[0];
    assert_eq!(compiled_query.name, "users");
    assert_eq!(
        compiled_query.jsonb_column, "data",
        "QueryDefinition should preserve jsonb_column after compilation"
    );
}

#[test]
fn test_jsonb_column_defaults_to_data() {
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
    assert_eq!(
        intermediate_query.jsonb_column, None,
        "IntermediateQuery should have None when not specified (will default during conversion)"
    );

    // Convert to compiled schema
    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Verify QueryDefinition defaults to "data"
    let compiled_query = &compiled.queries[0];
    assert_eq!(compiled_query.name, "users");
    assert_eq!(
        compiled_query.jsonb_column, "data",
        "QueryDefinition should default jsonb_column to 'data' when not specified"
    );
}

#[test]
fn test_jsonb_column_other_than_data_is_refused() {
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

    // The runtime reads `data`: a query naming another column is refused, not compiled into
    // an artifact whose first read fails.
    let err = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect_err("`metadata` is not a column the runtime reads");
    let message = format!("{err:#}");
    assert!(message.contains("Query 'orders': jsonb_column = 'metadata'"), "{message}");
}

#[test]
fn test_jsonb_column_in_compiled_schema_json() {
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
    let compiled_json =
        serde_json::to_value(&compiled).expect("Failed to serialize compiled schema");

    // Verify jsonb_column appears in JSON output
    let query_json = &compiled_json["queries"][0];
    assert_eq!(query_json["name"], "users");
    assert_eq!(
        query_json["jsonb_column"], "data",
        "Compiled schema JSON should include jsonb_column, not null or missing"
    );
}

#[test]
fn test_multiple_queries_keep_their_own_jsonb_column() {
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
                "jsonb_column": "data"
            },
            {
                "name": "orders",
                "return_type": "Order",
                "returns_list": true,
                "nullable": false,
                "sql_source": "tv_order",
                "jsonb_column": ""
            }
        ]
    });

    // Parse and convert
    let intermediate_str = serde_json::to_string(&schema_json).unwrap();
    let intermediate: fraiseql_cli::schema::IntermediateSchema =
        serde_json::from_str(&intermediate_str).expect("Failed to parse intermediate schema");

    let compiled = fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("Failed to convert schema");

    // Each query keeps its own jsonb_column: `data`, and the empty flat-column shape
    // federation's `_entities` reads (ADR 0014).
    assert_eq!(compiled.queries[0].name, "users");
    assert_eq!(compiled.queries[0].jsonb_column, "data");

    assert_eq!(compiled.queries[1].name, "orders");
    assert_eq!(compiled.queries[1].jsonb_column, "");
}
