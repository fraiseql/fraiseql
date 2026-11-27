//! Flight refuses what `/graphql` refuses, with the same message.
//!
//! Each row builds one server state with one limit or policy set, sends one document to
//! `/graphql` and to the Flight GraphQL seam, and asserts that `/graphql` refused it for
//! the reason the row names and that Flight's refusal carries the same message. The empty
//! fixture schema fails every document at execution on both transports, so a row that
//! did not name its reason would pass on two execution failures.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_arrow::QueryExecutor as _;
use fraiseql_core::{
    graphql::RequestValidator,
    schema::CompiledSchema,
    security::{IntrospectionPolicy, SecurityContext},
    types::UserId,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::Request;
use tower::ServiceExt;

use crate::{
    arrow::policy_seam::PolicyGatedExecutor, routes::graphql::AppState, server::Server,
    server_config::ServerConfig,
};

async fn server() -> Server {
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("a server over an empty schema starts")
}

/// The first error message `/graphql` answers `query` with, if it refused it.
async fn graphql_refusal(
    server: &Server,
    state: &AppState,
    query: &str,
    variables: Option<&serde_json::Value>,
) -> Option<String> {
    let app = server.build_graphql_router(state);
    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "query": query, "variables": variables }).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    body["errors"][0]["message"].as_str().map(str::to_string)
}

fn flight_context() -> SecurityContext {
    let user = fraiseql_core::security::auth_middleware::AuthenticatedUser {
        user_id:      UserId::new("flight-parity-user"),
        scopes:       vec!["user".to_string()],
        expires_at:   fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        email:        None,
        display_name: None,
        extra_claims: std::collections::HashMap::new(),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    };
    SecurityContext::from_user(&user, "req-flight-parity".to_string())
}

/// Send `query` to both transports over `state` and assert that `/graphql` refuses it
/// with a message containing `reason`, and Flight with the same message.
async fn assert_parity(row: &str, server: &Server, state: AppState, query: &str, reason: &str) {
    assert_parity_with(row, server, state, query, None, reason).await;
}

async fn assert_parity_with(
    row: &str,
    server: &Server,
    state: AppState,
    query: &str,
    variables: Option<&serde_json::Value>,
    reason: &str,
) {
    let expected = graphql_refusal(server, &state, query, variables)
        .await
        .unwrap_or_else(|| panic!("{row}: /graphql served the document"));
    assert!(expected.contains(reason), "{row}: /graphql said {expected:?}");
    let flight = PolicyGatedExecutor::new(state)
        .execute_with_security(query, variables, &flight_context())
        .await
        .expect_err("Flight refuses what /graphql refuses");
    assert!(
        flight.to_string().contains(&expected),
        "{row}: /graphql said {expected:?}, Flight said {flight}"
    );
}

#[tokio::test]
async fn flight_refuses_a_document_over_the_depth_limit() {
    let server = server().await;
    let state = server
        .build_app_state()
        .with_validator(RequestValidator::new().with_max_depth(2));
    assert_parity("depth", &server, state, "{ a { b { c { d } } } }", "maximum depth").await;
}

#[tokio::test]
async fn flight_refuses_a_document_over_the_complexity_limit() {
    let server = server().await;
    let state = server
        .build_app_state()
        .with_validator(RequestValidator::new().with_max_complexity(3));
    assert_parity("complexity", &server, state, "{ a b c d e f g h }", "maximum complexity").await;
}

#[tokio::test]
async fn flight_refuses_a_document_over_the_alias_limit() {
    let server = server().await;
    let state = server
        .build_app_state()
        .with_validator(RequestValidator::new().with_max_aliases(1));
    assert_parity("aliases", &server, state, "{ a0: x a1: x a2: x }", "maximum alias count").await;
}

#[tokio::test]
async fn flight_refuses_introspection_the_policy_disables() {
    let server = server().await;
    let state = server
        .build_app_state()
        .with_introspection_policy(IntrospectionPolicy::Disabled);
    assert_parity(
        "introspection",
        &server,
        state,
        "{ __schema { types { name } } }",
        "Introspection queries are disabled",
    )
    .await;
}

#[tokio::test]
async fn flight_refuses_a_variables_payload_that_is_not_an_object() {
    let server = server().await;
    let state = server.build_app_state();
    let variables = serde_json::json!([1, 2]);
    assert_parity_with(
        "variables",
        &server,
        state,
        "{ a }",
        Some(&variables),
        "Variables must be an object",
    )
    .await;
}
