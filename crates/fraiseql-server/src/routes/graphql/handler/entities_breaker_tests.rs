//! The per-entity circuit breaker is consulted for every `_entities` request: one whose
//! selected operation selects `_entities`.

use std::sync::Arc;

use axum::Router;
use fraiseql_core::schema::{CircuitBreakerConfig, FederationConfig, FederationEntity};
use serde_json::{Value, json};

use super::http_cache::{Sent, principal, router_with, send};
use crate::federation::FederationCircuitBreakerManager;

/// A federated router whose breaker for `User` is open.
fn user_breaker_open() -> Router {
    let federation = FederationConfig {
        enabled: true,
        entities: vec![FederationEntity {
            name: "User".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        circuit_breaker: Some(CircuitBreakerConfig {
            enabled:               true,
            failure_threshold:     1,
            recovery_timeout_secs: 600,
            success_threshold:     1,
            per_entity:            Vec::new(),
        }),
        ..Default::default()
    };
    let breaker = FederationCircuitBreakerManager::from_config(&federation).unwrap();
    breaker.record_failure("User");
    assert!(breaker.check("User").is_some(), "the breaker for `User` is open");
    router_with(
        |schema| schema.federation = Some(federation),
        |state| state.with_circuit_breaker(Arc::clone(&breaker)),
    )
    .0
}

fn refused_by_the_breaker(sent: &Sent) -> bool {
    sent.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|m| m.contains("'User' is temporarily unavailable"))
}

async fn post(body: Value) -> Sent {
    send(&user_breaker_open(), "POST", "/graphql", Some(body), &[], Some(principal())).await
}

const BY_REPRESENTATIONS: &str = "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id } } }";

#[tokio::test]
async fn an_entities_request_meets_the_open_breaker() {
    let body = json!({"query": BY_REPRESENTATIONS, "variables": {
        "representations": [{"__typename": "User", "id": 1}],
    }});
    let sent = post(body).await;
    assert!(refused_by_the_breaker(&sent), "{}", sent.body);
}

/// Two operations, one of them `_entities`: the one `operationName` selects decides.
const TWO_OPERATIONS: &str = "query Users { users { id } } \
     query Entities($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id } } }";

#[tokio::test]
async fn a_selected_entities_operation_meets_the_open_breaker() {
    let body = json!({"query": TWO_OPERATIONS, "operationName": "Entities", "variables": {
        "representations": [{"__typename": "User", "id": 1}],
    }});
    let sent = post(body).await;
    assert!(refused_by_the_breaker(&sent), "{}", sent.body);
}

#[tokio::test]
async fn a_selected_operation_without_entities_is_not_refused() {
    let body = json!({"query": TWO_OPERATIONS, "operationName": "Users", "variables": {
        "representations": [{"__typename": "User", "id": 1}],
    }});
    let sent = post(body).await;
    assert!(!refused_by_the_breaker(&sent), "{}", sent.body);
    assert_eq!(sent.body["data"]["users"][0]["id"], json!(1), "{}", sent.body);
}
