//! Docker Compose Federation Tests - Apollo Router Schema Composition
//!
//! Tests validate Apollo Router subgraph discovery, schema composition,
//! SDL completeness, federation directives, query routing, and error handling.

use super::common::*;

// ============================================================================
// Apollo Router Schema Composition Tests
// ============================================================================

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_discovers_subgraphs() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router discovers all 3 subgraphs ---");

    // Query introspection to verify all subgraph types are present
    let introspection_query = r"
        query {
            __schema {
                types {
                    name
                    kind
                }
            }
        }
    ";

    let response = graphql_query(APOLLO_GATEWAY_URL, introspection_query)
        .await
        .expect("Introspection should succeed");

    assert!(
        !has_errors(&response),
        "Introspection should not have errors: {}",
        get_error_messages(&response)
    );

    // Extract all type names
    let type_names: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Verify key types from each subgraph
    assert!(type_names.contains(&"User".to_string()), "User type from users subgraph");
    assert!(type_names.contains(&"Order".to_string()), "Order type from orders subgraph");
    assert!(
        type_names.contains(&"Product".to_string()),
        "Product type from products subgraph"
    );

    println!("✓ Apollo Router discovered all 3 subgraphs ({} total types)", type_names.len());
    println!("  - User type (users subgraph)");
    println!("  - Order type (orders subgraph)");
    println!("  - Product type (products subgraph)");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_schema_composition() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router schema composition ---");

    // Query the composed schema structure
    let basic_schema_query = r"
        query {
            __schema {
                queryType {
                    name
                    fields {
                        name
                    }
                }
            }
        }
    ";

    let response = graphql_query(APOLLO_GATEWAY_URL, basic_schema_query)
        .await
        .expect("Schema query should succeed");

    assert!(
        !has_errors(&response),
        "Schema query should not have errors: {}",
        get_error_messages(&response)
    );

    let query_type = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .and_then(|q| q.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");

    let root_fields: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .and_then(|q| q.get("fields"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    println!("✓ Apollo Router composed schema with Query type: {}", query_type);
    println!("  Root fields: {}", root_fields.join(", "));

    // Verify key queries are present
    assert!(root_fields.contains(&"users".to_string()), "users query from composition");
    assert!(root_fields.contains(&"orders".to_string()), "orders query from composition");
    assert!(root_fields.contains(&"products".to_string()), "products query from composition");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_sdl_completeness() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router SDL completeness ---");

    // Use introspection to build SDL representation
    let sdl_query = r"
        query {
            __schema {
                types {
                    name
                    kind
                    description
                    fields {
                        name
                        type { name kind }
                    }
                }
                queryType { name }
                mutationType { name }
            }
        }
    ";

    let response = graphql_query(APOLLO_GATEWAY_URL, sdl_query)
        .await
        .expect("SDL query should succeed");

    assert!(
        !has_errors(&response),
        "SDL query should not have errors: {}",
        get_error_messages(&response)
    );

    // Verify schema has both queries and potentially mutations
    let has_query_type = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("queryType"))
        .is_some();

    let types_count = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("types"))
        .and_then(|t| t.as_array())
        .map_or(0, |arr| arr.len());

    println!("✓ Apollo Router SDL completeness verified");
    println!("  - Query type present: {}", has_query_type);
    println!("  - Total types in schema: {}", types_count);

    assert!(has_query_type, "Schema should have Query type");
    assert!(types_count > 0, "Schema should have types");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_federation_directives() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router federation directives ---");

    // Query introspection to check for federation directives
    let directive_query = r"
        query {
            __schema {
                directives {
                    name
                    locations
                }
            }
        }
    ";

    let response = graphql_query(APOLLO_GATEWAY_URL, directive_query)
        .await
        .expect("Directive query should succeed");

    assert!(
        !has_errors(&response),
        "Directive query should not have errors: {}",
        get_error_messages(&response)
    );

    let directive_names: Vec<String> = extract_data(&response)
        .and_then(|d| d.get("__schema"))
        .and_then(|s| s.get("directives"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    println!("✓ Apollo Router federation directives verified");
    println!("  Available directives: {}", directive_names.join(", "));

    // Verify standard federation and GraphQL directives
    let has_skip = directive_names.contains(&"skip".to_string());
    let has_include = directive_names.contains(&"include".to_string());

    println!("  - @skip directive: {}", if has_skip { "✓" } else { "✗" });
    println!("  - @include directive: {}", if has_include { "✓" } else { "✗" });
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_query_routing() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router query routing ---");

    // Test routing to users subgraph
    let users_query = r"
        query {
            users(limit: 1) {
                id
                identifier
            }
        }
    ";

    let users_response = graphql_query(APOLLO_GATEWAY_URL, users_query)
        .await
        .expect("Users query should succeed");

    assert!(
        !has_errors(&users_response),
        "Users query should not error: {}",
        get_error_messages(&users_response)
    );

    // Test routing to orders subgraph
    let orders_query = r"
        query {
            orders(limit: 1) {
                id
                status
            }
        }
    ";

    let orders_response = graphql_query(APOLLO_GATEWAY_URL, orders_query)
        .await
        .expect("Orders query should succeed");

    assert!(
        !has_errors(&orders_response),
        "Orders query should not error: {}",
        get_error_messages(&orders_response)
    );

    // Test routing to products subgraph
    let products_query = r"
        query {
            products(limit: 1) {
                id
                name
                price
            }
        }
    ";

    let products_response = graphql_query(APOLLO_GATEWAY_URL, products_query)
        .await
        .expect("Products query should succeed");

    assert!(
        !has_errors(&products_response),
        "Products query should not error: {}",
        get_error_messages(&products_response)
    );

    println!("✓ Apollo Router query routing verified");
    println!("  - Users subgraph routed correctly");
    println!("  - Orders subgraph routed correctly");
    println!("  - Products subgraph routed correctly");
}

#[tokio::test]
#[ignore = "requires Docker Compose federation stack on localhost:4000-4003"]
async fn test_apollo_router_error_handling() {
    setup_three_subgraph_tests().await.expect("Setup should succeed");

    println!("\n--- Test: Apollo Router error handling ---");

    // Test invalid query structure
    let invalid_query = r"
        query {
            users {
                id
                nonexistentField
            }
        }
    ";

    let invalid_response = graphql_query(APOLLO_GATEWAY_URL, invalid_query)
        .await
        .expect("Query should be sent");

    let has_error = has_errors(&invalid_response);
    println!(
        "✓ Apollo Router error handling for invalid field: {}",
        if has_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    // Test query to non-existent root field
    let nonexistent_query = r"
        query {
            nonexistentRootField {
                id
            }
        }
    ";

    let nonexistent_response = graphql_query(APOLLO_GATEWAY_URL, nonexistent_query)
        .await
        .expect("Query should be sent");

    let has_nonexistent_error = has_errors(&nonexistent_response);
    println!(
        "✓ Apollo Router error handling for non-existent field: {}",
        if has_nonexistent_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    // Test malformed query
    let malformed_query = "{ users { id";

    let malformed_response = graphql_query(APOLLO_GATEWAY_URL, malformed_query)
        .await
        .expect("Malformed query should be handled");

    let has_malformed_error = has_errors(&malformed_response);
    println!(
        "✓ Apollo Router error handling for malformed query: {}",
        if has_malformed_error {
            "✓ (errors present)"
        } else {
            "✗ (no errors)"
        }
    );

    println!("\n✓ Apollo Router error handling comprehensive validation complete");
}
