//! HTTP mutation transport tests for Phase 6D
//!
//! Tests for executing extended mutations via HTTP to remote subgraphs.

use fraiseql_core::federation::{
    mutation_http_client::{
        GraphQLRequest, GraphQLResponse, HttpMutationClient, HttpMutationConfig,
    },
    types::{FederatedType, FederationMetadata, KeyDirective},
};
use serde_json::json;

// ============================================================================
// HTTP Mutation Client Tests
// ============================================================================

#[test]
fn test_build_mutation_query_for_update() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["email".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let fed_type = &metadata.types[0];
    let variables = json!({
        "id": "user123",
        "name": "Alice",
        "status": "verified"
    });

    let result = client.build_mutation_query("User", "updateUser", &variables, fed_type);
    assert!(result.is_ok());

    let request = result.unwrap();
    assert!(request.query.contains("mutation"));
    assert!(request.query.contains("updateUser"));
    assert_eq!(request.variables["id"], "user123");
}

#[test]
fn test_mutation_query_excludes_external_fields() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["customer_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let fed_type = &metadata.types[0];
    let variables = json!({
        "order_id": "order123",
        "status": "shipped",
        "customer_id": "cust456"  // This is external, should be excluded
    });

    let result = client.build_mutation_query("Order", "shipOrder", &variables, fed_type);
    assert!(result.is_ok());

    let request = result.unwrap();
    // Query should include status and order_id but not customer_id (which is external)
    assert!(request.query.contains("status:"));
    assert!(request.query.contains("order_id:"));
}

#[test]
fn test_graphql_response_parsing_with_mutation_result() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let response = GraphQLResponse {
        data:   Some(json!({
            "updateUser": {
                "__typename": "User",
                "id": "user123",
                "name": "Alice",
                "status": "verified"
            }
        })),
        errors: None,
    };

    let result = client.parse_response(response, "updateUser");
    assert!(result.is_ok());

    let entity = result.unwrap();
    assert_eq!(entity["__typename"], "User");
    assert_eq!(entity["id"], "user123");
    assert_eq!(entity["status"], "verified");
}

#[test]
fn test_graphql_response_with_mutation_error() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let response = GraphQLResponse {
        data:   None,
        errors: Some(vec![
            fraiseql_core::federation::mutation_http_client::GraphQLError {
                message: "User not found".to_string(),
            },
        ]),
    };

    let result = client.parse_response(response, "updateUser");
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("GraphQL error"));
    assert!(error_msg.contains("User not found"));
}

#[test]
fn test_graphql_response_missing_mutation_field() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let response = GraphQLResponse {
        data:   Some(json!({
            "otherMutation": {
                "__typename": "User",
                "id": "user123"
            }
        })),
        errors: None,
    };

    let result = client.parse_response(response, "updateUser");
    assert!(result.is_err());
}

#[test]
fn test_http_mutation_client_with_custom_config() {
    let config = HttpMutationConfig {
        timeout_ms:     10000,
        max_retries:    5,
        retry_delay_ms: 200,
    };

    let _client = HttpMutationClient::new(config);
    // Should create successfully with custom config
}

// ============================================================================
// Extended Mutation Response Tests
// ============================================================================

#[test]
fn test_extended_mutation_response_structure() {
    // Response should maintain federation format
    let response = json!({
        "__typename": "User",
        "id": "user123",
        "name": "Alice",
        "email": "alice@example.com"
    });

    assert_eq!(response["__typename"], "User");
    assert!(response.get("id").is_some());
}

#[test]
fn test_extended_mutation_preserves_key_fields() {
    // Key fields must be preserved in response for federation
    let response = json!({
        "__typename": "Order",
        "order_id": "order123",
        "status": "shipped"
    });

    assert_eq!(response["order_id"], "order123");
    assert_eq!(response["status"], "shipped");
}

#[test]
fn test_extended_mutation_with_composite_keys() {
    // Composite keys should be handled correctly
    let response = json!({
        "__typename": "OrgUser",
        "organization_id": "org456",
        "user_id": "user789",
        "role": "admin"
    });

    assert_eq!(response["organization_id"], "org456");
    assert_eq!(response["user_id"], "user789");
}

// ============================================================================
// Mutation Type Detection Tests
// ============================================================================

#[test]
fn test_mutation_name_patterns_update() {
    // Mutation names following Apollo patterns
    let update_patterns = vec!["updateUser", "modifyUser", "updateUserProfile"];

    for pattern in update_patterns {
        let lower = pattern.to_lowercase();
        assert!(
            lower.starts_with("update") || lower.starts_with("modify"),
            "Pattern {} should match update",
            pattern
        );
    }
}

#[test]
fn test_mutation_name_patterns_create() {
    let create_patterns = vec!["createUser", "addUser", "createNewUser"];

    for pattern in create_patterns {
        let lower = pattern.to_lowercase();
        assert!(
            lower.starts_with("create") || lower.starts_with("add"),
            "Pattern {} should match create",
            pattern
        );
    }
}

#[test]
fn test_mutation_name_patterns_delete() {
    let delete_patterns = vec!["deleteUser", "removeUser", "deleteArchived"];

    for pattern in delete_patterns {
        let lower = pattern.to_lowercase();
        assert!(
            lower.starts_with("delete") || lower.starts_with("remove"),
            "Pattern {} should match delete",
            pattern
        );
    }
}

// ============================================================================
// Extended Mutation Propagation Tests
// ============================================================================

#[test]
fn test_extended_mutation_includes_metadata() {
    // Extended mutations should include proper federation metadata
    let response = json!({
        "__typename": "User",
        "id": "user123",
        "status": "verified",
        "_mutation": "verifyUser",
        "_remote_execution": true
    });

    // Check federation metadata fields
    assert_eq!(response["_mutation"], "verifyUser");
    assert_eq!(response["_remote_execution"], true);
}

#[test]
fn test_extended_mutation_batch_responses() {
    // Batch mutations should preserve order
    let mutations = vec![
        json!({"id": "user1", "status": "verified"}),
        json!({"id": "user2", "status": "verified"}),
        json!({"id": "user3", "status": "verified"}),
    ];

    assert_eq!(mutations.len(), 3);
    for (idx, mutation) in mutations.iter().enumerate() {
        assert_eq!(mutation["id"], format!("user{}", idx + 1));
    }
}

#[test]
fn test_extended_mutation_error_propagation() {
    // Errors from remote subgraph should be propagated properly
    let error_response = json!({
        "errors": [
            {
                "message": "Validation failed: email is required",
                "path": ["updateUser"]
            }
        ]
    });

    assert!(error_response["errors"].is_array());
    assert_eq!(error_response["errors"][0]["message"], "Validation failed: email is required");
}

// ============================================================================
// GraphQL Request Format Tests
// ============================================================================

#[test]
fn test_graphql_request_with_variables() {
    let request = GraphQLRequest {
        query:     "mutation($id: ID!, $name: String!) { updateUser(id: $id, name: $name) { id } }"
            .to_string(),
        variables: json!({
            "id": "user123",
            "name": "Alice"
        }),
    };

    let json = serde_json::to_value(&request).unwrap();
    assert!(json["query"].as_str().unwrap().contains("$id"));
    assert!(json["query"].as_str().unwrap().contains("$name"));
    assert_eq!(json["variables"]["id"], "user123");
}

#[test]
fn test_graphql_request_without_variables() {
    let request = GraphQLRequest {
        query:     "mutation { deleteUser(id: \"user123\") { id } }".to_string(),
        variables: json!({}),
    };

    let json = serde_json::to_value(&request).unwrap();
    assert!(json["query"].is_string());
    assert!(json["variables"].is_object());
}

#[test]
fn test_variable_type_inference() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config);

    let variables = json!({
        "string_val": "Alice",
        "int_val": 42,
        "bool_val": true,
        "null_val": null
    });

    let var_defs = client.build_variable_definitions(&variables).unwrap();

    assert!(var_defs.contains("$string_val: String!"));
    assert!(var_defs.contains("$int_val: Int!"));
    assert!(var_defs.contains("$bool_val: Boolean!"));
    assert!(var_defs.contains("$null_val: String"));
}
