//! Integration tests for `HttpEntityResolver`.
//!
//! Uses `wiremock` to stand up a local mock HTTP server, exercising the full
//! HTTP request/response cycle without touching a real subgraph.
//!
//! Tests that verify SSRF blocking use the production `new()` constructor.
//! Tests that need to contact the local wiremock server use `new_for_test()`,
//! which skips URL validation (available with the `test-utils` feature only).
//!
//! Run with: `cargo test -p fraiseql-federation --features test-utils`

// This file only compiles (and therefore runs) when the `test-utils` feature is
// enabled, since `HttpEntityResolver::new_for_test` is gated on that feature.
#![cfg(feature = "test-utils")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use fraiseql_federation::{
    EntityRepresentation, HttpClientConfig, HttpEntityResolver, selection_parser::FieldSelection,
};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn make_repr(typename: &str, id: &str) -> EntityRepresentation {
    let mut key_fields = HashMap::new();
    key_fields.insert("id".to_string(), json!(id));
    let mut all_fields = key_fields.clone();
    all_fields.insert("__typename".to_string(), json!(typename));

    EntityRepresentation {
        typename: typename.to_string(),
        key_fields,
        all_fields,
    }
}

fn field_selection(fields: &[&str]) -> FieldSelection {
    FieldSelection {
        fields: fields.iter().map(|&s| s.to_string()).collect(),
    }
}

/// Test resolver that skips SSRF validation (for contacting local mock servers).
fn test_resolver() -> HttpEntityResolver {
    HttpEntityResolver::new_for_test(HttpClientConfig::default()).unwrap()
}

// ── Happy path ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_resolves_entity_from_subgraph_200_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "_entities": [
                    {"__typename": "User", "id": "u1", "email": "alice@example.com"}
                ]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    let results = test_resolver()
        .resolve_entities(&url, &[make_repr("User", "u1")], &field_selection(&["id", "email"]))
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    let entity = results[0].as_ref().unwrap();
    assert_eq!(entity["email"], "alice@example.com");
}

#[tokio::test]
async fn test_entity_resolution_graphql_error_is_propagated() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": "Entity not found"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    let result = test_resolver()
        .resolve_entities(&url, &[make_repr("User", "missing")], &field_selection(&["id"]))
        .await;

    assert!(result.is_err(), "GraphQL errors must surface as an Err");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Entity not found") || msg.contains("GraphQL"), "{msg}");
}

#[tokio::test]
async fn test_entity_resolution_http_error_bubbles() {
    let server = MockServer::start().await;

    // Return HTTP 503 — resolver should exhaust retries and return Err.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());

    // Use a fast-retry config to keep tests quick.
    let config = HttpClientConfig {
        timeout_ms: 5000,
        max_retries: 2,
        retry_delay_ms: 1,
    };
    let result = HttpEntityResolver::new_for_test(config)
        .unwrap()
        .resolve_entities(&url, &[make_repr("User", "u1")], &field_selection(&["id"]))
        .await;

    assert!(result.is_err(), "HTTP errors must surface as Err after retries exhausted");
}

#[tokio::test]
async fn test_entity_batch_respects_representation_count() {
    let server = MockServer::start().await;

    // Return exactly 3 entities matching the 3 input representations.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "_entities": [
                    {"__typename": "User", "id": "u1"},
                    {"__typename": "User", "id": "u2"},
                    {"__typename": "User", "id": "u3"},
                ]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    let reps = vec![
        make_repr("User", "u1"),
        make_repr("User", "u2"),
        make_repr("User", "u3"),
    ];
    let results = test_resolver()
        .resolve_entities(&url, &reps, &field_selection(&["id"]))
        .await
        .unwrap();

    assert_eq!(results.len(), 3, "result count must equal input representation count");
}

// ── SSRF guard at the HTTP layer ──────────────────────────────────────────────

#[test]
fn test_entity_resolution_ssrf_blocked_loopback_ip() {
    // Production resolver (SSRF active) — loopback IP must be rejected.
    let blocked_url = "https://127.0.0.1:9999/graphql";
    let reps = vec![make_repr("User", "u1")];
    let sel = field_selection(&["id"]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        HttpEntityResolver::new(HttpClientConfig::default())
            .unwrap()
            .resolve_entities(blocked_url, &reps, &sel)
            .await
    });

    assert!(result.is_err(), "loopback IP must be rejected by SSRF guard");
}
