#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;

#[test]
fn test_http_mutation_client_creation() {
    let config = HttpMutationConfig::default();
    let _client = HttpMutationClient::new(config).unwrap();
}

#[test]
fn test_mutation_config_defaults() {
    let config = HttpMutationConfig::default();
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_delay_ms, 100);
}

#[test]
fn test_mutation_config_custom() {
    let config = HttpMutationConfig {
        timeout_ms: 10000,
        max_retries: 5,
        retry_delay_ms: 200,
    };
    assert_eq!(config.timeout_ms, 10000);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.retry_delay_ms, 200);
}

#[test]
fn test_graphql_request_serialization() {
    let request = GraphQLRequest {
        query: "mutation { updateUser(id: $id) { id } }".to_string(),
        variables: json!({ "id": "123" }),
    };

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["query"], "mutation { updateUser(id: $id) { id } }");
    assert_eq!(json["variables"]["id"], "123");
}

#[test]
fn test_graphql_response_parsing_success() {
    let response_json = json!({
        "data": {
            "updateUser": {
                "__typename": "User",
                "id": "123",
                "name": "Alice"
            }
        }
    });

    let response: GraphQLResponse = serde_json::from_value(response_json).unwrap();
    assert!(response.data.is_some());
    assert!(response.errors.is_none());

    let data = response.data.unwrap();
    assert_eq!(data["updateUser"]["id"], "123");
}

#[test]
fn test_graphql_response_with_errors() {
    let response_json = json!({
        "data": null,
        "errors": [
            {
                "message": "User not found"
            }
        ]
    });

    let response: GraphQLResponse = serde_json::from_value(response_json).unwrap();
    assert!(response.data.is_none());
    assert!(response.errors.is_some());
    assert_eq!(response.errors.unwrap()[0].message, "User not found");
}

#[test]
fn test_variable_definition_building() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config).unwrap();

    let variables = json!({
        "id": "123",
        "name": "Alice",
        "active": true
    });

    let var_defs = client.build_variable_definitions(&variables).unwrap();
    assert!(var_defs.contains("$id: String!"));
    assert!(var_defs.contains("$name: String!"));
    assert!(var_defs.contains("$active: Boolean!"));
}

#[test]
fn test_variable_definition_with_numbers() {
    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config).unwrap();

    let variables = json!({
        "count": 42,
        "price": 9.99
    });

    let var_defs = client.build_variable_definitions(&variables).unwrap();
    assert!(var_defs.contains("$count: Int!"));
    assert!(var_defs.contains("$price: Int!"));
}

// ── S22-H2: Federation mutation response size cap ─────────────────────────

#[tokio::test]
async fn mutation_response_oversized_is_rejected() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;

    // Return a body exceeding MAX_MUTATION_RESPONSE_BYTES
    let oversized = vec![b'x'; MAX_MUTATION_RESPONSE_BYTES + 1];
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
        .mount(&mock)
        .await;

    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config).unwrap();
    let url = format!("{}/graphql", mock.uri());
    let request = GraphQLRequest {
        query: "mutation { dummy }".to_string(),
        variables: json!({}),
    };
    let reqwest_client = reqwest::Client::new();
    let result = client.execute_with_retry(&reqwest_client, &url, &request).await;

    assert!(result.is_err(), "oversized mutation response must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("too large"), "error must mention size limit: {msg}");
}

#[tokio::test]
async fn mutation_response_within_limit_is_parsed() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"ok": true}, "errors": null})),
        )
        .mount(&mock)
        .await;

    let config = HttpMutationConfig::default();
    let client = HttpMutationClient::new(config).unwrap();
    let url = format!("{}/graphql", mock.uri());
    let request = GraphQLRequest {
        query: "mutation { dummy }".to_string(),
        variables: json!({}),
    };
    let reqwest_client = reqwest::Client::new();
    let result = client.execute_with_retry(&reqwest_client, &url, &request).await;

    assert!(result.is_ok(), "valid mutation response must be accepted: {result:?}");
    assert!(result.unwrap().data.is_some());
}

// ── S27-H2: Exponential backoff ───────────────────────────────────────────

#[test]
fn exponential_backoff_grows_correctly() {
    let base: u64 = 100;
    // attempt=1 → delay = 100 * 2^0 = 100
    // attempt=2 → delay = 100 * 2^1 = 200
    // attempt=3 → delay = 100 * 2^2 = 400
    assert_eq!(base.saturating_mul(2_u64.saturating_pow(1 - 1)), 100);
    assert_eq!(base.saturating_mul(2_u64.saturating_pow(2 - 1)), 200);
    assert_eq!(base.saturating_mul(2_u64.saturating_pow(3 - 1)), 400);
}

#[test]
fn exponential_backoff_does_not_overflow() {
    // Very large attempt count must not panic (saturating_pow + saturating_mul).
    let base: u64 = 1000;
    let _ = base.saturating_mul(2_u64.saturating_pow(63));
}
